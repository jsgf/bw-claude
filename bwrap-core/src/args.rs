//! Common CLI argument structure for bw-* executables

use clap::Parser;
use std::path::PathBuf;

/// Common CLI arguments shared by all bw-* executables
#[derive(Parser, Debug)]
pub struct CommonArgs {
    /// Disable network access (default: network enabled)
    #[arg(long)]
    pub no_network: bool,

    /// Allow full home directory access (default: safe dirs only)
    #[arg(long)]
    pub full_home_access: bool,

    /// Print sandbox configuration and bwrap command to stderr
    #[arg(long, short)]
    pub verbose: bool,

    /// Launch an interactive shell in the sandbox (for debugging)
    #[arg(long)]
    pub shell: bool,

    /// Mount additional read-only path (can be used multiple times)
    #[arg(long = "allow-ro", value_name = "PATH")]
    pub allow_ro_paths: Vec<PathBuf>,

    /// Mount additional read-write path (can be used multiple times)
    #[arg(long = "allow-rw", value_name = "PATH")]
    pub allow_rw_paths: Vec<PathBuf>,

    /// Set working directory in sandbox (default: current directory)
    #[arg(long, value_name = "PATH")]
    pub dir: Option<PathBuf>,

    /// Pass an environment variable into the sandbox (can be used multiple times)
    #[arg(long = "pass-env", value_name = "VAR_NAME")]
    pub pass_env_vars: Vec<String>,

    /// Enable filtered proxy mode for fine-grained network control
    #[arg(long)]
    pub use_filter_proxy: bool,

    /// Proxy configuration file (TOML format)
    #[arg(long, value_name = "PATH")]
    pub proxy_config: Option<PathBuf>,

    /// Path to bw-relay binary (for filtered proxy mode)
    #[arg(long, value_name = "PATH")]
    pub bw_relay_path: Option<PathBuf>,

    /// Tool arguments (use -- to separate from bw-* options)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub cli_args: Vec<String>,
}
