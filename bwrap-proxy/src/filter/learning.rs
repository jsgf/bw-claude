use crate::config::schema::HostGroup;
use crate::error::{ProxyError, Result};
use chrono::Utc;
use std::collections::HashSet;
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Records accessed hosts and IPs during learning mode
/// Can track both allowed access (--learn) and denied access (--learn-deny)
#[derive(Clone)]
pub struct LearningRecorder {
    // Allowed access recording
    hosts: Arc<Mutex<HashSet<String>>>,
    ipv4_ranges: Arc<Mutex<HashSet<String>>>,
    ipv6_ranges: Arc<Mutex<HashSet<String>>>,
    // Denied access recording (for --learn-deny mode)
    denied_hosts: Arc<Mutex<HashSet<String>>>,
    session_name: String,
}

impl LearningRecorder {
    /// Create a new learning recorder with a timestamped session name
    pub fn new() -> Self {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let session_name = format!("learned_session_{}", timestamp);

        Self {
            hosts: Arc::new(Mutex::new(HashSet::new())),
            ipv4_ranges: Arc::new(Mutex::new(HashSet::new())),
            ipv6_ranges: Arc::new(Mutex::new(HashSet::new())),
            denied_hosts: Arc::new(Mutex::new(HashSet::new())),
            session_name,
        }
    }

    /// Create a recorder with a custom session name
    pub fn with_session_name(name: impl Into<String>) -> Self {
        Self {
            hosts: Arc::new(Mutex::new(HashSet::new())),
            ipv4_ranges: Arc::new(Mutex::new(HashSet::new())),
            ipv6_ranges: Arc::new(Mutex::new(HashSet::new())),
            denied_hosts: Arc::new(Mutex::new(HashSet::new())),
            session_name: name.into(),
        }
    }

    /// Record a host access (skips if already in existing learned file)
    pub fn record_host(&self, host: &str) {
        if let Ok(mut hosts) = self.hosts.lock() {
            hosts.insert(host.to_string());
        }
    }

    /// Record an IP access (skips if already in existing learned file)
    pub fn record_ip(&self, ip: IpAddr) {
        match ip {
            IpAddr::V4(addr) => {
                if let Ok(mut ipv4s) = self.ipv4_ranges.lock() {
                    ipv4s.insert(addr.to_string());
                }
            }
            IpAddr::V6(addr) => {
                if let Ok(mut ipv6s) = self.ipv6_ranges.lock() {
                    ipv6s.insert(addr.to_string());
                }
            }
        }
    }

    /// Record a connection (both host and IP if available)
    /// Automatically skips entries already in the learned file
    pub fn record(&self, host: &str, ip: Option<IpAddr>) {
        self.record_host(host);
        if let Some(addr) = ip {
            self.record_ip(addr);
        }
    }

    /// Record a denied host access (for --learn-deny mode)
    pub fn record_denied_host(&self, host: &str) {
        if let Ok(mut hosts) = self.denied_hosts.lock() {
            hosts.insert(host.to_string());
        }
    }

    /// Record a denied connection (for --learn-deny mode)
    pub fn record_denied(&self, host: &str, _ip: Option<IpAddr>) {
        // For now, we only record the hostname for denials
        // IP addresses in denials are less useful since the policy determines access
        self.record_denied_host(host);
    }

