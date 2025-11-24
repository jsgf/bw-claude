//! Bubblewrap sandboxing wrapper for Gemini CLI

use anyhow::{Context, Result};
use bwrap_core::{HomeAccessMode, NetworkMode, SandboxBuilder, SandboxConfig, ToolConfig};
use clap::Parser;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

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

    /// Gemini arguments (use -- to separate from bw-gemini options)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    cli_args: Vec<String>,
}

fn main() -> Result<()> {
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

    // Build sandbox configuration
    let config = SandboxConfig {
        tool_name: "gemini".to_string(),
        tool_config,
        target_dir,
        network_mode: if args.no_network {
            NetworkMode::Disabled
        } else {
            NetworkMode::Enabled
        },
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
