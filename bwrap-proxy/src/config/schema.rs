//! Configuration schema types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Complete proxy configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub common: CommonConfig,
    #[serde(default)]
    pub network: NetworkConfig,
    pub claude: Option<ToolConfig>,
    pub gemini: Option<ToolConfig>,
}

/// Common settings across all tools
#[derive(Debug, Clone, Deserialize, Serialize)]
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

/// Network configuration with groups and policies
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct NetworkConfig {
    #[serde(default)]
    pub groups: HashMap<String, HostGroup>,
    #[serde(default)]
    pub policies: HashMap<String, Policy>,
}

/// A named group of hosts and IP ranges
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HostGroup {
    #[serde(default)]
    pub description: String,
    /// Hosts to allow/include
    #[serde(default)]
    pub hosts: Vec<String>,
    /// Hosts to explicitly deny (override allow rules)
    #[serde(default)]
    pub hosts_deny: Vec<String>,
    #[serde(default)]
    pub ipv4_ranges: Vec<String>,
    #[serde(default)]
    pub ipv6_ranges: Vec<String>,
    /// References to other groups (for composition)
    #[serde(default)]
    pub groups: Vec<String>,
}

/// Policy mode: block-by-default or allow-by-default
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PolicyMode {
    /// Block by default, only allow listed groups (safer default)
    Allow,
    /// Allow by default, only block listed groups
    Deny,
}

fn default_policy_mode() -> PolicyMode {
    PolicyMode::Allow
}

/// A policy that references groups
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Policy {
    #[serde(default)]
    pub description: String,
    /// Groups to allow (used in Allow mode)
    #[serde(default)]
    pub allow_groups: Vec<String>,
    /// Groups to deny (used in Deny mode or as overrides)
    #[serde(default)]
    pub deny_groups: Vec<String>,
    /// Backward compatibility: old 'groups' field is alias for allow_groups
    #[serde(default)]
    pub groups: Vec<String>,
    /// Allow all traffic (bypass all filtering)
    #[serde(default)]
    pub allow_all: bool,
    /// Policy mode: Allow (block-by-default) or Deny (allow-by-default)
    #[serde(default = "default_policy_mode")]
    pub mode: PolicyMode,
}

impl Policy {
    /// Get the effective allow groups (combining both old and new fields)
    pub fn effective_allow_groups(&self) -> Vec<String> {
        if !self.allow_groups.is_empty() {
            self.allow_groups.clone()
        } else {
            self.groups.clone()  // Backward compatibility
        }
    }
}

/// Tool-specific configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
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
    pub fn parse(s: &str) -> Result<Self, crate::error::ValidationError> {
        match s {
            "open" => Ok(ProxyMode::Open),
            "learning" => Ok(ProxyMode::Learning),
            s if s.starts_with("restrictive:") => {
                let policy = s.strip_prefix("restrictive:").unwrap().to_string();
                if policy.is_empty() {
                    Err(crate::error::ValidationError::InvalidMode {
                        mode: s.to_string(),
                    })
                } else {
                    Ok(ProxyMode::Restrictive(policy))
                }
            }
            _ => Err(crate::error::ValidationError::InvalidMode {
                mode: s.to_string(),
            }),
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
