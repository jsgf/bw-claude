//! Environment variable management for sandbox

use std::collections::HashMap;
use std::env;
use std::ffi::OsString;

/// Builder for environment variables in the sandbox
#[derive(Debug, Default)]
pub struct EnvironmentBuilder {
    vars: HashMap<String, String>,
}

impl EnvironmentBuilder {
    /// Create a new environment builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an environment variable
    pub fn set<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) -> &mut Self {
        self.vars.insert(key.into(), value.into());
        self
    }

    /// Pass through an environment variable from the host
    pub fn pass_through(&mut self, key: &str) -> &mut Self {
        if let Ok(value) = env::var(key) {
            self.vars.insert(key.to_string(), value);
        }
        self
    }

    /// Set multiple environment variables
    pub fn set_many(&mut self, vars: HashMap<String, String>) -> &mut Self {
        self.vars.extend(vars);
        self
    }

    /// Pass through multiple environment variables from the host
    pub fn pass_through_many(&mut self, keys: &[String]) -> &mut Self {
        for key in keys {
            self.pass_through(key);
        }
        self
    }

    /// Convert to bwrap command arguments
    pub fn to_args(&self) -> Vec<OsString> {
        let mut args = Vec::new();

        for (key, value) in &self.vars {
            args.push("--setenv".into());
            args.push(key.into());
            args.push(value.into());
        }

        args
    }

    /// Get the environment variables as a HashMap
    pub fn vars(&self) -> &HashMap<String, String> {
        &self.vars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_env() {
        let mut builder = EnvironmentBuilder::new();
        builder.set("FOO", "bar");

        assert_eq!(builder.vars().get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    fn test_to_args() {
        let mut builder = EnvironmentBuilder::new();
        builder.set("FOO", "bar").set("BAZ", "qux");

        let args = builder.to_args();

        // Check that we have the right number of arguments (--setenv KEY VALUE for each)
        assert_eq!(args.len(), 4);
        assert!(args.contains(&OsString::from("--setenv")));
    }
}
