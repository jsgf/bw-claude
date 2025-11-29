//! Configuration validation including cycle detection

use super::schema::{HostGroup, NetworkConfig};
use crate::error::{Result, ValidationError};
use indexmap::IndexMap;
use std::collections::HashSet;

pub struct ConfigValidator;

impl ConfigValidator {
    /// Validate entire network configuration
    pub fn validate(network: &NetworkConfig) -> Result<()> {
        Self::check_cycles(network)?;
        Self::validate_references(network)?;
        Self::validate_patterns(network)?;
        Ok(())
    }

    /// Check for cycles in group references using DFS
    fn check_cycles(network: &NetworkConfig) -> Result<()> {
        for (group_name, _) in &network.groups {
            let mut visited = HashSet::new();
            let mut path = Vec::new();
            Self::dfs_cycle_check(group_name, &network.groups, &mut visited, &mut path)?;
        }
        Ok(())
    }

    /// DFS-based cycle detection
    fn dfs_cycle_check(
        group_name: &str,
        groups: &IndexMap<String, HostGroup>,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Result<()> {
        // If this group is in the current path, we found a cycle
        if path.contains(&group_name.to_string()) {
            path.push(group_name.to_string());
            return Err(ValidationError::CycleDetected {
                path: path.join(" -> "),
            }
            .into());
        }

        // If already fully processed, skip
        if visited.contains(group_name) {
            return Ok(());
        }

        visited.insert(group_name.to_string());
        path.push(group_name.to_string());

        // Recursively check all referenced groups
        if let Some(group) = groups.get(group_name) {
            for child in &group.groups {
                Self::dfs_cycle_check(child, groups, visited, path)?;
            }
        }

        path.pop();
        Ok(())
    }

    /// Validate that all group references exist
    fn validate_references(network: &NetworkConfig) -> Result<()> {
        // Check group-to-group references
        for (group_name, group) in &network.groups {
            for ref_name in &group.groups {
                if !network.groups.contains_key(ref_name) {
                    return Err(ValidationError::UnknownGroup {
                        group: format!("{} -> {}", group_name, ref_name),
                    }
                    .into());
                }
            }
        }

        Ok(())
    }

    /// Validate wildcard patterns
    fn validate_patterns(network: &NetworkConfig) -> Result<()> {
        for (group_name, group) in &network.groups {
            for pattern in &group.hosts {
                // Basic validation: no double wildcards
                if pattern.contains("**") {
                    return Err(ValidationError::InvalidPattern {
                        pattern: format!("{} in group {}", pattern, group_name),
                    }
                    .into());
                }

                // Check for invalid characters
                if pattern.contains('\0') || pattern.contains('\n') {
                    return Err(ValidationError::InvalidPattern {
                        pattern: format!("{} in group {}", pattern, group_name),
                    }
                    .into());
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::HostGroup;

    #[test]
    fn test_no_cycle() {
        let mut network = NetworkConfig::default();

        network.groups.insert(
            "a".to_string(),
            HostGroup {
                description: "A".to_string(),
                hosts: vec![],
                hosts_deny: vec![],
                groups: vec!["b".to_string()],
            },
        );

        network.groups.insert(
            "b".to_string(),
            HostGroup {
                description: "B".to_string(),
                hosts: vec![],
                hosts_deny: vec![],
                groups: vec![],
            },
        );

        assert!(ConfigValidator::check_cycles(&network).is_ok());
    }

    #[test]
    fn test_detect_cycle() {
        let mut network = NetworkConfig::default();

        network.groups.insert(
            "a".to_string(),
            HostGroup {
                description: "A".to_string(),
                hosts: vec![],
                hosts_deny: vec![],
                groups: vec!["b".to_string()],
            },
        );

        network.groups.insert(
            "b".to_string(),
            HostGroup {
                description: "B".to_string(),
                hosts: vec![],
                hosts_deny: vec![],
                groups: vec!["a".to_string()], // Cycle!
            },
        );

        assert!(ConfigValidator::check_cycles(&network).is_err());
    }

}
