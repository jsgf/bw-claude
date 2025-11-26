//! Sandbox builder and execution

use crate::config::{
    HomeAccessMode, NetworkMode, ESSENTIAL_ETC_DIRS, ESSENTIAL_ETC_FILES, SAFE_CONFIG_DIRS,
    SAFE_HOME_DIRS, SandboxConfig,
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
}

impl SandboxBuilder {
    /// Create a new sandbox builder
    pub fn new(config: SandboxConfig) -> Result<Self> {
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
                self.mount_safe_home_dirs(&home_path)?;
                self.mount_safe_config_dirs(&home_path)?;
            }
        }

        // System binaries and libraries (read-only)
        for path in ["/usr", "/lib", "/lib64"] {
            if Path::new(path).exists() {
                self.mounts.push(MountPoint::ro(path, path));
            }
        }

        // Create /bin as symlink to /usr/bin for compatibility
        self.mounts
            .push(MountPoint::symlink("/usr/bin", "/bin"));

        // Tool-specific state directories
        let global_tool_dir = home_path.join(format!(".{}", self.config.tool_name));
        if global_tool_dir.exists() {
            self.mounts
                .push(MountPoint::rw(&global_tool_dir, &global_tool_dir));
        }

        // Tool-specific dot file in home
        if let Some(ref dot_file) = self.config.tool_config.home_dot_file {
            let dot_file_path = home_path.join(dot_file);
            if !dot_file_path.exists() {
                // Create empty file so bind mount works
                fs::File::create(&dot_file_path)?;
            }
            self.mounts
                .push(MountPoint::rw(&dot_file_path, &dot_file_path));
        }

        // Project directory (read-write by default)
        self.mounts
            .push(MountPoint::rw(&self.config.target_dir, &self.config.target_dir));

        // Process and device access
        self.mounts.push(MountPoint::ro("/proc", "/proc")); // Use --proc instead
        self.mounts.push(MountPoint::rw("/dev", "/dev")); // Use --dev-bind instead

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

        // Handle Filtered network mode setup
        if let NetworkMode::Filtered { proxy_socket, .. } = &self.config.network_mode {
            // Mount proxy socket for relay to connect to
            self.mounts
                .push(MountPoint::rw(proxy_socket, &PathBuf::from("/proxy.sock")));

            // Determine bw-relay path (REQUIRED for filtered proxy mode)
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
        }

        Ok(())
    }

    fn mount_minimal_etc(&mut self) -> Result<()> {
        // Create empty /etc
        self.mounts.push(MountPoint::tmpfs("/etc"));

        // Mount individual essential files
        for filename in ESSENTIAL_ETC_FILES {
            let filepath = PathBuf::from("/etc").join(filename);
            self.mounts.push(MountPoint::ro_try(&filepath, &filepath));
        }

        // Mount essential directories
        for dirname in ESSENTIAL_ETC_DIRS {
            let dirpath = PathBuf::from("/etc").join(dirname);
            self.mounts.push(MountPoint::ro_try(&dirpath, &dirpath));
        }

        // Special handling for /etc/resolv.conf if it's a symlink
        let resolv_conf = Path::new("/etc/resolv.conf");
        if resolv_conf.exists() && resolv_conf.is_symlink() {
            let real_resolv = resolv_conf
                .canonicalize()
                .map_err(|e| SandboxError::SymlinkResolution {
                    path: resolv_conf.to_path_buf(),
                    source: e,
                })?;
            self.mounts
                .push(MountPoint::ro_try(&real_resolv, &PathBuf::from("/etc/resolv.conf")));
        }

        // Remount /etc as read-only to prevent processes from creating new files
        self.mounts.push(MountPoint::remount_ro("/etc"));

        Ok(())
    }

    fn mount_safe_home_dirs(&mut self, home: &Path) -> Result<()> {
        for dir_name in SAFE_HOME_DIRS {
            let dir_path = home.join(dir_name);
            if dir_path.exists() {
                // Use ro_try to skip if mount fails (e.g., permission issues)
                self.mounts.push(MountPoint::ro_try(&dir_path, &dir_path));
            }
        }
        Ok(())
    }

    fn mount_safe_config_dirs(&mut self, home: &Path) -> Result<()> {
        let config_dir = home.join(".config");
        for subdir in SAFE_CONFIG_DIRS {
            let subdir_path = config_dir.join(subdir);
            if subdir_path.exists() {
                // Use ro_try to skip if mount fails (e.g., permission issues)
                self.mounts.push(MountPoint::ro_try(&subdir_path, &subdir_path));
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
            NetworkMode::Enabled => {
                cmd.arg("--share-net");
            }
            NetworkMode::Disabled => {
                cmd.arg("--unshare-net");
            }
            NetworkMode::Filtered { .. } => {
                // Proxy mode disables direct network access, all traffic goes through proxy
                cmd.arg("--unshare-net");
            }
        }

        // Add all mounts
        for mount in &self.mounts {
            // Skip --proc and --dev-bind, handle them specially
            if mount.target == Path::new("/proc") {
                continue;
            }
            if mount.target == Path::new("/dev") {
                continue;
            }
            cmd.args(mount.to_args());
        }

        // Special handling for /proc and /dev
        cmd.arg("--proc").arg("/proc");
        cmd.arg("--dev-bind").arg("/dev").arg("/dev");

        // Set working directory
        cmd.arg("--chdir")
            .arg(&self.config.target_dir);

        // Clear environment and set variables
        cmd.arg("--clearenv");
        cmd.args(self.env_builder.to_args());

        // Shell or CLI command
        // Proxy relay must always be used when proxy is enabled
        if let NetworkMode::Filtered { .. } = &self.config.network_mode {
            // In Filtered mode, execute bw-relay with the target command
            let (target_binary, target_args) = if self.config.shell {
                // Pass shell as the target command to bw-relay
                (PathBuf::from("/bin/sh"), vec!["-i".to_string()])
            } else {
                // Pass the CLI tool with its arguments
                (
                    self.config.tool_config.cli_path.clone(),
                    {
                        let mut args = self.config.tool_config.default_args.clone();
                        args.extend(self.config.tool_config.cli_args.clone());
                        args
                    },
                )
            };
            // bw-relay runs inside the container, so use the in-container socket path
            let relay_args = crate::startup_script::build_relay_command(
                &PathBuf::from("/proxy.sock"),
                &PathBuf::from("/bw-relay"),
                &target_binary,
                &target_args,
            );
            cmd.args(relay_args);
        } else if self.config.shell {
            cmd.arg("/bin/sh").arg("-i");
        } else {
            cmd.arg(&self.config.tool_config.cli_path);
            cmd.args(&self.config.tool_config.default_args);
            cmd.args(&self.config.tool_config.cli_args);
        }

        // Print debug info if verbose
        if self.config.verbose {
            tracing::info!("Working directory: {}", self.config.target_dir.display());
            if let Some(ref tmp_dir) = self.tmp_export_dir {
                tracing::info!("Export /tmp: {}", tmp_dir.display());
            }
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