    /// Load existing domains from a TOML file and return them as a set
    /// Used to deduplicate against previously learned domains
    fn load_existing_domains(path: &Path) -> Result<HashSet<String>> {
        if !path.exists() {
            return Ok(HashSet::new());
        }

        let content = fs::read_to_string(path).map_err(ProxyError::from)?;
        let config: toml::Table = toml::from_str(&content).map_err(ProxyError::from)?;

        let mut existing = HashSet::new();

        // Extract all hosts from all groups in the config
        if let Some(network) = config.get("network").and_then(|v| v.as_table()) {
            if let Some(groups) = network.get("groups").and_then(|v| v.as_table()) {
                for (_group_name, group_value) in groups {
                    if let Some(group_table) = group_value.as_table() {
                        // Extract hosts array
                        if let Some(hosts_array) = group_table.get("hosts").and_then(|v| v.as_array()) {
                            for host in hosts_array {
                                if let Some(host_str) = host.as_str() {
                                    existing.insert(host_str.to_string());
                                }
                            }
                        }
                        // Extract IPv4 ranges
                        if let Some(ipv4_array) = group_table.get("ipv4_ranges").and_then(|v| v.as_array()) {
                            for ip in ipv4_array {
                                if let Some(ip_str) = ip.as_str() {
                                    existing.insert(ip_str.to_string());
                                }
                            }
                        }
                        // Extract IPv6 ranges
                        if let Some(ipv6_array) = group_table.get("ipv6_ranges").and_then(|v| v.as_array()) {
                            for ip in ipv6_array {
                                if let Some(ip_str) = ip.as_str() {
                                    existing.insert(ip_str.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(existing)
    }

    /// Get the session name
    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    /// Get a snapshot of recorded data as a HostGroup
    pub fn to_host_group(&self) -> HostGroup {
        let hosts = self.hosts.lock()
            .map(|h| h.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        let ipv4_ranges = self.ipv4_ranges.lock()
            .map(|h| h.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        let ipv6_ranges = self.ipv6_ranges.lock()
            .map(|h| h.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        HostGroup {
            description: self.session_name.clone(),
            hosts,
            hosts_deny: Vec::new(),
            ipv4_ranges,
            ipv6_ranges,
            groups: Vec::new(),
        }
    }

    /// Get denied hosts as a HostGroup (for --learn-deny mode)
    pub fn to_denied_host_group(&self) -> HostGroup {
        let denied_hosts = self.denied_hosts.lock()
            .map(|h| h.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        HostGroup {
            description: format!("{}_denied", self.session_name),
            hosts: denied_hosts,
            hosts_deny: Vec::new(),
            ipv4_ranges: Vec::new(),
            ipv6_ranges: Vec::new(),
            groups: Vec::new(),
        }
    }

    /// Save recorded data to a TOML file
    ///
    /// The data is appended as a new group to the existing config file.
    /// Automatically deduplicates against any existing entries in the file.
    /// Only new entries discovered in this session are saved.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let mut host_group = self.to_host_group();

        // Load existing domains from the file and filter them out
        if path.exists() {
            let existing = Self::load_existing_domains(path)?;

            // Remove any entries that already exist
            host_group.hosts.retain(|h| !existing.contains(h));
            host_group.ipv4_ranges.retain(|ip| !existing.contains(ip));
            host_group.ipv6_ranges.retain(|ip| !existing.contains(ip));
        }

        // Check if we have any NEW data to save after deduplication
        if host_group.hosts.is_empty()
            && host_group.ipv4_ranges.is_empty()
            && host_group.ipv6_ranges.is_empty() {
            return Ok(()); // Nothing new to save
        }

        // Read existing config or create minimal structure
        let mut config_content = if path.exists() {
            fs::read_to_string(path).map_err(ProxyError::from)?
        } else {
            // Create minimal config structure
            String::from("[common]\nlog_level = \"info\"\n\n[network]\n\n")
        };

        // Generate the new group section (with only new entries)
        let group_section = format!(
            "[network.groups.{}]\n{}{}{}\n",
            self.session_name,
            format_toml_array("hosts", &host_group.hosts),
            format_toml_array("ipv4_ranges", &host_group.ipv4_ranges),
            format_toml_array("ipv6_ranges", &host_group.ipv6_ranges),
        );

        // Append the new group
        config_content.push_str(&group_section);

        // Write back to file
        fs::write(path, config_content).map_err(ProxyError::from)?;

        Ok(())
    }

    /// Save denied hosts to a TOML file (for --learn-deny mode)
    /// Saves denied hosts to a group that can be used to create deny policies
    pub fn save_denied_to_file(&self, path: &Path) -> Result<()> {
        let mut host_group = self.to_denied_host_group();

        // Load existing domains from the file and filter them out
        if path.exists() {
            let existing = Self::load_existing_domains(path)?;

            // Remove any entries that already exist
            host_group.hosts.retain(|h| !existing.contains(h));
        }

        // Check if we have any NEW denied data to save after deduplication
        if host_group.hosts.is_empty() {
            return Ok(()); // Nothing new to save
        }

        // Read existing config or create minimal structure
        let mut config_content = if path.exists() {
            fs::read_to_string(path).map_err(ProxyError::from)?
        } else {
            // Create minimal config structure
            String::from("[common]\nlog_level = \"info\"\n\n[network]\n\n")
        };

        // Generate the new group section (with only new denied entries)
        let group_section = format!(
            "[network.groups.{}]\n{}\n",
            host_group.description,
            format_toml_array("hosts", &host_group.hosts),
        );

        // Append the new group
        config_content.push_str(&group_section);

        // Write back to file
        fs::write(path, config_content).map_err(ProxyError::from)?;

        Ok(())
    }

    /// Get statistics about recorded data
    pub fn stats(&self) -> LearningStats {
        LearningStats {
            host_count: self.hosts.lock().map(|h| h.len()).unwrap_or(0),
            ipv4_count: self.ipv4_ranges.lock().map(|h| h.len()).unwrap_or(0),
            ipv6_count: self.ipv6_ranges.lock().map(|h| h.len()).unwrap_or(0),
        }
    }
}

impl Default for LearningRecorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about recorded learning data
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearningStats {
    pub host_count: usize,
    pub ipv4_count: usize,
    pub ipv6_count: usize,
}

impl LearningStats {
    pub fn total(&self) -> usize {
        self.host_count + self.ipv4_count + self.ipv6_count
    }
}

/// Helper function to format a TOML array
fn format_toml_array(key: &str, values: &[String]) -> String {
    if values.is_empty() {
        return String::new();
    }

    let mut result = format!("{} = [\n", key);
    for value in values {
        result.push_str(&format!("  \"{}\",\n", value));
    }
    result.push_str("]\n");
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use tempfile::NamedTempFile;

    #[test]
    fn test_record_host() {
        let recorder = LearningRecorder::new();
        recorder.record_host("example.com");
        recorder.record_host("api.example.com");
        recorder.record_host("example.com"); // Duplicate

        let group = recorder.to_host_group();
        assert_eq!(group.hosts.len(), 2);
        assert!(group.hosts.contains(&"example.com".to_string()));
        assert!(group.hosts.contains(&"api.example.com".to_string()));
    }

    #[test]
    fn test_record_ip() {
        let recorder = LearningRecorder::new();
        let ipv4 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let ipv6 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));

        recorder.record_ip(ipv4);
        recorder.record_ip(ipv6);

        let group = recorder.to_host_group();
        assert_eq!(group.ipv4_ranges.len(), 1);
        assert_eq!(group.ipv6_ranges.len(), 1);
    }

    #[test]
    fn test_record_combined() {
        let recorder = LearningRecorder::new();
        let ip = IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34));

        recorder.record("example.com", Some(ip));

        let group = recorder.to_host_group();
        assert_eq!(group.hosts.len(), 1);
        assert_eq!(group.ipv4_ranges.len(), 1);
        assert!(group.hosts.contains(&"example.com".to_string()));
    }

    #[test]
    fn test_stats() {
        let recorder = LearningRecorder::new();
        recorder.record_host("example.com");
        recorder.record_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        recorder.record_ip(IpAddr::V6(Ipv6Addr::LOCALHOST));

        let stats = recorder.stats();
        assert_eq!(stats.host_count, 1);
        assert_eq!(stats.ipv4_count, 1);
        assert_eq!(stats.ipv6_count, 1);
        assert_eq!(stats.total(), 3);
    }

    #[test]
    fn test_save_to_file() {
        let recorder = LearningRecorder::with_session_name("test_session");
        recorder.record_host("example.com");
        recorder.record_host("api.example.com");
        recorder.record_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));

        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        recorder.save_to_file(path).unwrap();

        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("[network.groups.test_session]"));
        assert!(content.contains("example.com"));
        assert!(content.contains("api.example.com"));
        assert!(content.contains("192.168.1.1"));
    }

    #[test]
    fn test_save_empty_recorder() {
        let recorder = LearningRecorder::new();
        let temp_file = NamedTempFile::new().unwrap();

        // Should not error when saving empty recorder
        recorder.save_to_file(temp_file.path()).unwrap();

        // File should not exist or be empty
        if temp_file.path().exists() {
            let content = fs::read_to_string(temp_file.path()).unwrap();
            assert!(!content.contains("[network.groups"));
        }
    }

    #[test]
    fn test_thread_safety() {
        use std::thread;

        let recorder = LearningRecorder::new();
        let mut handles = vec![];

        for i in 0..10 {
            let rec = recorder.clone();
            let handle = thread::spawn(move || {
                rec.record_host(&format!("host{}.com", i));
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let stats = recorder.stats();
        assert_eq!(stats.host_count, 10);
    }

    #[test]
    fn test_format_toml_array() {
        let values = vec!["host1.com".to_string(), "host2.com".to_string()];
        let result = format_toml_array("hosts", &values);

        assert!(result.contains("hosts = ["));
        assert!(result.contains("\"host1.com\""));
        assert!(result.contains("\"host2.com\""));
    }

    #[test]
    fn test_format_toml_array_empty() {
        let values: Vec<String> = vec![];
        let result = format_toml_array("hosts", &values);
        assert_eq!(result, "");
    }
}
