//! Bubblewrap sandboxing wrapper for Gemini CLI

use anyhow::{Context, Result};
use bwrap_core::{CommonArgs, HomeAccessMode, NetworkMode, SandboxBuilder, SandboxConfig, ToolConfig};
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
    #[command(flatten)]
    common: CommonArgs,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging - only if BW_LOG env var or verbose flag
    let _ = if args.common.verbose || env::var("BW_LOG").is_ok() {
        let filter = env::var("BW_LOG").unwrap_or_else(|_| "debug".to_string());
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .try_init()
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::ERROR)
            .with_writer(std::io::stderr)
            .try_init()
    };

    // Get Gemini CLI path
    let gemini_path = get_gemini_path()?;

    // Build tool configuration
    let tool_config = ToolConfig {
        name: "gemini".to_string(),
        cli_path: gemini_path,
        home_dot_file: None,
        default_args: vec![],
        cli_args: args.common.cli_args,
        help_text: "Gemini arguments are passed through unchanged.\n\nFor authentication, you may need to pass environment variables into the sandbox.\nUse the --pass-env argument for each variable you need."
            .to_string(),
    };

    // Determine target directory
    let target_dir = if let Some(dir) = args.common.dir {
        dir.canonicalize()
            .context("Failed to canonicalize target directory")?
    } else {
        env::current_dir().context("Failed to get current directory")?
    };

    // Handle network mode and proxy
    // --proxy and --no-network are mutually exclusive
    let network_mode = if args.common.proxy && args.common.no_network {
        anyhow::bail!("--proxy and --no-network are mutually exclusive");
    } else if args.common.proxy {
        // --proxy enables filtered network with SOCKS5
        let socket_path = create_proxy_task(&args.common.proxy_config, args.common.verbose).await?;
        NetworkMode::Filtered {
            proxy_socket: socket_path,
            allowed_domains: vec![],
        }
    } else if args.common.no_network {
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
        home_access: if args.common.full_home_access {
            HomeAccessMode::Full
        } else {
            HomeAccessMode::Safe
        },
        additional_ro_paths: args.common.allow_ro_paths,
        additional_rw_paths: args.common.allow_rw_paths,
        env_vars: HashMap::new(),
        pass_through_env: args.common.pass_env_vars,
        verbose: args.common.verbose,
        shell: args.common.shell,
        bw_relay_path: args.common.bw_relay_path,
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
