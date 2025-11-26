//! Core library for bubblewrap sandboxing
//!
//! Provides type-safe configuration and command building for bubblewrap-based
//! sandboxing of LLM CLI tools.

pub mod args;
pub mod config;
pub mod env;
pub mod error;
pub mod mount;
pub mod sandbox;
pub mod startup_script;

pub use args::CommonArgs;
pub use config::{HomeAccessMode, NetworkMode, SandboxConfig, ToolConfig};
pub use error::{Result, SandboxError};
pub use sandbox::{Sandbox, SandboxBuilder};
