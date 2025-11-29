//! Policy engine for evaluating network access

use super::matcher::HostMatcher;
use crate::config::schema::{DefaultMode, HostGroup, NetworkConfig};
use crate::error::{ProxyError, Result};
use indexmap::IndexMap;
use std::collections::HashSet;
use std::net::IpAddr;

/// Policy engine that evaluates whether connections should be allowed
/// Uses "more specific wins" logic when both allow and deny rules match
#[derive(Debug, Clone)]
pub struct PolicyEngine {
    allow_matcher: HostMatcher,
    deny_matcher: HostMatcher,
    default: crate::config::schema::DefaultMode,
}

impl PolicyEngine {
    /// Create a policy engine from allow/deny groups and default mode
    pub fn from_network_policy(
        allow_groups: Vec<String>,
        deny_groups: Vec<String>,
        default: DefaultMode,
        network_config: &NetworkConfig,
    ) -> Result<Self> {
        let mut allow_matcher = HostMatcher::new();
        let mut deny_matcher = HostMatcher::new();
        let mut processed = HashSet::new();

        // Recursively expand all groups referenced by the policy's allow groups
        for group_name in &allow_groups {
            Self::expand_group(group_name, &network_config.groups, &mut allow_matcher, &mut processed)?;
        }

        // Recursively expand all groups referenced by the policy's deny groups
        processed.clear();
        for group_name in &deny_groups {
            Self::expand_group_deny(group_name, &network_config.groups, &mut deny_matcher, &mut processed)?;
        }

        Ok(Self {
            allow_matcher,
            deny_matcher,
            default,
        })
    }

    /// Recursively expand a group and add its hosts/IPs to the matcher (allow patterns)
    fn expand_group(
        group_name: &str,
        groups: &IndexMap<String, HostGroup>,
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

        // Add allow host patterns
        for host in &group.hosts {
            matcher.add_pattern(host);
        }

        // Recursively expand referenced groups
        for child_name in &group.groups {
            Self::expand_group(child_name, groups, matcher, processed)?;
        }

        Ok(())
    }

    /// Recursively expand a group and add its hosts/IPs to the deny matcher
    fn expand_group_deny(
        group_name: &str,
        groups: &IndexMap<String, HostGroup>,
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

        // Add deny host patterns
        for host in &group.hosts_deny {
            matcher.add_deny_pattern(host);
        }

        // Recursively expand referenced groups (deny patterns)
        for child_name in &group.groups {
            Self::expand_group_deny(child_name, groups, matcher, processed)?;
        }

        Ok(())
    }

    /// Check if a connection to the given host/IP should be allowed
    /// Uses "longest match" logic: when both allow and deny rules match,
    /// the one with highest specificity wins. On a tie, deny wins.
    pub fn allow(&self, host: &str, ip: Option<IpAddr>) -> bool {
        // Check hostname specificity for both allow and deny matchers
        let allow_hostname_spec = self.allow_matcher.matches_with_specificity(host);
        let deny_hostname_spec = self.deny_matcher.matches_with_specificity(host);

        // Check IP matches (no specificity - either matches or doesn't)
        let allow_ip_match = ip.map(|a| self.allow_matcher.matches_ip(a)).unwrap_or(false);
        let deny_ip_match = ip.map(|a| self.deny_matcher.matches_ip(a)).unwrap_or(false);

        // Apply "longest match wins" logic with deny as tiebreak
        match (allow_hostname_spec, deny_hostname_spec) {
            (Some(allow_spec), Some(deny_spec)) => {
                // Both matched by hostname - more specific wins (deny wins on tie)
                return allow_spec > deny_spec;
            }
            (Some(_), None) => {
                // Only allow matched by hostname
                return true;
            }
            (None, Some(_)) => {
                // Only deny matched by hostname
                return false;
            }
            (None, None) => {
                // No hostname matches, check IP matches
                match (allow_ip_match, deny_ip_match) {
                    (true, true) => {
                        // Both matched by IP, deny wins on tie
                        return false;
                    }
                    (true, false) => return true,
                    (false, true) => return false,
                    (false, false) => {
                        // Neither matched - use default behavior
                        // DefaultMode::Allow: allow by default (return true)
                        // DefaultMode::Deny: deny by default (return false)
                        return self.default == DefaultMode::Allow;
                    }
                }
            }
        }
    }
}
