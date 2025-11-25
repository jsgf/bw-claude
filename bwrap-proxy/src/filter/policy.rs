//! Policy engine for evaluating network access

use super::matcher::HostMatcher;
use crate::config::schema::{HostGroup, NetworkConfig};
use crate::error::{ProxyError, Result};
use ipnet::{Ipv4Net, Ipv6Net};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;

/// Policy engine that evaluates whether connections should be allowed
#[derive(Debug, Clone)]
pub struct PolicyEngine {
    matcher: HostMatcher,
    allow_all: bool,
}

impl PolicyEngine {
    /// Create a policy engine from a named policy
    pub fn from_policy(policy_name: &str, network: &NetworkConfig) -> Result<Self> {
        let policy = network
            .policies
            .get(policy_name)
            .ok_or_else(|| ProxyError::PolicyNotFound {
                policy: policy_name.to_string(),
            })?;

        // If policy allows all, return early
        if policy.allow_all {
            return Ok(Self {
                matcher: HostMatcher::new(),
                allow_all: true,
            });
        }

        let mut matcher = HostMatcher::new();
        let mut processed = HashSet::new();

        // Recursively expand all groups referenced by the policy
        for group_name in &policy.groups {
            Self::expand_group(group_name, &network.groups, &mut matcher, &mut processed)?;
        }

        Ok(Self {
            matcher,
            allow_all: false,
        })
    }

    /// Recursively expand a group and add its hosts/IPs to the matcher
    fn expand_group(
        group_name: &str,
        groups: &HashMap<String, HostGroup>,
        matcher: &mut HostMatcher,
        processed: &mut HashSet<String>,
    ) -> Result<()> {
        // Avoid reprocessing groups (handles DAG structure)
        if processed.contains(group_name) {
            return Ok(());
        }

        processed.insert(group_name.to_string());

        let group = groups.get(group_name).ok_or_else(|| ProxyError::GroupNotFound {
            group: group_name.to_string(),
        })?;

        // Add host patterns
        for host in &group.hosts {
            matcher.add_pattern(host);
        }

        // Add IPv4 ranges
        for range_str in &group.ipv4_ranges {
            let range = range_str
                .parse::<Ipv4Net>()
                .map_err(|e| ProxyError::Network(format!("Invalid IPv4 range {}: {}", range_str, e)))?;
            matcher.add_ipv4_range(range);
        }

        // Add IPv6 ranges
        for range_str in &group.ipv6_ranges {
            let range = range_str
                .parse::<Ipv6Net>()
                .map_err(|e| ProxyError::Network(format!("Invalid IPv6 range {}: {}", range_str, e)))?;
            matcher.add_ipv6_range(range);
        }

        // Recursively expand referenced groups
        for child_name in &group.groups {
            Self::expand_group(child_name, groups, matcher, processed)?;
        }

        Ok(())
    }

    /// Check if a connection to the given host/IP should be allowed
    pub fn allow(&self, host: &str, ip: Option<IpAddr>) -> bool {
        if self.allow_all {
            return true;
        }

        self.matcher.matches(host, ip)
    }

    /// Check if this policy allows all traffic
    pub fn is_allow_all(&self) -> bool {
        self.allow_all
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{HostGroup, NetworkConfig, Policy};

    fn create_test_network() -> NetworkConfig {
        let mut network = NetworkConfig::default();

        // Create some test groups
        network.groups.insert(
            "anthropic".to_string(),
            HostGroup {
                description: "Anthropic".to_string(),
                hosts: vec!["*.anthropic.com".to_string(), "*.claude.ai".to_string()],
                ipv4_ranges: vec![],
                ipv6_ranges: vec![],
                groups: vec![],
            },
        );

        network.groups.insert(
            "google".to_string(),
            HostGroup {
                description: "Google".to_string(),
                hosts: vec!["*.google.com".to_string()],
                ipv4_ranges: vec!["142.250.0.0/15".to_string()],
                ipv6_ranges: vec![],
                groups: vec![],
            },
        );

        // Create a policy
        network.policies.insert(
            "claude_default".to_string(),
            Policy {
                description: "Default Claude policy".to_string(),
                groups: vec!["anthropic".to_string(), "google".to_string()],
                allow_all: false,
            },
        );

        network.policies.insert(
            "open".to_string(),
            Policy {
                description: "Allow all".to_string(),
                groups: vec![],
                allow_all: true,
            },
        );

        network
    }

    #[test]
    fn test_policy_engine_from_policy() {
        let network = create_test_network();
        let engine = PolicyEngine::from_policy("claude_default", &network).unwrap();

        assert!(engine.allow("api.anthropic.com", None));
        assert!(engine.allow("console.claude.ai", None));
        assert!(engine.allow("www.google.com", None));
        assert!(!engine.allow("www.example.com", None));
    }

    #[test]
    fn test_allow_all_policy() {
        let network = create_test_network();
        let engine = PolicyEngine::from_policy("open", &network).unwrap();

        assert!(engine.is_allow_all());
        assert!(engine.allow("anything.com", None));
        assert!(engine.allow("example.org", None));
    }

    #[test]
    fn test_policy_not_found() {
        let network = create_test_network();
        let result = PolicyEngine::from_policy("nonexistent", &network);

        assert!(result.is_err());
    }

    #[test]
    fn test_group_composition() {
        let mut network = NetworkConfig::default();

        // Create groups that reference each other
        network.groups.insert(
            "base".to_string(),
            HostGroup {
                description: "Base".to_string(),
                hosts: vec!["*.base.com".to_string()],
                ipv4_ranges: vec![],
                ipv6_ranges: vec![],
                groups: vec![],
            },
        );

        network.groups.insert(
            "extended".to_string(),
            HostGroup {
                description: "Extended".to_string(),
                hosts: vec!["*.extended.com".to_string()],
                ipv4_ranges: vec![],
                ipv6_ranges: vec![],
                groups: vec!["base".to_string()],
            },
        );

        network.policies.insert(
            "test".to_string(),
            Policy {
                description: "Test".to_string(),
                groups: vec!["extended".to_string()],
                allow_all: false,
            },
        );

        let engine = PolicyEngine::from_policy("test", &network).unwrap();

        // Should match both base and extended
        assert!(engine.allow("api.base.com", None));
        assert!(engine.allow("api.extended.com", None));
        assert!(!engine.allow("api.other.com", None));
    }
}
