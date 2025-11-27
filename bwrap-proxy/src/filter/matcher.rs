//! Host and IP matching logic

use ipnet::{Ipv4Net, Ipv6Net};
use std::net::IpAddr;
use wildmatch::WildMatch;

/// Matcher for hosts and IP addresses
#[derive(Debug, Clone)]
pub struct HostMatcher {
    patterns: Vec<WildMatch>,
    ipv4_ranges: Vec<Ipv4Net>,
    ipv6_ranges: Vec<Ipv6Net>,
}

impl HostMatcher {
    /// Create a new empty matcher
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
            ipv4_ranges: Vec::new(),
            ipv6_ranges: Vec::new(),
        }
    }

    /// Add a wildcard pattern for host matching
    pub fn add_pattern(&mut self, pattern: &str) {
        self.patterns.push(WildMatch::new(pattern));
    }

    /// Add an IPv4 CIDR range
    pub fn add_ipv4_range(&mut self, range: Ipv4Net) {
        self.ipv4_ranges.push(range);
    }

    /// Add an IPv6 CIDR range
    pub fn add_ipv6_range(&mut self, range: Ipv6Net) {
        self.ipv6_ranges.push(range);
    }

    /// Check if a hostname matches any pattern
    pub fn matches_host(&self, host: &str) -> bool {
        self.patterns.iter().any(|p| p.matches(host))
    }

    /// Check if an IP address matches any range
    pub fn matches_ip(&self, ip: IpAddr) -> bool {
        match ip {
            IpAddr::V4(ipv4) => self.ipv4_ranges.iter().any(|net| net.contains(&ipv4)),
            IpAddr::V6(ipv6) => self.ipv6_ranges.iter().any(|net| net.contains(&ipv6)),
        }
    }

    /// Check if either hostname or IP matches
    pub fn matches(&self, host: &str, ip: Option<IpAddr>) -> bool {
        if self.matches_host(host) {
            return true;
        }

        if let Some(addr) = ip {
            return self.matches_ip(addr);
        }

        false
    }

    /// Check if matcher has any patterns or ranges
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty() && self.ipv4_ranges.is_empty() && self.ipv6_ranges.is_empty()
    }

    /// Check if host matches with specificity calculation
    /// Returns Some(specificity) if matched, None if no match
    /// Specificity = count of non-wildcard domain elements in the matched hostname
    pub fn matches_with_specificity(&self, host: &str) -> Option<usize> {
        let mut max_specificity = None;

        // Check host patterns
        for pattern in &self.patterns {
            if pattern.matches(host) {
                let spec = calculate_hostname_specificity(host);
                max_specificity = Some(max_specificity.unwrap_or(0).max(spec));
            }
        }

        max_specificity
    }
}

/// Calculate specificity of a hostname for matching purposes
/// Specificity = count of non-wildcard domain elements
/// Example: "api.example.com" = 3, "test.org" = 2
fn calculate_hostname_specificity(host: &str) -> usize {
    host.split('.').count()
}

impl Default for HostMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_wildcard_matching() {
        let mut matcher = HostMatcher::new();
        matcher.add_pattern("*.example.com");
        matcher.add_pattern("test.*.org");

        assert!(matcher.matches_host("foo.example.com"));
        assert!(matcher.matches_host("bar.example.com"));
        assert!(matcher.matches_host("test.something.org"));
        assert!(!matcher.matches_host("example.com"));
        assert!(!matcher.matches_host("something.org"));
    }

    #[test]
    fn test_ipv4_matching() {
        let mut matcher = HostMatcher::new();
        let range: Ipv4Net = "192.168.1.0/24".parse().unwrap();
        matcher.add_ipv4_range(range);

        let ip_in = IpAddr::V4("192.168.1.100".parse::<Ipv4Addr>().unwrap());
        let ip_out = IpAddr::V4("192.168.2.100".parse::<Ipv4Addr>().unwrap());

        assert!(matcher.matches_ip(ip_in));
        assert!(!matcher.matches_ip(ip_out));
    }

    #[test]
    fn test_combined_matching() {
        let mut matcher = HostMatcher::new();
        matcher.add_pattern("*.example.com");
        let range: Ipv4Net = "10.0.0.0/8".parse().unwrap();
        matcher.add_ipv4_range(range);

        let ip = Some(IpAddr::V4("10.1.2.3".parse::<Ipv4Addr>().unwrap()));

        assert!(matcher.matches("foo.example.com", None));
        assert!(matcher.matches("anything.com", ip));
        assert!(!matcher.matches("other.org", None));
    }

    #[test]
    fn test_specificity_matching() {
        let mut matcher = HostMatcher::new();
        matcher.add_pattern("*.example.com");
        matcher.add_pattern("*.api.example.com");

        // More specific pattern should win
        assert_eq!(matcher.matches_with_specificity("test.api.example.com"), Some(4));
        assert_eq!(matcher.matches_with_specificity("test.example.com"), Some(3));

        // No match
        assert_eq!(matcher.matches_with_specificity("other.org"), None);
    }

    #[test]
    fn test_hostname_specificity() {
        assert_eq!(calculate_hostname_specificity("api.example.com"), 3);
        assert_eq!(calculate_hostname_specificity("test.api.example.com"), 4);
        assert_eq!(calculate_hostname_specificity("localhost"), 1);
        assert_eq!(calculate_hostname_specificity("example.com"), 2);
    }
}
