//! Bubblewrap sandboxing wrapper for Gemini CLI

use anyhow::{Context, Result};
use bwrap_core::{HomeAccessMode, NetworkMode, SandboxBuilder, SandboxConfig, ToolConfig};
use bwrap_proxy::{ConfigLoader, ProxyServer, ProxyServerConfig};
use clap::Parser;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Parser)]
#[command(
    name = "bw-gemini",
    about = "Bubblewrap sandboxing wrapper for Gemini CLI",
    version
)]
struct Args {
    /// Disable network access (default: network enabled)
    #[arg(long)]
    no_network: bool,

    /// Allow full home directory access (default: safe dirs only)
    #[arg(long)]
    full_home_access: bool,

    /// Print sandbox configuration and bwrap command to stderr
    #[arg(long, short)]
    verbose: bool,

    /// Launch an interactive shell in the sandbox (for debugging)
    #[arg(long)]
    shell: bool,

    /// Mount additional read-only path (can be used multiple times)
    #[arg(long = "allow-ro", value_name = "PATH")]
    allow_ro_paths: Vec<PathBuf>,

    /// Mount additional read-write path (can be used multiple times)
    #[arg(long = "allow-rw", value_name = "PATH")]
    allow_rw_paths: Vec<PathBuf>,

    /// Set working directory in sandbox (default: current directory)
    #[arg(long, value_name = "PATH")]
    dir: Option<PathBuf>,

    /// Pass an environment variable into the sandbox (can be used multiple times)
    #[arg(long = "pass-env", value_name = "VAR_NAME")]
    pass_env_vars: Vec<String>,

    /// Enable filtered proxy mode for fine-grained network control
    #[arg(long)]
    use_filter_proxy: bool,

    /// Proxy configuration file (TOML format)
    #[arg(long, value_name = "PATH")]
    proxy_config: Option<PathBuf>,

    /// Path to bw-relay binary (for filtered proxy mode)
    #[arg(long, value_name = "PATH")]
    bw_relay_path: Option<PathBuf>,

    /// Gemini arguments (use -- to separate from bw-gemini options)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    cli_args: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(if args.verbose { "debug" } else { "warn" })
        .with_writer(std::io::stderr)
        .init();

    // Get Gemini CLI path
    let gemini_path = get_gemini_path()?;

    // Build tool configuration
    let tool_config = ToolConfig {
        name: "gemini".to_string(),
        cli_path: gemini_path,
        home_dot_file: None,
        default_args: vec![],
        cli_args: args.cli_args,
        help_text: "Gemini arguments are passed through unchanged.\n\nFor authentication, you may need to pass environment variables into the sandbox.\nUse the --pass-env argument for each variable you need."
            .to_string(),
    };

    // Determine target directory
    let target_dir = if let Some(dir) = args.dir {
        dir.canonicalize()
            .context("Failed to canonicalize target directory")?
    } else {
        env::current_dir().context("Failed to get current directory")?
    };

    // Handle proxy lifecycle if needed
    // The proxy runs as an async task spawned within create_proxy_task
    let network_mode = if args.use_filter_proxy {
        let socket_path = create_proxy_task(&args.proxy_config, args.verbose).await?;
        NetworkMode::Filtered {
            proxy_socket: socket_path,
            allowed_domains: vec![],
        }
    } else if args.no_network {
        NetworkMode::Disabled
    } else {
        NetworkMode::Enabled
    };

    // Build sandbox configuration
    let config = SandboxConfig {
        tool_name: "gemini".to_string(),
        tool_config,
        target_dir,
        network_mode,
        home_access: if args.full_home_access {
            HomeAccessMode::Full
        } else {
            HomeAccessMode::Safe
        },
        additional_ro_paths: args.allow_ro_paths,
        additional_rw_paths: args.allow_rw_paths,
        env_vars: HashMap::new(),
        pass_through_env: args.pass_env_vars,
        verbose: args.verbose,
        shell: args.shell,
        bw_relay_path: args.bw_relay_path,
    };

    // Build and execute sandbox
    let sandbox = SandboxBuilder::new(config)
        .context("Failed to create sandbox builder")?
        .build()
        .context("Failed to build sandbox")?;

    let status = sandbox.exec().context("Failed to execute sandbox")?;

    std::process::exit(status.code().unwrap_or(1))
}

fn get_gemini_path() -> Result<PathBuf> {
    let home = env::var("HOME").context("HOME environment variable not set")?;
    let path = PathBuf::from(&home).join(".local").join("bin").join("gemini");

    if path.exists() {
        return Ok(path);
    }

    // Fallback: check if gemini is in PATH
    if let Ok(output) = std::process::Command::new("which").arg("gemini").output() {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout);
            let path = PathBuf::from(path_str.trim());
            if path.exists() {
                return Ok(path);
            }
        }
    }

    anyhow::bail!(
        "Gemini CLI not found at {:?} or in PATH",
        PathBuf::from(home).join(".local/bin/gemini")
    );
}

/// Create and spawn the proxy server as an async task
async fn create_proxy_task(config_path: &Option<PathBuf>, _verbose: bool) -> Result<PathBuf> {
    // Generate a unique socket path in /tmp
    let session_id = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .context("Failed to get system time")?
        .as_nanos();
    let socket_path = PathBuf::from(format!("/tmp/bw-gemini-proxy-{}.sock", session_id));

    // Load configuration
    let config = ConfigLoader::load_or_default(config_path.clone())
        .context("Failed to load proxy configuration")?;

    // Create proxy server
    let proxy_config = ProxyServerConfig {
        socket_path: socket_path.clone(),
        network_config: Arc::new(config.network),
        policy_engine: None, // Default to open mode
        learning_recorder: None,
    };

    let proxy = ProxyServer::new(proxy_config);

    // Spawn as async task (will run until bw-gemini exits)
    tokio::spawn(async move {
        if let Err(e) = proxy.start().await {
            tracing::error!("Proxy server error: {}", e);
        }
    });

    // Wait a bit for the socket to be created
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    Ok(socket_path)
}
