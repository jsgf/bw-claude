//! Bubblewrap sandboxing wrapper for Gemini CLI

use anyhow::{Context, Result};
use bwrap_core::{CommonArgs, ConfigLoader, HomeAccessMode, SandboxBuilder, SandboxConfig, ToolConfig, setup_policy};
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

    // Get Gemini CLI path
    let gemini_path = get_gemini_path()?;

    // Load configuration
    let app_config = ConfigLoader::load_or_default(args.common.proxy_config.clone())
        .context("Failed to load application configuration")?;

    // Set up policy with tool-specific default
    let policy_setup = setup_policy(&app_config, &args.common, "gemini")
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
        name: "gemini".to_string(),
        cli_path: gemini_path,
        default_args: vec![],
        cli_args: args.common.cli_args,
        help_text: "Gemini arguments are passed through unchanged.\n\nFor authentication, you may need to pass environment variables into the sandbox.\nUse the --pass-env argument for each variable you need."
            .to_string(),
    };

    // Build sandbox configuration
    let config = SandboxConfig {
        tool_name: "gemini".to_string(),
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
        env_vars: std::collections::HashMap::new(),
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
