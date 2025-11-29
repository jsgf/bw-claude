use super::schema::Config;
use super::loader::ConfigLoader;
use crate::error::{Result, SandboxError};
use bwrap_proxy::filter::LearningRecorderTrait;
use bwrap_proxy::config::HostGroup;
use chrono::Utc;
use indexmap::IndexMap;
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Records accessed hosts and IPs during learning mode
/// Maintains the config file as an in-memory data structure
/// and periodically writes it to disk
#[derive(Clone)]
pub struct LearningRecorder {
    // In-memory config that tracks the file state
    config: Arc<Mutex<Config>>,
    // Path to the learning output file
    output_path: Arc<Mutex<Option<PathBuf>>>,
    session_name: String,
}

impl LearningRecorder {
    /// Create a new learning recorder with a timestamped session name
    pub fn new() -> Self {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let session_name = format!("learned_session_{}", timestamp);

        Self {
            config: Arc::new(Mutex::new(Config::default())),
            output_path: Arc::new(Mutex::new(None)),
            session_name,
        }
    }

    /// Create a recorder with a custom session name
    pub fn with_session_name(name: impl Into<String>) -> Self {
        Self {
            config: Arc::new(Mutex::new(Config::default())),
            output_path: Arc::new(Mutex::new(None)),
            session_name: name.into(),
        }
    }

    /// Initialize the recorder with a file path and load existing config
    pub fn with_output_path(name: impl Into<String>, path: PathBuf) -> Result<Self> {
        let session_name = name.into();

        // For learning mode, try to load only the existing file (no built-in merge)
        // If it doesn't exist or fails to load, start with empty config
        let config = match ConfigLoader::load_from_file(&path) {
            Ok(cfg) => cfg,
            Err(_) => {
                // File doesn't exist or failed to load - start fresh with empty config
                Config {
                    common: Default::default(),
                    network: Default::default(),
                    filesystem: Default::default(),
                    policy: Default::default(),
                    tools: IndexMap::new(),
                }
            }
        };

        Ok(Self {
            config: Arc::new(Mutex::new(config)),
            output_path: Arc::new(Mutex::new(Some(path))),
            session_name,
        })
    }

    /// Record a host access
    pub fn record_host(&self, host: &str) {
        if let Ok(mut config) = self.config.lock() {
            let group = config
                .network
                .groups
                .entry(self.session_name.clone())
                .or_insert_with(|| HostGroup {
                    description: self.session_name.clone(),
                    hosts: Vec::new(),
                    hosts_deny: Vec::new(),
                    groups: Vec::new(),
                });

            if !group.hosts.contains(&host.to_string()) {
                group.hosts.push(host.to_string());
            }
        }
    }

    /// Record an IP access
    /// Note: IP recording is no longer supported after removal of ipv4/v6 filtering
    pub fn record_ip(&self, _ip: IpAddr) {
        // No-op: IPs are no longer tracked after removal of IP range filtering
    }

    /// Record a connection (both host and IP if available)
    pub fn record(&self, host: &str, ip: Option<IpAddr>) {
        self.record_host(host);
        if let Some(addr) = ip {
            self.record_ip(addr);
        }
    }

    /// Record a denied host access (for --learn-deny mode)
    pub fn record_denied_host(&self, host: &str) {
        if let Ok(mut config) = self.config.lock() {
            let denied_group_name = format!("{}_denied", self.session_name);
            let group = config
                .network
                .groups
                .entry(denied_group_name.clone())
                .or_insert_with(|| HostGroup {
                    description: denied_group_name,
                    hosts: Vec::new(),
                    hosts_deny: Vec::new(),
                    groups: Vec::new(),
                });

            if !group.hosts.contains(&host.to_string()) {
                group.hosts.push(host.to_string());
            }
        }
    }

    /// Record a denied connection (for --learn-deny mode)
    pub fn record_denied(&self, host: &str, _ip: Option<IpAddr>) {
        // For now, we only record the hostname for denials
        // IP addresses in denials are less useful since the policy determines access
        self.record_denied_host(host);
    }

    /// Flush the in-memory config to disk
    pub fn flush(&self) -> Result<()> {
        let output_path = self.output_path.lock().ok()
            .and_then(|path| path.as_ref().cloned());

        if let Some(path) = output_path {
            let config = self.config.lock()
                .map_err(|_| SandboxError::ConfigError("Failed to acquire config lock".to_string()))?;

            let toml_str = toml::to_string_pretty(&*config)
                .map_err(|e| SandboxError::ConfigError(format!("Failed to serialize config: {e}")))?;

            fs::write(&path, toml_str)?;
        }

        Ok(())
    }

    /// Get statistics about recorded data
    pub fn stats(&self) -> LearningStats {
        if let Ok(config) = self.config.lock() {
            let host_count = config.network.groups.get(&self.session_name)
                .map(|g| g.hosts.len())
                .unwrap_or(0);

            LearningStats { host_count }
        } else {
            LearningStats { host_count: 0 }
        }
    }

