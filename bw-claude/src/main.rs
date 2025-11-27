//! Bubblewrap sandboxing wrapper for Claude CLI

use anyhow::{Context, Result};
use bwrap_core::{CommonArgs, create_proxy_task, HomeAccessMode, NetworkMode, SandboxBuilder, SandboxConfig, ToolConfig};
use clap::Parser;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "bw-claude",
    about = "Bubblewrap sandboxing wrapper for Claude CLI",
    version
)]
struct Args {
    /// Disable --dangerously-skip-permissions for Claude
    #[arg(long)]
    no_skip_permissions: bool,

    #[command(flatten)]
    common: CommonArgs,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging - only if BW_LOG env var or verbose flag
    let _ = if args.common.verbose || env::var("BW_LOG").is_ok() {
        let filter = env::var("BW_LOG").unwrap_or_else(|_| "info".to_string());
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

    // Get Claude CLI path
    let claude_path = get_claude_path()?;

    // Handle network mode and proxy first (while args.common is still intact)
    // --proxy or --policy implies --no-network (disables direct network, forces proxy-only)
    let (network_mode, _proxy_socket) = determine_network_mode(
        &args.common,
        &args.common.proxy_config,
    )
    .await?;

    // Determine target directory (use reference to avoid moving)
    let target_dir = if let Some(dir) = args.common.dir.as_ref() {
        dir.canonicalize()
            .context("Failed to canonicalize target directory")?
    } else {
        env::current_dir().context("Failed to get current directory")?
    };

    // Build tool configuration (now we can move cli_args)
    let tool_config = ToolConfig {
        name: "claude".to_string(),
        cli_path: claude_path,
        home_dot_file: Some(".claude.json".to_string()),
        default_args: if !args.no_skip_permissions {
            vec!["--dangerously-skip-permissions".to_string()]
        } else {
            vec![]
        },
        cli_args: args.common.cli_args,
        help_text: "Claude-specific options:\n  By default, --dangerously-skip-permissions is passed to Claude.\n  Use --no-skip-permissions to disable this behavior."
            .to_string(),
    };

    // Build sandbox configuration
    // Note: bw-relay sets proxy env vars, so we don't set them here
    let config = SandboxConfig {
        tool_name: "claude".to_string(),
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

/// Determine the network mode based on CLI flags
/// --policy or --learn enables HTTP CONNECT proxy with filtering
async fn determine_network_mode(
    common: &CommonArgs,
    proxy_config: &Option<PathBuf>,
) -> Result<(NetworkMode, Option<PathBuf>)> {
    // Check if proxy should be enabled
    let use_proxy = common.policy.is_some() || common.learn.is_some();

    if use_proxy {
        // Determine policy name:
        // - If --learn is specified, use "open" (allow all, but record)
        // - Otherwise, use --policy if specified, default to "open"
        let policy_name = if common.learn.is_some() {
            "open"
        } else {
            common.policy.as_ref().map(|s| s.as_str()).unwrap_or("open")
        };

        // Learning output: set if --learn is specified
        let learning_output = common.learn.as_ref();

        // Create proxy task with policy and learning configuration
        let socket_path = create_proxy_task(
            proxy_config,
            Some(policy_name),
            learning_output,
        )
        .await?;

        let network_mode = NetworkMode::Filtered {
            proxy_socket: socket_path.clone(),
            policy_name: policy_name.to_string(),
            learning_output: common.learn.clone(),
            allowed_domains: vec![], // Deprecated field, kept for compatibility
        };

        Ok((network_mode, Some(socket_path)))
    } else if common.no_network {
        Ok((NetworkMode::Disabled, None))
    } else {
        Ok((NetworkMode::Enabled, None))
    }
}
