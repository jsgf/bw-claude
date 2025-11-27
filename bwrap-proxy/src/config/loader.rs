//! Configuration file loading and merging

use super::schema::Config;
use crate::error::{ProxyError, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ConfigLoader;

impl ConfigLoader {
    /// Get the default config file path
    pub fn default_config_path() -> PathBuf {
        // Priority order:
        // 1. $BW_CLAUDE_CONFIG
        // 2. $XDG_CONFIG_HOME/bw-claude/config.toml
        // 3. ~/.config/bw-claude/config.toml

        if let Ok(path) = env::var("BW_CLAUDE_CONFIG") {
            return PathBuf::from(path);
        }

        if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("bw-claude/config.toml");
        }

        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home).join(".config/bw-claude/config.toml");
        }

        PathBuf::from("config.toml")
    }

    /// Load config from a file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Config> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|e| ProxyError::ConfigLoad {
            path: path.to_path_buf(),
            source: e,
        })?;

        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Load built-in configuration embedded in the binary
    pub fn load_builtin() -> Result<Config> {
        const BUILTIN_TOML: &str = include_str!("../builtin-policies.toml");
        let config: Config = toml::from_str(BUILTIN_TOML)?;
        Ok(config)
    }

    /// Merge user config on top of built-in config
    /// User config takes precedence: groups and policies are extended,
    /// tool-specific settings override built-in
    pub fn merge_configs(builtin: Config, user: Config) -> Config {
        let mut merged = builtin;

        // Merge network groups: user groups override/extend built-in
        for (name, group) in user.network.groups {
            merged.network.groups.insert(name, group);
        }

        // Merge network policies: user policies override/extend built-in
        for (name, policy) in user.network.policies {
            merged.network.policies.insert(name, policy);
        }

        // Override common config with user settings
        merged.common = user.common;

        // Override tool configs if user specified them
        if user.claude.is_some() {
            merged.claude = user.claude;
        }
        if user.gemini.is_some() {
            merged.gemini = user.gemini;
        }

        merged
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

    /// Load config from optional path or default with built-in merge
    /// Priority: Explicit path > User config > Built-in config
    pub fn load_or_default(path: Option<PathBuf>) -> Result<Config> {
        if let Some(p) = path {
            let user = Self::load_from_file(&p)?;
            let builtin = Self::load_builtin()?;
            Ok(Self::merge_configs(builtin, user))
        } else {
            Self::load_with_builtins()
        }
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

impl Default for Config {
    fn default() -> Self {
        Self {
            common: Default::default(),
            network: Default::default(),
            claude: None,
            gemini: None,
        }
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

[[network.host_groups]]
name = "test"
description = "Test group"
domains = ["*.example.com"]
"#;

        let result: std::result::Result<Config, _> = toml::from_str(toml_str);
        assert!(result.is_ok());
    }
}
