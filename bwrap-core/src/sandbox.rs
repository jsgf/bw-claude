//! Sandbox builder and execution

use crate::config::{
    HomeAccessMode, NetworkMode, SandboxConfig, FilesystemSpec,
};
use crate::env::EnvironmentBuilder;
use crate::error::{Result, SandboxError};
use crate::mount::MountPoint;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};

/// Format a Command for display
fn format_command(cmd: &Command) -> String {
    let program = cmd.get_program().to_string_lossy();
    let args: Vec<String> = cmd
        .get_args()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect();
    if args.is_empty() {
        program.to_string()
    } else {
        format!("{} {}", program, args.join(" "))
    }
}

/// Builder for creating a sandbox
pub struct SandboxBuilder {
    config: SandboxConfig,
    mounts: Vec<MountPoint>,
    env_builder: EnvironmentBuilder,
    tmp_export_dir: Option<PathBuf>,
    filesystem_spec: FilesystemSpec,
}

impl SandboxBuilder {
    /// Create a new sandbox builder with a filesystem spec
    pub fn new(config: SandboxConfig, filesystem_spec: FilesystemSpec) -> Result<Self> {
        // Validate configuration
        if !config.shell && !config.tool_config.cli_path.exists() {
            return Err(SandboxError::CliNotFound(
                config.tool_config.cli_path.clone(),
            ));
        }

        if !config.target_dir.is_dir() {
            return Err(SandboxError::DirNotFound(config.target_dir.clone()));
        }

        Ok(Self {
            config,
            mounts: Vec::new(),
            env_builder: EnvironmentBuilder::new(),
            tmp_export_dir: None,
            filesystem_spec,
        })
    }

    /// Build the sandbox
    pub fn build(mut self) -> Result<Sandbox> {
        // Create isolated /tmp export directory
        self.tmp_export_dir = Some(self.create_tmp_export_dir()?);

        // Set up mounts
        self.setup_mounts()?;

        // Set up environment
        self.setup_environment()?;

        // Build the command
        let command = self.build_command()?;

        Ok(Sandbox {
            command,
            tmp_export_dir: self.tmp_export_dir,
        })
    }

    fn create_tmp_export_dir(&self) -> Result<PathBuf> {
        let session_id = uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .take(8)
            .collect::<String>();
        let export_dir = PathBuf::from("/tmp").join(format!("bw-{}-{}", self.config.tool_name, session_id));

        fs::create_dir_all(&export_dir).map_err(SandboxError::TmpDirCreation)?;

        Ok(export_dir)
    }

    fn setup_mounts(&mut self) -> Result<()> {
        let home = env::var("HOME").map_err(|_| SandboxError::EnvVarNotFound("HOME".to_string()))?;
        let home_path = PathBuf::from(&home);

        // Mount isolated /tmp
        if let Some(ref tmp_dir) = self.tmp_export_dir {
            self.mounts.push(MountPoint::rw(tmp_dir, &PathBuf::from("/tmp")));
        }

        // Mount /etc as tmpfs, then mount essential files
        self.mount_minimal_etc()?;

        // Home directory access
        match self.config.home_access {
            HomeAccessMode::Full => {
                self.mounts.push(MountPoint::rw(&home_path, &home_path));
            }
            HomeAccessMode::Safe => {
                self.mount_ro_home_dirs(&home_path)?;
                self.mount_rw_home_dirs(&home_path)?;
                self.mount_home_files(&home_path)?;
            }
        }

        // Mount additional paths from config (both modes can use this)
        self.mount_config_paths()?;

        // System binaries and libraries (read-only)
        for path in ["/usr", "/lib", "/lib64"] {
            if Path::new(path).exists() {
                self.mounts.push(MountPoint::ro(path, path));
            }
        }

        // Create /bin as symlink to /usr/bin for compatibility
        self.mounts
            .push(MountPoint::symlink("/usr/bin", "/bin"));

        // Note: Tool-specific state directories and dot files should be configured
        // via the filesystem config (safe_home_dirs), not hardcoded here

        // Project directory (read-write by default)
        self.mounts
            .push(MountPoint::rw(&self.config.target_dir, &self.config.target_dir));

        // Mount bw-relay binary for command execution
        self.mount_bw_relay()?;

        // Process and device access (handled with special modes)
        self.mounts.push(MountPoint::proc());
        self.mounts.push(MountPoint::dev_bind());

        // Additional mount paths (support relative paths)
        for path in &self.config.additional_ro_paths {
            let resolved_path = if path.is_absolute() {
                path.clone()
            } else {
                self.config.target_dir.join(path)
            };
            if resolved_path.exists() {
                self.mounts.push(MountPoint::ro(&resolved_path, &resolved_path));
            } else if self.config.verbose {
                tracing::warn!("--allow-ro path does not exist: {}", path.display());
            }
        }

        for path in &self.config.additional_rw_paths {
            let resolved_path = if path.is_absolute() {
                path.clone()
            } else {
                self.config.target_dir.join(path)
            };
            if resolved_path.exists() {
                self.mounts.push(MountPoint::rw(&resolved_path, &resolved_path));
            } else if self.config.verbose {
                tracing::warn!("--allow-rw path does not exist: {}", path.display());
            }
        }

        // Mount proxy socket if in Filtered network mode
        if let NetworkMode::Filtered { proxy_socket, .. } = &self.config.network_mode {
            self.mounts
                .push(MountPoint::rw(proxy_socket, &PathBuf::from("/proxy.sock")));
        }

        Ok(())
    }

