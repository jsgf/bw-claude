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

    /// Load config from optional path or default
    pub fn load_or_default(path: Option<PathBuf>) -> Result<Config> {
        if let Some(p) = path {
            Self::load_from_file(&p)
        } else {
            Self::load()
        }
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
