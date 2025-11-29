//! Error types for proxy operations

use std::path::PathBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, ProxyError>;

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("Configuration validation error: {0}")]
    Validation(#[from] ValidationError),

    #[error("Policy not found: {policy}")]
    PolicyNotFound { policy: String },

    #[error("Group not found: {group}")]
    GroupNotFound { group: String },

    #[error("Connection denied: {host}:{port}")]
    ConnectionDenied { host: String, port: u16 },

    #[error("SOCKS5 protocol error: {0}")]
    Socks5(String),

    #[error("Failed to load config from {path}: {source}")]
    ConfigLoad {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to parse config: {0}")]
    ConfigParse(#[from] toml::de::Error),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Network error: {0}")]
    Network(String),
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Cycle detected in group references: {path}")]
    CycleDetected { path: String },

    #[error("Unknown group reference: {group}")]
    UnknownGroup { group: String },

    #[error("Invalid CIDR notation: {cidr}")]
    InvalidCidr { cidr: String },

    #[error("Invalid wildcard pattern: {pattern}")]
    InvalidPattern { pattern: String },

    #[error("Invalid proxy mode: {mode}")]
    InvalidMode { mode: String },
}