    fn mount_minimal_etc(&mut self) -> Result<()> {
        // Create empty /etc
        self.mounts.push(MountPoint::tmpfs("/etc"));

        // Collect paths first to avoid borrowing issues
        let files_to_mount: Vec<PathBuf> = self
            .filesystem_spec
            .essential_etc_files
            .iter()
            .map(|f| PathBuf::from("/etc").join(f))
            .collect();

        let dirs_to_mount: Vec<PathBuf> = self
            .filesystem_spec
            .essential_etc_dirs
            .iter()
            .map(|d| PathBuf::from("/etc").join(d))
            .collect();

        // Mount individual essential files (resolve symlinks if needed)
        for filepath in files_to_mount {
            self.mount_with_symlink_resolution(&filepath)?;
        }

        // Mount essential directories (resolve symlinks if needed)
        for dirpath in dirs_to_mount {
            self.mount_with_symlink_resolution(&dirpath)?;
        }

        // Remount /etc as read-only to prevent processes from creating new files
        self.mounts.push(MountPoint::remount_ro("/etc"));

        Ok(())
    }

    /// Mount a path, following symlinks if necessary
    fn mount_with_symlink_resolution(&mut self, path: &Path) -> Result<()> {
        if path.exists() {
            if path.is_symlink() {
                // Resolve symlink and mount the real path
                let real_path = path.canonicalize().map_err(|e| {
                    SandboxError::SymlinkResolution {
                        path: path.to_path_buf(),
                        source: e,
                    }
                })?;
                self.mounts.push(MountPoint::ro_try(&real_path, &path.to_path_buf()));
            } else {
                // Regular file/directory, mount as-is
                self.mounts.push(MountPoint::ro_try(path, path));
            }
        }
        Ok(())
    }

    /// Mount bw-relay binary for command execution
    fn mount_bw_relay(&mut self) -> Result<()> {
        let relay_path = if let Some(explicit_path) = &self.config.bw_relay_path {
            // Explicit path provided - must exist
            if !explicit_path.exists() {
                return Err(SandboxError::CliNotFound(explicit_path.clone()))?;
            }
            explicit_path.clone()
        } else {
            // Try to find bw-relay in common locations
            let mut candidates = Vec::new();

            // 1. Same directory as current executable
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(parent) = exe_path.parent() {
                    candidates.push(parent.join("bw-relay"));
                }
            }

            // 2. Search in PATH
            if let Ok(path_env) = std::env::var("PATH") {
                for path_dir in path_env.split(':') {
                    candidates.push(PathBuf::from(path_dir).join("bw-relay"));
                }
            }

            // Find first existing candidate
            let default = candidates
                .into_iter()
                .find(|p| p.exists())
                .ok_or_else(|| SandboxError::CliNotFound(PathBuf::from("bw-relay")))?;

            default
        };

        tracing::debug!("Mounting bw-relay from: {:?}", relay_path);
        self.mounts
            .push(MountPoint::ro(&relay_path, &PathBuf::from("/bw-relay")));

