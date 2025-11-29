//! Core library for bubblewrap sandboxing
//!
//! Provides type-safe configuration and command building for bubblewrap-based
//! sandboxing of LLM CLI tools.

pub mod args;
pub mod config;
pub mod env;
pub mod error;
pub mod mount;
pub mod network;
pub mod policy;
pub mod proxy;
pub mod sandbox;

pub use args::CommonArgs;
pub use config::{
    Config, ConfigLoader, DefaultMode, FilesystemConfig, FilesystemSpec, HomeAccessMode,
    NetworkMode, NetworkPolicy, Policy, PolicyConfig, ProxyMode, SandboxConfig, ToolConfig,
    resolve_filesystem_config, resolve_policy,
};
pub use error::{Result, SandboxError};
pub use network::determine_network_mode;
pub use policy::{setup_policy, PolicySetup};
pub use proxy::create_proxy_task;
pub use sandbox::{Sandbox, SandboxBuilder};
