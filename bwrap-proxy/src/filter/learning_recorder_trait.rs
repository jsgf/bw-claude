//! Learning recorder trait for recording network access

use std::net::IpAddr;

/// Trait for recording network access during learning mode
pub trait LearningRecorderTrait: Send + Sync {
    /// Record a successful host access
    fn record_host(&self, host: &str);

    /// Record a denied host access
    fn record_denied_host(&self, host: &str);

    /// Record both hostname and IP address
    fn record(&self, host: &str, ip: Option<IpAddr>);

    /// Record a denied connection
    fn record_denied(&self, host: &str, ip: Option<IpAddr>);

    /// Flush recorded data to persistence
    fn flush(&self) -> Result<(), String>;
}
