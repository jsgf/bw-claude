//! Network configuration schema for proxy filtering
//!
//! This module defines network-specific configuration types used by the proxy.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Network configuration with host groups
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct NetworkConfig {
    #[serde(default)]
    pub groups: IndexMap<String, HostGroup>,
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
    /// References to other groups (for composition)
    #[serde(default)]
    pub groups: Vec<String>,
}

/// Network mode: how to handle network access
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    /// Full network access without proxy
    Open,
    /// No network access at all
    Disabled,
    /// Network access through filtering proxy
    Proxy,
}

/// Default behavior when no policy rule matches
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DefaultMode {
    /// Allow by default (deny-listed access)
    Allow,
    /// Deny by default (allow-listed access)
    Deny,
}
