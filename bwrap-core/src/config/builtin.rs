//! Built-in default configuration embedded in the binary
//!
//! The builtin configuration serves as the lowest-priority configuration layer,
//! providing sensible defaults for all settings. It is lazy-loaded on first access
//! and cached using LazyLock to avoid repeated deserialization.

use std::sync::LazyLock;
use super::schema::Config;

/// Lazy-initialized builtin configuration
static BUILTIN_CONFIG: LazyLock<Config> = LazyLock::new(load_builtin_config);

/// Get the builtin configuration
pub fn get_builtin() -> &'static Config {
    &BUILTIN_CONFIG
}

/// Load builtin configuration from embedded TOML string
fn load_builtin_config() -> Config {
    const BUILTIN_TOML: &str = include_str!("../builtin-policies.toml");
    toml::from_str(BUILTIN_TOML).expect("Failed to parse builtin configuration")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_loads() {
        let config = get_builtin();
        assert_eq!(config.common.config_version, "1.0");
    }

    #[test]
    fn test_builtin_cached() {
        let config1 = get_builtin();
        let config2 = get_builtin();
        // Should be the same pointer since it's cached
        assert_eq!(config1 as *const _, config2 as *const _);
    }

    #[test]
    fn test_builtin_has_policies() {
        let config = get_builtin();
        assert!(config.policy.policies.contains_key("shell"), "shell policy not found");
        assert!(config.policy.policies.contains_key("claude"), "claude policy not found");
        assert!(config.policy.policies.contains_key("gemini"), "gemini policy not found");
        assert!(config.policy.policies.contains_key("deny"), "deny policy not found");
        assert!(config.policy.policies.contains_key("open"), "open policy not found");
    }
}
