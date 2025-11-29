//! Bubblewrap sandboxing wrapper for Claude CLI

use anyhow::{Context, Result};
use bwrap_core::{CommonArgs, ConfigLoader, HomeAccessMode, SandboxBuilder, SandboxConfig, ToolConfig, setup_policy};
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

    // Handle --list-policies and --list-groups flags
    if args.common.list_policies || args.common.list_groups {
        let config = ConfigLoader::load_or_default(args.common.proxy_config.clone())
            .context("Failed to load proxy configuration")?;

        if args.common.list_policies {
            println!("Available policies:\n");
            for (name, policy) in &config.policy.policies {
                println!("  {} - {}", name, policy.description.as_deref().unwrap_or("(no description)"));
            }
            println!();
        }

        if args.common.list_groups {
            println!("Available host groups:\n");
            for (name, group) in &config.network.groups {
                println!("  {} - {}", name, group.description);
            }
            println!();
        }

        return Ok(());
    }

    // Get Claude CLI path
    let claude_path = get_claude_path()?;

    // Load configuration
    let app_config = ConfigLoader::load_or_default(args.common.proxy_config.clone())
        .context("Failed to load application configuration")?;

    // Set up policy with tool-specific default
    let policy_setup = setup_policy(&app_config, &args.common, "claude")
        .await
        .context("Failed to set up policy")?;

    // Determine target directory
    let target_dir = if let Some(dir) = args.common.dir.as_ref() {
        dir.canonicalize()
            .context("Failed to canonicalize target directory")?
    } else {
        env::current_dir().context("Failed to get current directory")?
    };

    // Build tool configuration
    let tool_config = ToolConfig {
        name: "claude".to_string(),
        cli_path: claude_path,
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
    let config = SandboxConfig {
        tool_name: "claude".to_string(),
        policy_name: policy_setup.policy_name,
        tool_config,
        target_dir,
        network_mode: policy_setup.network_mode,
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
    let sandbox = SandboxBuilder::new(config, policy_setup.filesystem_spec)
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
