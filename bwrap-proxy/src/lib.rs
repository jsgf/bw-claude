//! SOCKS5 proxy with network filtering for bw-claude sandbox

pub mod config;
pub mod error;
pub mod filter;
pub mod proxy;

// Re-export commonly used types
pub use config::{Config, ConfigLoader, ProxyMode};
pub use error::{ProxyError, Result, ValidationError};
pub use filter::{HostMatcher, PolicyEngine, LearningRecorder};
pub use proxy::{ProxyServer, ProxyServerConfig};
