//! Configuration management for proxy network filtering
//!
//! This module only handles network-specific configuration types.
//! The full application configuration system is in bwrap-core.

pub mod schema;
pub mod validator;

pub use schema::{HostGroup, NetworkConfig, NetworkMode, DefaultMode};
