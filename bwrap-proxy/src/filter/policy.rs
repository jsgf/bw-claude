//! Policy engine for evaluating network access

use super::matcher::HostMatcher;
use crate::config::schema::{HostGroup, NetworkConfig};
use crate::error::{ProxyError, Result};
use ipnet::{Ipv4Net, Ipv6Net};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;

/// Policy engine that evaluates whether connections should be allowed
/// Uses "more specific wins" logic when both allow and deny rules match
#[derive(Debug, Clone)]
pub struct PolicyEngine {
    allow_matcher: HostMatcher,
    deny_matcher: HostMatcher,
    mode: crate::config::schema::PolicyMode,
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
                allow_matcher: HostMatcher::new(),
                deny_matcher: HostMatcher::new(),
                mode: policy.mode.clone(),
                allow_all: true,
            });
        }

        let mut allow_matcher = HostMatcher::new();
        let mut deny_matcher = HostMatcher::new();
        let mut processed = HashSet::new();

        // Recursively expand all groups referenced by the policy's allow groups
        // Use effective_allow_groups() which handles backward compatibility
        for group_name in &policy.effective_allow_groups() {
            Self::expand_group(group_name, &network.groups, &mut allow_matcher, &mut processed)?;
        }

        // Recursively expand all groups referenced by the policy's deny groups
        processed.clear();
        for group_name in &policy.deny_groups {
            Self::expand_group(group_name, &network.groups, &mut deny_matcher, &mut processed)?;
        }

        Ok(Self {
            allow_matcher,
            deny_matcher,
            mode: policy.mode.clone(),
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
    /// Uses "more specific wins" logic: when both allow and deny rules match,
    /// the one with higher specificity wins. On a tie, deny wins.
    pub fn allow(&self, host: &str, ip: Option<IpAddr>) -> bool {
        if self.allow_all {
            return true;
        }

        // Check hostname specificity for both matchers
        let allow_hostname_spec = self.allow_matcher.matches_with_specificity(host);
        let deny_hostname_spec = self.deny_matcher.matches_with_specificity(host);

        // Check IP matches (no specificity - either matches or doesn't)
        let allow_ip_match = ip.map(|a| self.allow_matcher.matches_ip(a)).unwrap_or(false);
        let deny_ip_match = ip.map(|a| self.deny_matcher.matches_ip(a)).unwrap_or(false);

        // Apply "more specific wins" logic with hostnames taking precedence
        if let (Some(allow_spec), Some(deny_spec)) = (allow_hostname_spec, deny_hostname_spec) {
            // Both matched by hostname, more specific wins (deny wins on tie)
            return allow_spec > deny_spec;
        }

        if allow_hostname_spec.is_some() {
            // Only allow matched by hostname
            return true;
        }

        if deny_hostname_spec.is_some() {
            // Only deny matched by hostname
            return false;
        }

        // No hostname matches, check IP matches
        if allow_ip_match && deny_ip_match {
            // Both matched by IP, deny wins on tie
            return false;
        }

        if allow_ip_match {
            return true;
        }

        if deny_ip_match {
            return false;
        }

        // Neither matched - use policy mode default
        // In Allow mode: block by default (return false)
        // In Deny mode: allow by default (return true)
        self.mode == crate::config::schema::PolicyMode::Deny
    }

    /// Check if this policy allows all traffic
    pub fn is_allow_all(&self) -> bool {
        self.allow_all
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{HostGroup, NetworkConfig, Policy, PolicyMode};

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
                allow_groups: vec!["anthropic".to_string(), "google".to_string()],
                deny_groups: vec![],
                groups: vec![],
                allow_all: false,
                mode: PolicyMode::Allow,
            },
        );

        network.policies.insert(
            "open".to_string(),
            Policy {
                description: "Allow all".to_string(),
                allow_groups: vec![],
                deny_groups: vec![],
                groups: vec![],
                allow_all: true,
                mode: PolicyMode::Allow,
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
                allow_groups: vec!["extended".to_string()],
                deny_groups: vec![],
                groups: vec![],
                allow_all: false,
                mode: PolicyMode::Allow,
            },
        );

        let engine = PolicyEngine::from_policy("test", &network).unwrap();

        // Should match both base and extended
        assert!(engine.allow("api.base.com", None));
        assert!(engine.allow("api.extended.com", None));
        assert!(!engine.allow("api.other.com", None));
    }
}
