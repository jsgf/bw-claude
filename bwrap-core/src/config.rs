//! Configuration types and constants for sandboxing

use std::collections::HashMap;
use std::path::PathBuf;

/// Directories safe to mount from home when using safe mode (default)
/// NOTE: Documents and Downloads are NOT included as they often contain sensitive files
pub const SAFE_HOME_DIRS: &[&str] = &[
    ".local/share",
    ".local/bin",
    "Projects",
    ".cargo",
    ".rustup",
    ".npm",
    ".gem",
    ".gradle",
    ".m2",
    ".nvm",
    ".go",
    ".viminfo",
    ".gitconfig",
];

/// Safe subdirectories within ~/.config/ to mount (excludes browsers and sensitive data)
pub const SAFE_CONFIG_DIRS: &[&str] = &[
    "git", "nvim", "vim", "htop", "nano", "less", "lsd", "bat", "zsh", "bash", "fish",
    "alacritty", "kitty",
];

/// Essential /etc files to mount (minimal /etc)
pub const ESSENTIAL_ETC_FILES: &[&str] = &["hostname", "hosts", "resolv.conf", "passwd", "group"];

/// Additional directories to mount from /etc
pub const ESSENTIAL_ETC_DIRS: &[&str] = &["pki", "ssl", "crypto-policies"];

/// Complete sandbox configuration
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Name of the tool being sandboxed (e.g., "claude", "gemini")
    pub tool_name: String,

    /// Tool-specific configuration
    pub tool_config: ToolConfig,

    /// Working directory for the sandbox
    pub target_dir: PathBuf,

    /// Network access mode
    pub network_mode: NetworkMode,

    /// Home directory access mode
    pub home_access: HomeAccessMode,

    /// Additional read-only paths to mount
    pub additional_ro_paths: Vec<PathBuf>,

    /// Additional read-write paths to mount
    pub additional_rw_paths: Vec<PathBuf>,

    /// Environment variables to set in sandbox
    pub env_vars: HashMap<String, String>,

    /// Environment variables to pass through from host
    pub pass_through_env: Vec<String>,

    /// Enable verbose output
    pub verbose: bool,

    /// Launch shell instead of tool CLI
    pub shell: bool,
}

/// Tool-specific configuration
#[derive(Debug, Clone)]
pub struct ToolConfig {
    /// Tool name (e.g., "claude", "gemini")
    pub name: String,

    /// Path to the tool's CLI executable
    pub cli_path: PathBuf,

    /// Optional dot file in home directory (e.g., ".claude.json")
    pub home_dot_file: Option<String>,

    /// Default arguments to pass to the tool
    pub default_args: Vec<String>,

    /// Arguments to pass to the tool CLI
    pub cli_args: Vec<String>,

    /// Help text for the tool
    pub help_text: String,
}

/// Network access mode for the sandbox
#[derive(Debug, Clone, PartialEq)]
pub enum NetworkMode {
    /// Full network access (--share-net)
    Enabled,

    /// No network access (--unshare-net)
    Disabled,

    /// Filtered network access via SOCKS proxy
    Filtered {
        proxy_socket: PathBuf,
        allowed_domains: Vec<String>,
    },
}

/// Home directory access mode
#[derive(Debug, Clone, PartialEq)]
pub enum HomeAccessMode {
    /// Safe mode: only mount whitelisted directories
    Safe,

    /// Full home directory access (unsafe)
    Full,
}
