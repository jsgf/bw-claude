//! Bubblewrap sandboxing wrapper for Gemini CLI

use anyhow::{Context, Result};
use bwrap_core::{CommonArgs, create_proxy_task, HomeAccessMode, NetworkMode, SandboxBuilder, SandboxConfig, ToolConfig};
use clap::Parser;
use std::env;
use std::path::PathBuf;

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

    // Get Gemini CLI path
    let gemini_path = get_gemini_path()?;

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
        name: "gemini".to_string(),
        cli_path: gemini_path,
        home_dot_file: None,
        default_args: vec![],
        cli_args: args.common.cli_args,
        help_text: "Gemini arguments are passed through unchanged.\n\nFor authentication, you may need to pass environment variables into the sandbox.\nUse the --pass-env argument for each variable you need."
            .to_string(),
    };

    // Build sandbox configuration
    // Note: bw-relay sets proxy env vars, so we don't set them here
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
        env_vars: std::collections::HashMap::new(),
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

/// Determine the network mode based on CLI flags
/// --policy, --learn, or --learn-deny enables HTTP CONNECT proxy with filtering
async fn determine_network_mode(
    common: &CommonArgs,
    proxy_config: &Option<PathBuf>,
) -> Result<(NetworkMode, Option<PathBuf>)> {
    // Check if lockdown policy is specified (pure network isolation, no proxy)
    if let Some(ref policy) = common.policy {
        if policy == "lockdown" {
            return Ok((NetworkMode::Disabled, None));
        }
    }

    // Check if proxy should be enabled
    let use_proxy = common.policy.is_some()
        || common.learn.is_some()
        || common.learn_deny.is_some();

    if use_proxy {
        // Determine policy name and learning mode:
        // - If --learn is specified: use "open" policy (allow all, record access)
        // - If --learn-deny is specified: use specified policy or default to "block" (enforce policy, record denials)
        // - If --policy is specified: use that policy (no learning)
        let (policy_name, learning_mode, learning_output) = if let Some(_) = common.learn {
            ("open", Some("learn"), common.learn.as_ref())
        } else if let Some(_) = common.learn_deny {
            (
                common.policy.as_ref().map(|s| s.as_str()).unwrap_or("block"),
                Some("learn_deny"),
                common.learn_deny.as_ref(),
            )
        } else {
            (
                common.policy.as_ref().map(|s| s.as_str()).unwrap_or("open"),
                None,
                None,
            )
        };

        // Create proxy task with policy and learning configuration
        let (socket_path, _) = create_proxy_task(
            proxy_config,
            Some(policy_name),
            learning_output,
            learning_mode.map(|s| s.to_string()),
        )
        .await?;

        let network_mode = NetworkMode::Filtered {
            proxy_socket: socket_path.clone(),
            policy_name: policy_name.to_string(),
            learning_output: learning_output.cloned(),
            learning_mode: learning_mode.map(|s| s.to_string()),
            allowed_domains: vec![], // Deprecated field, kept for compatibility
        };

        Ok((network_mode, Some(socket_path)))
    } else if common.no_network {
        Ok((NetworkMode::Disabled, None))
    } else {
        Ok((NetworkMode::Enabled, None))
    }
}