        Ok(())
    }

    fn mount_ro_home_dirs(&mut self, home: &Path) -> Result<()> {
        for dir_name in &self.filesystem_spec.ro_home_dirs {
            let dir_path = home.join(dir_name);
            if dir_path.exists() {
                // Use ro_try to skip if mount fails (e.g., permission issues)
                self.mounts.push(MountPoint::ro_try(&dir_path, &dir_path));
            }
        }
        Ok(())
    }

    fn mount_rw_home_dirs(&mut self, home: &Path) -> Result<()> {
        for dir_name in &self.filesystem_spec.rw_home_dirs {
            let dir_path = home.join(dir_name);
            if dir_path.exists() {
                // Use rw mount for read-write home directories
                self.mounts.push(MountPoint::rw(&dir_path, &dir_path));
            }
        }
        Ok(())
    }

    fn mount_home_files(&mut self, home: &Path) -> Result<()> {
        // Mount read-only files in home directory
        for file_name in &self.filesystem_spec.ro_home_files {
            let file_path = home.join(file_name);
            if file_path.exists() {
                self.mounts.push(MountPoint::ro_try(&file_path, &file_path));
            }
        }

        // Mount read-write files in home directory
        for file_name in &self.filesystem_spec.rw_home_files {
            let file_path = home.join(file_name);
            if file_path.exists() {
                self.mounts.push(MountPoint::rw(&file_path, &file_path));
            }
        }
        Ok(())
    }

    fn mount_config_paths(&mut self) -> Result<()> {
        // Mount read-only paths from config
        for path_str in &self.filesystem_spec.ro_paths {
            let path = PathBuf::from(path_str);
            if path.exists() {
                self.mounts.push(MountPoint::ro_try(&path, &path));
            }
        }

        // Mount read-write paths from config
        for path_str in &self.filesystem_spec.rw_paths {
            let path = PathBuf::from(path_str);
            if path.exists() {
                self.mounts.push(MountPoint::rw(&path, &path));
            }
        }
        Ok(())
    }

    fn setup_environment(&mut self) -> Result<()> {
        let home = env::var("HOME").map_err(|_| SandboxError::EnvVarNotFound("HOME".to_string()))?;
        let user = env::var("USER").unwrap_or_else(|_| "user".to_string());
        let path_env = env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin:/usr/sbin:/sbin".to_string());
        let term_env = env::var("TERM").unwrap_or_else(|_| "xterm".to_string());

        self.env_builder
            .set("HOME", home)
            .set("PWD", self.config.target_dir.display().to_string())
            .set("USER", user)
            .set("PATH", path_env)
            .set("TERM", term_env);

        // Set additional environment variables
        self.env_builder.set_many(self.config.env_vars.clone());

        // Pass through specified environment variables
        self.env_builder
            .pass_through_many(&self.config.pass_through_env);

        Ok(())
    }

    fn build_command(&self) -> Result<Command> {
        let mut cmd = Command::new("bwrap");

        // Basic sandbox setup
        cmd.arg("--die-with-parent")
            .arg("--unshare-pid")
            .arg("--unshare-ipc");

        // Network namespace
        match self.config.network_mode {
            NetworkMode::Enabled => 
                cmd.arg("--share-net"),
            NetworkMode::Disabled | NetworkMode::Filtered { .. } => 
                cmd.arg("--unshare-net"),
        };

        // Add all mounts
        for mount in &self.mounts {
            cmd.args(mount.to_args());
        }

        // Set working directory
        cmd.arg("--chdir")
            .arg(&self.config.target_dir);

        // Clear environment and set variables
        cmd.arg("--clearenv");
        cmd.args(self.env_builder.to_args());

        // Always use bw-relay to execute the target command
        let (target_binary, target_args) = if self.config.shell {
            // Pass shell as the target command to bw-relay
            // Include any CLI arguments passed through
            let mut shell_args = vec!["-i".to_string()];
            shell_args.extend(self.config.tool_config.cli_args.clone());
            (PathBuf::from("/bin/sh"), shell_args)
        } else {
            // Pass the CLI tool with its arguments
            let mut args = self.config.tool_config.default_args.clone();
            args.extend(self.config.tool_config.cli_args.clone());
            (self.config.tool_config.cli_path.clone(), args)
        };

        // Build bw-relay command with optional socket (only in Filtered mode)
        cmd.arg("/bw-relay");

        // Add socket argument only if we're in proxy mode
        if let NetworkMode::Filtered { .. } = &self.config.network_mode {
            cmd.arg("--socket").arg("/proxy.sock");
        }

        // Add separator and target command
        cmd.arg("--");
        cmd.arg(&target_binary);
        cmd.args(&target_args);

        // Print debug info if verbose
        if self.config.verbose {
            tracing::info!("Working directory: {}", self.config.target_dir.display());
            if let Some(ref tmp_dir) = self.tmp_export_dir {
                tracing::info!("Export /tmp: {}", tmp_dir.display());
            }
            tracing::info!("Policy: {}", self.config.policy_name());
            tracing::info!(
                "Network: {}",
                match self.config.network_mode {
                    NetworkMode::Enabled => "enabled",
                    NetworkMode::Disabled => "disabled",
                    NetworkMode::Filtered { .. } => "filtered",
                }
            );
            tracing::info!(
                "Home access: {}",
                match self.config.home_access {
                    HomeAccessMode::Safe => "safe (restricted)",
                    HomeAccessMode::Full => "full (unsafe)",
                }
            );
            if self.config.shell {
                tracing::info!("Mode: Interactive shell");
            }
            tracing::info!("Command: {}", format_command(&cmd));
        }

        Ok(cmd)
    }
}

/// A configured sandbox ready to execute
pub struct Sandbox {
    command: Command,
    tmp_export_dir: Option<PathBuf>,
}

impl Sandbox {
    /// Execute the sandbox and wait for completion
    pub fn exec(mut self) -> Result<ExitStatus> {
        self.command
            .status()
            .map_err(SandboxError::BwrapExecution)
    }

    /// Spawn the sandbox as a child process
    pub fn spawn(mut self) -> Result<Child> {
        self.command
            .spawn()
            .map_err(SandboxError::BwrapExecution)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // Clean up temporary export directory
        if let Some(ref tmp_dir) = self.tmp_export_dir {
            let _ = fs::remove_dir_all(tmp_dir);
        }
    }
}

// Add uuid dependency for session IDs
mod uuid {
    use std::time::SystemTime;

    pub struct Uuid;

    impl Uuid {
        pub fn new_v4() -> Self {
            Uuid
        }

        pub fn to_string(&self) -> String {
            // Simple pseudo-UUID based on timestamp
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            format!("{:x}", now)
        }
    }
}
