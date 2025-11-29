//! Configuration schema types

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use bwrap_proxy::config::{NetworkConfig, DefaultMode, NetworkMode};

/// Complete application configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub common: CommonConfig,
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub filesystem: FilesystemConfig,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub tools: IndexMap<String, ToolConfig>,
}

/// Common settings across all tools
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommonConfig {
    #[serde(default = "default_config_version")]
    pub config_version: String,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub proxy: ProxyConfig,
}

fn default_config_version() -> String {
    "1.0".to_string()
}

impl Default for CommonConfig {
    fn default() -> Self {
        Self {
            config_version: default_config_version(),
            verbose: false,
            proxy: ProxyConfig::default(),
        }
    }
}

/// Proxy-specific settings
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyConfig {
    #[serde(default = "default_proxy_mode")]
    pub default_mode: String,
    #[serde(default = "default_socket_dir")]
    pub socket_dir: PathBuf,
    #[serde(default = "default_learning_output")]
    pub learning_output: PathBuf,
}

fn default_proxy_mode() -> String {
    "restrictive".to_string()
}

fn default_socket_dir() -> PathBuf {
    PathBuf::from("/tmp")
}

fn default_learning_output() -> PathBuf {
    PathBuf::from("~/.config/bw-claude/learned-domains.toml")
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            default_mode: default_proxy_mode(),
            socket_dir: default_socket_dir(),
            learning_output: default_learning_output(),
        }
    }
}

fn default_network_mode() -> NetworkMode {
    NetworkMode::Proxy
}

fn default_default_mode() -> DefaultMode {
    DefaultMode::Deny
}

/// Filesystem configuration: named filesystem configurations
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FilesystemConfig {
    /// Named filesystem configurations
    #[serde(default)]
    pub configs: IndexMap<String, FilesystemSpec>,
}

impl Default for FilesystemConfig {
    fn default() -> Self {
        Self {
            configs: IndexMap::new(),
        }
    }
}

/// A named filesystem specification
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FilesystemSpec {
    #[serde(default)]
    pub description: Option<String>,
    /// Read-only home directories to mount (including .config/... subdirectories)
    #[serde(default)]
    pub ro_home_dirs: Vec<String>,
    /// Read-write home directories to mount
    #[serde(default)]
    pub rw_home_dirs: Vec<String>,
    /// Read-only files in home directory
    #[serde(default)]
    pub ro_home_files: Vec<String>,
    /// Read-write files in home directory
    #[serde(default)]
    pub rw_home_files: Vec<String>,
    /// Essential /etc files to mount
    #[serde(default)]
    pub essential_etc_files: Vec<String>,
    /// Essential /etc directories to mount
    #[serde(default)]
    pub essential_etc_dirs: Vec<String>,
    /// System paths to mount (e.g., /usr, /lib)
    #[serde(default)]
    pub system_paths: Vec<String>,
    /// Read-only paths to mount (anywhere in filesystem)
    #[serde(default)]
    pub ro_paths: Vec<String>,
    /// Read-write paths to mount (anywhere in filesystem)
    #[serde(default)]
    pub rw_paths: Vec<String>,
    /// Reference to other filesystem configs to extend (composition)
    #[serde(default)]
    pub extends: Vec<String>,
}

impl Default for FilesystemSpec {
    fn default() -> Self {
        Self {
            description: None,
            ro_home_dirs: vec![],
            rw_home_dirs: vec![],
            ro_home_files: vec![],
            rw_home_files: vec![],
            essential_etc_files: vec![],
            essential_etc_dirs: vec![],
            system_paths: vec![],
            ro_paths: vec![],
            rw_paths: vec![],
            extends: vec![],
        }
    }
}

/// Policy configuration: named policies combining network and filesystem settings
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyConfig {
    /// Named policies combining network, filesystem, etc.
    #[serde(default)]
    pub policies: IndexMap<String, Policy>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            policies: IndexMap::new(),
        }
    }
}

/// A top-level policy combining network and filesystem configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(default)]
    pub description: Option<String>,
    /// Network configuration for this policy
    #[serde(default)]
    pub network: NetworkPolicy,
    /// Reference to a named filesystem config
    pub filesystem: Option<String>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            description: None,
            network: NetworkPolicy::default(),
            filesystem: None,
        }
    }
}

/// Network settings within a policy
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkPolicy {
    /// Network mode: how to handle network access
    #[serde(default = "default_network_mode")]
    pub network: NetworkMode,
    /// Default behavior on no match
    #[serde(default = "default_default_mode")]
    pub default: DefaultMode,
    /// Groups to allow
    #[serde(default)]
    pub allow_groups: Vec<String>,
    /// Groups to deny
    #[serde(default)]
    pub deny_groups: Vec<String>,
    /// Backward compatibility: old 'groups' field is alias for allow_groups
    #[serde(default)]
    pub groups: Vec<String>,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            network: default_network_mode(),
            default: default_default_mode(),
            allow_groups: vec![],
            deny_groups: vec![],
            groups: vec![],
        }
    }
}

impl NetworkPolicy {
    /// Get the effective allow groups (combining both old and new fields)
    pub fn effective_allow_groups(&self) -> Vec<String> {
        if !self.allow_groups.is_empty() {
            self.allow_groups.clone()
        } else {
            self.groups.clone()  // Backward compatibility
        }
    }
}

/// Tool-specific configuration in config file
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub proxy_mode: Option<String>,
    /// Default policy for this tool (e.g., "claude" or "gemini")
    #[serde(default)]
    pub default_policy: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Proxy operating mode
#[derive(Debug, Clone, PartialEq)]
pub enum ProxyMode {
    /// Allow all traffic
    Open,
    /// Allow all traffic but record accessed domains
    Learning,
    /// Enforce a named policy
    Restrictive(String),
}

impl ProxyMode {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "open" => Ok(ProxyMode::Open),
            "learning" => Ok(ProxyMode::Learning),
            s if s.starts_with("restrictive:") => {
                let policy = s.strip_prefix("restrictive:").unwrap().to_string();
                if policy.is_empty() {
                    Err(format!("Invalid proxy mode: {}", s))
                } else {
                    Ok(ProxyMode::Restrictive(policy))
                }
            }
            _ => Err(format!("Invalid proxy mode: {}", s)),
        }
    }
}

impl std::fmt::Display for ProxyMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProxyMode::Open => write!(f, "open"),
            ProxyMode::Learning => write!(f, "learning"),
            ProxyMode::Restrictive(policy) => write!(f, "restrictive:{}", policy),
        }
    }
}
