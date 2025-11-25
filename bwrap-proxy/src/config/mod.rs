//! Configuration management for proxy

pub mod loader;
pub mod schema;
pub mod validator;

pub use loader::ConfigLoader;
pub use schema::{Config, HostGroup, NetworkConfig, Policy, ProxyMode, ToolConfig};
pub use validator::ConfigValidator;
