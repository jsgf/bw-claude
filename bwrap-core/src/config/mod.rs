//! Configuration system for bw-claude
//!
//! This module handles all configuration:
//! - SandboxConfig: Runtime configuration for sandbox execution
//! - Config: Application configuration with filesystem, policies, and network settings

pub mod sandbox;
pub mod schema;
pub mod loader;
pub mod resolver;
pub mod learning;
pub mod builtin;

// Re-export commonly used types
pub use sandbox::{HomeAccessMode, SandboxConfig, ToolConfig, NetworkMode};
pub use schema::{
    Config, CommonConfig, FilesystemConfig, FilesystemSpec,
    NetworkPolicy, Policy, PolicyConfig, ProxyConfig, ProxyMode,
};
// Re-export network types from bwrap-proxy
pub use bwrap_proxy::config::{DefaultMode, HostGroup, NetworkConfig};
pub use loader::ConfigLoader;
pub use resolver::{resolve_filesystem_config, resolve_policy};
pub use learning::LearningRecorder;
