//! Bubblewrap sandboxing wrapper for Claude CLI

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
    name = "bw-claude",
    about = "Bubblewrap sandboxing wrapper for Claude CLI",
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

    /// Disable --dangerously-skip-permissions for Claude
    #[arg(long)]
    no_skip_permissions: bool,

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

    /// Claude arguments (use -- to separate from bw-claude options)
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

    // Get Claude CLI path
    let claude_path = get_claude_path()?;

    // Build tool configuration
    let tool_config = ToolConfig {
        name: "claude".to_string(),
        cli_path: claude_path,
        home_dot_file: Some(".claude.json".to_string()),
        default_args: if !args.no_skip_permissions {
            vec!["--dangerously-skip-permissions".to_string()]
        } else {
            vec![]
        },
        cli_args: args.cli_args,
        help_text: "Claude-specific options:\n  By default, --dangerously-skip-permissions is passed to Claude.\n  Use --no-skip-permissions to disable this behavior."
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
        tool_name: "claude".to_string(),
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

fn get_claude_path() -> Result<PathBuf> {
    let home = env::var("HOME").context("HOME environment variable not set")?;
    let path = PathBuf::from(home)
        .join(".claude")
        .join("local")
        .join("claude");

    if !path.exists() {
        anyhow::bail!("Claude CLI not found at {:?}", path);
    }

    Ok(path)
}

/// Create and spawn the proxy server as an async task
async fn create_proxy_task(config_path: &Option<PathBuf>, _verbose: bool) -> Result<PathBuf> {
    // Generate a unique socket path in /tmp
    let session_id = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .context("Failed to get system time")?
        .as_nanos();
    let socket_path = PathBuf::from(format!("/tmp/bw-claude-proxy-{}.sock", session_id));

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

    // Spawn as async task (will run until bw-claude exits)
    tokio::spawn(async move {
        if let Err(e) = proxy.start().await {
            tracing::error!("Proxy server error: {}", e);
        }
    });

    // Wait a bit for the socket to be created
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    Ok(socket_path)
}
