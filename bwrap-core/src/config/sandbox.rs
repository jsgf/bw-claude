//! Configuration types and constants for sandboxing

use std::collections::HashMap;
use std::path::PathBuf;

/// Complete sandbox configuration
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Name of the tool being sandboxed (e.g., "claude", "gemini")
    pub tool_name: String,

    /// Name of the policy being enforced (e.g., "claude", "gemini", "lockdown")
    /// Indicates which policy is in effect for this sandbox
    pub policy_name: String,

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

    /// Optional explicit path to bw-relay binary (for filtered proxy mode)
    pub bw_relay_path: Option<PathBuf>,
}

impl SandboxConfig {
    /// Get the name of the policy being used
    pub fn policy_name(&self) -> &str {
        &self.policy_name
    }
}

/// Tool-specific configuration
#[derive(Debug, Clone)]
pub struct ToolConfig {
    /// Tool name (e.g., "claude", "gemini")
    pub name: String,

    /// Path to the tool's CLI executable
    pub cli_path: PathBuf,

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

    /// Filtered network access via SOCKS proxy with policy enforcement
    Filtered {
        /// Path to the SOCKS proxy socket
        proxy_socket: PathBuf,

        /// Name of the policy to enforce (e.g., "claude", "gemini", "lockdown", "open")
        /// "open" = allow all, "lockdown" = localhost only, other = named policy
        policy_name: String,

        /// Optional path for learning mode output (records accessed domains)
        learning_output: Option<PathBuf>,

        /// Learning mode type: "learn" (allow all, record access) or "learn_deny" (enforce policy, record denials)
        /// None if not in learning mode
        learning_mode: Option<String>,

        /// Kept for backward compatibility (derived from policy)
        #[deprecated(note = "Use policy_name instead")]
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