    /// Get session name
    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    /// Set the output path (for late initialization)
    pub fn set_output_path(&self, path: PathBuf) -> Result<()> {
        // For learning mode, try to load only the existing file (no built-in merge)
        // If it doesn't exist or fails to load, start with empty config
        let config = match ConfigLoader::load_from_file(&path) {
            Ok(cfg) => cfg,
            Err(_) => {
                // File doesn't exist or failed to load - start fresh with empty config
                Config {
                    common: Default::default(),
                    network: Default::default(),
                    filesystem: Default::default(),
                    policy: Default::default(),
                    tools: IndexMap::new(),
                }
            }
        };

        if let Ok(mut cfg_lock) = self.config.lock() {
            *cfg_lock = config;
        }

        if let Ok(mut output) = self.output_path.lock() {
            *output = Some(path);
        }

        Ok(())
    }
}

impl Default for LearningRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl LearningRecorderTrait for LearningRecorder {
    fn record_host(&self, host: &str) {
        LearningRecorder::record_host(self, host);
    }

    fn record_denied_host(&self, host: &str) {
        LearningRecorder::record_denied_host(self, host);
    }

    fn record(&self, host: &str, ip: Option<IpAddr>) {
        LearningRecorder::record(self, host, ip);
    }

    fn record_denied(&self, host: &str, ip: Option<IpAddr>) {
        LearningRecorder::record_denied(self, host, ip);
    }

    fn flush(&self) -> std::result::Result<(), String> {
        LearningRecorder::flush(self)
            .map_err(|e| format!("Failed to flush learning recorder: {:?}", e))
    }
}

/// Statistics about recorded learning data
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearningStats {
    pub host_count: usize,
}

impl LearningStats {
    pub fn total(&self) -> usize {
        self.host_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_record_host() {
        let recorder = LearningRecorder::new();
        recorder.record_host("example.com");
        recorder.record_host("api.example.com");
        recorder.record_host("example.com"); // Duplicate

        let stats = recorder.stats();
        assert_eq!(stats.host_count, 2);
    }

    #[test]
    fn test_record_ip() {
        let recorder = LearningRecorder::new();
        let ipv4 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let ipv6 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));

        // IP recording is now a no-op since IP filtering was removed
        recorder.record_ip(ipv4);
        recorder.record_ip(ipv6);

        let stats = recorder.stats();
        assert_eq!(stats.host_count, 0);
    }

    #[test]
    fn test_record_denied_host() {
        let recorder = LearningRecorder::new();
        recorder.record_denied_host("blocked.com");
        recorder.record_denied_host("blocked.com"); // Duplicate

        let config = recorder.config.lock().unwrap();
        let denied_group_name = format!("{}_denied", recorder.session_name);
        let group = config.network.groups.get(&denied_group_name).unwrap();
        assert_eq!(group.hosts.len(), 1);
    }

    #[test]
    fn test_stats() {
        let recorder = LearningRecorder::new();
        recorder.record_host("example.com");
        // IP recording is now a no-op since IP filtering was removed
        recorder.record_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        recorder.record_ip(IpAddr::V6(Ipv6Addr::LOCALHOST));

        let stats = recorder.stats();
        assert_eq!(stats.host_count, 1);
        assert_eq!(stats.total(), 1);
    }

    #[test]
    fn test_flush_to_file() {
        use std::fs;
        use tempfile::NamedTempFile;

        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_path_buf();

        // Create recorder and set output path
        let recorder = LearningRecorder::new();
        recorder.set_output_path(file_path.clone()).unwrap();

        // Record some data
        recorder.record_host("example.com");
        recorder.record_host("api.example.com");
        // IP recording is now a no-op since IP filtering was removed
        recorder.record_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));

        // Flush to file
        recorder.flush().unwrap();

        // Verify file was written
        let content = fs::read_to_string(&file_path).unwrap();
        eprintln!("Flushed content:\n{content}");
        assert!(content.contains("example.com"), "Content missing example.com");
        assert!(content.contains("api.example.com"), "Content missing api.example.com");
    }

    #[test]
    fn test_flush_denied_hosts() {
        use std::fs;
        use tempfile::NamedTempFile;

        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_path_buf();

        // Create recorder and set output path
        let recorder = LearningRecorder::with_output_path("test_session", file_path.clone()).unwrap();

        // Record denied hosts
        recorder.record_denied_host("blocked.com");
        recorder.record_denied_host("malware.com");

        // Flush to file
        recorder.flush().unwrap();

        // Verify file was written with denied group
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("test_session_denied"));
        assert!(content.contains("blocked.com"));
        assert!(content.contains("malware.com"));
    }
}
