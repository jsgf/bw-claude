//! Mount point management for sandbox

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// A mount point in the sandbox
#[derive(Debug, Clone)]
pub struct MountPoint {
    /// Source path on host
    pub source: PathBuf,

    /// Target path in sandbox
    pub target: PathBuf,

    /// Mount mode (read-only, read-write, etc.)
    pub mode: MountMode,
}

/// Mount mode for a mount point
#[derive(Debug, Clone, PartialEq)]
pub enum MountMode {
    /// Read-only bind mount
    ReadOnly,

    /// Read-write bind mount
    ReadWrite,

    /// Read-only bind mount (skip if source doesn't exist)
    ReadOnlyTry,

    /// Tmpfs mount
    Tmpfs,

    /// Remount an existing mount as read-only
    RemountRo,

    /// Symlink
    Symlink { target: PathBuf },

    /// /proc mount
    Proc,

    /// /dev bind mount
    DevBind,
}

impl MountPoint {
    /// Create a read-only mount point
    pub fn ro<P: AsRef<Path>>(source: P, target: P) -> Self {
        Self {
            source: source.as_ref().to_path_buf(),
            target: target.as_ref().to_path_buf(),
            mode: MountMode::ReadOnly,
        }
    }

    /// Create a read-write mount point
    pub fn rw<P: AsRef<Path>>(source: P, target: P) -> Self {
        Self {
            source: source.as_ref().to_path_buf(),
            target: target.as_ref().to_path_buf(),
            mode: MountMode::ReadWrite,
        }
    }

    /// Create a read-only mount point that skips if source doesn't exist
    pub fn ro_try<P: AsRef<Path>>(source: P, target: P) -> Self {
        Self {
            source: source.as_ref().to_path_buf(),
            target: target.as_ref().to_path_buf(),
            mode: MountMode::ReadOnlyTry,
        }
    }

    /// Create a tmpfs mount point
    pub fn tmpfs<P: AsRef<Path>>(target: P) -> Self {
        Self {
            source: PathBuf::new(),
            target: target.as_ref().to_path_buf(),
            mode: MountMode::Tmpfs,
        }
    }

    /// Remount an existing mount as read-only
    pub fn remount_ro<P: AsRef<Path>>(target: P) -> Self {
        Self {
            source: PathBuf::new(),
            target: target.as_ref().to_path_buf(),
            mode: MountMode::RemountRo,
        }
    }

    /// Create a symlink
    pub fn symlink<P: AsRef<Path>>(link_target: P, link_path: P) -> Self {
        Self {
            source: PathBuf::new(),
            target: link_path.as_ref().to_path_buf(),
            mode: MountMode::Symlink {
                target: link_target.as_ref().to_path_buf(),
            },
        }
    }

    /// Mount /proc filesystem
    pub fn proc() -> Self {
        Self {
            source: PathBuf::new(),
            target: PathBuf::from("/proc"),
            mode: MountMode::Proc,
        }
    }

    /// Bind mount /dev filesystem
    pub fn dev_bind() -> Self {
        Self {
            source: PathBuf::from("/dev"),
            target: PathBuf::from("/dev"),
            mode: MountMode::DevBind,
        }
    }

    /// Convert this mount point to bwrap command arguments
    pub fn to_args(&self) -> Vec<OsString> {
        match &self.mode {
            MountMode::ReadOnly => {
                vec![
                    "--ro-bind".into(),
                    self.source.clone().into(),
                    self.target.clone().into(),
                ]
            }
            MountMode::ReadWrite => {
                vec![
                    "--bind".into(),
                    self.source.clone().into(),
                    self.target.clone().into(),
                ]
            }
            MountMode::ReadOnlyTry => {
                vec![
                    "--ro-bind-try".into(),
                    self.source.clone().into(),
                    self.target.clone().into(),
                ]
            }
            MountMode::Tmpfs => {
                vec!["--tmpfs".into(), self.target.clone().into()]
            }
            MountMode::RemountRo => {
                vec!["--remount-ro".into(), self.target.clone().into()]
            }
            MountMode::Symlink { target } => {
                vec![
                    "--symlink".into(),
                    target.clone().into(),
                    self.target.clone().into(),
                ]
            }
            MountMode::Proc => {
                vec!["--proc".into(), self.target.clone().into()]
            }
            MountMode::DevBind => {
                vec![
                    "--dev-bind".into(),
                    self.source.clone().into(),
                    self.target.clone().into(),
                ]
            }
        }
    }
}
