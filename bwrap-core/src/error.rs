//! Error types for sandbox operations

use std::path::PathBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, SandboxError>;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("Tool CLI not found at {0}")]
    CliNotFound(PathBuf),

    #[error("Directory does not exist: {0}")]
    DirNotFound(PathBuf),

    #[error("Failed to create temporary directory: {0}")]
    TmpDirCreation(#[source] std::io::Error),

    #[error("Failed to resolve symlink {path}: {source}")]
    SymlinkResolution {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to execute bwrap command: {0}")]
    BwrapExecution(#[source] std::io::Error),

    #[error("Environment variable {0} not found")]
    EnvVarNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
