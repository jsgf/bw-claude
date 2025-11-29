//! Configuration file loading and merging

use super::schema::Config;
use super::builtin;
use crate::error::{Result, SandboxError};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use indexmap::IndexMap;

pub struct ConfigLoader;

impl ConfigLoader {
    /// Find user config by checking environment and standard locations
    pub fn find_user_config() -> Option<PathBuf> {
        // 1. $BW_CLAUDE_CONFIG
        if let Ok(path) = env::var("BW_CLAUDE_CONFIG") {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some(p);
            }
        }

        // 2. $XDG_CONFIG_HOME/bw-claude/config.toml
        if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
            let p = PathBuf::from(xdg).join("bw-claude/config.toml");
            if p.exists() {
                return Some(p);
            }
        }

        // 3. ~/.config/bw-claude/config.toml
        if let Ok(home) = env::var("HOME") {
            let p = PathBuf::from(home).join(".config/bw-claude/config.toml");
            if p.exists() {
                return Some(p);
            }
        }

        None
    }

    /// Find project config by searching up directory tree for .bwconfig.toml
    pub fn find_project_config() -> Option<PathBuf> {
        let mut current = env::current_dir().ok()?;

        loop {
            let project_config = current.join(".bwconfig.toml");
            if project_config.exists() {
                return Some(project_config);
            }

            if !current.pop() {
                break;
            }
        }

        None
    }

    /// Get the default config file path (deprecated, use find_user_config instead)
    pub fn default_config_path() -> PathBuf {
        Self::find_user_config().unwrap_or_else(|| PathBuf::from("config.toml"))
    }

    /// Load config from a file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Config> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|source| SandboxError::ConfigLoad {
            path: path.to_path_buf(),
            source,
        })?;

        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Load built-in configuration embedded in the binary
    pub fn load_builtin() -> Result<Config> {
        Ok(builtin::get_builtin().clone())
    }

    /// Merge user config on top of built-in config
    /// User config takes precedence: groups and policies are extended,
    /// tool-specific settings override built-in
    pub fn merge_configs(mut base: Config, override_cfg: Config) -> Config {
        // Merge network groups: extend with overrides
        for (name, group) in override_cfg.network.groups {
            base.network.groups.insert(name, group);
        }

        // Merge filesystem configs: extend with overrides
        for (name, fs_config) in override_cfg.filesystem.configs {
            base.filesystem.configs.insert(name, fs_config);
        }

        // Merge policies: extend with overrides
        for (name, policy) in override_cfg.policy.policies {
            base.policy.policies.insert(name, policy);
        }

        // Merge tool configs: extend with overrides
        for (name, tool_config) in override_cfg.tools {
            base.tools.insert(name, tool_config);
        }

        // Override common config with user settings
        base.common = override_cfg.common;

        base
    }

    /// Load config with built-in as lowest-priority fallback
    /// Priority: User config > Built-in config
    pub fn load_with_builtins() -> Result<Config> {
        let builtin = Self::load_builtin()?;
        let path = Self::default_config_path();

        if path.exists() {
            let user = Self::load_from_file(&path)?;
            Ok(Self::merge_configs(builtin, user))
        } else {
            tracing::debug!("User config not found at {:?}, using built-in defaults", path);
            Ok(builtin)
        }
    }

    /// Load config with fallback to defaults
    pub fn load() -> Result<Config> {
        let path = Self::default_config_path();

        if path.exists() {
            Self::load_from_file(&path)
        } else {
            tracing::debug!("Config file not found at {:?}, using defaults", path);
            Ok(Config::default())
        }
    }

    /// Load with full config priority order
    /// Priority: built-in < user < project < explicit
    pub fn load_with_priority(explicit_config: Option<PathBuf>) -> Result<Config> {
        let mut configs = Vec::new();

        // 1. Built-in (lowest priority)
        configs.push(Self::load_builtin()?);

        // 2. User config
        if let Some(user_path) = Self::find_user_config() {
            tracing::debug!("Loading user config from {:?}", user_path);
            configs.push(Self::load_from_file(&user_path)?);
        }

        // 3. Project config
        if let Some(project_path) = Self::find_project_config() {
            tracing::debug!("Loading project config from {:?}", project_path);
            configs.push(Self::load_from_file(&project_path)?);
        }

        // 4. Explicit --config option (highest priority)
        if let Some(explicit_path) = explicit_config {
            tracing::debug!("Loading explicit config from {:?}", explicit_path);
            configs.push(Self::load_from_file(&explicit_path)?);
        }

        // Merge all configs, later ones override earlier ones
        Ok(configs
            .into_iter()
            .reduce(|acc, cfg| Self::merge_configs(acc, cfg))
            .unwrap_or_default())
    }

    /// Load config from optional path or default with built-in merge
    /// Priority: built-in < user < project < explicit
    pub fn load_or_default(path: Option<PathBuf>) -> Result<Config> {
        Self::load_with_priority(path)
    }

    /// Alias for load_or_default for clarity in naming
    pub fn load_or_builtin(path: Option<PathBuf>) -> Result<Config> {
        Self::load_or_default(path)
    }

    /// Ensure config directory exists
    pub fn ensure_config_dir() -> std::io::Result<PathBuf> {
        let config_path = Self::default_config_path();
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(config_path)
    }
}

/// Create an empty config with no settings
/// Most code should use load_with_builtins() or load_with_priority() instead
/// to get builtin defaults merged in
fn empty_config() -> Config {
    use bwrap_proxy::config::NetworkConfig;

    Config {
        common: super::schema::CommonConfig {
            config_version: "1.0".to_string(),
            verbose: false,
            proxy: super::schema::ProxyConfig {
                default_mode: "restrictive".to_string(),
                socket_dir: PathBuf::from("/tmp"),
                learning_output: PathBuf::from("~/.config/bw-claude/learned-domains.toml"),
            },
        },
        network: NetworkConfig::default(),
        filesystem: super::schema::FilesystemConfig {
            configs: IndexMap::new(),
        },
        policy: super::schema::PolicyConfig {
            policies: IndexMap::new(),
        },
        tools: IndexMap::new(),
    }
}

impl Default for Config {
    fn default() -> Self {
        // Return empty config - callers should use load_with_builtins() for defaults
        empty_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.common.config_version, "1.0");
        assert!(!config.common.verbose);
    }

    #[test]
    fn test_parse_toml() {
        let toml_str = r#"
[common]
config_version = "1.0"
verbose = true

[common.proxy]
default_mode = "open"

[network]
"#;

        let result: std::result::Result<Config, _> = toml::from_str(toml_str);
        assert!(result.is_ok());
    }
}
