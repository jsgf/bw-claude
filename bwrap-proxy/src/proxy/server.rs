use crate::config::schema::NetworkConfig;
use crate::error::Result;
use crate::filter::{LearningRecorder, PolicyEngine};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::UnixListener;
use tracing::{debug, info, warn};

/// SOCKS5 proxy server configuration
#[derive(Clone)]
pub struct ProxyServerConfig {
    /// Unix domain socket path to listen on
    pub socket_path: PathBuf,
    /// Network configuration with policies and groups
    pub network_config: Arc<NetworkConfig>,
    /// Policy engine for evaluation
    pub policy_engine: Option<Arc<PolicyEngine>>,
    /// Learning recorder for learning mode
    pub learning_recorder: Option<Arc<LearningRecorder>>,
}

/// SOCKS5 proxy server
pub struct ProxyServer {
    config: ProxyServerConfig,
}

impl ProxyServer {
    /// Create a new proxy server with the given configuration
    pub fn new(config: ProxyServerConfig) -> Self {
        Self { config }
    }

    /// Start the proxy server listening on the Unix domain socket
    pub async fn start(&self) -> Result<()> {
        // Remove existing socket if it exists
        let _ = std::fs::remove_file(&self.config.socket_path);

        // Create Unix domain socket listener
        let listener = UnixListener::bind(&self.config.socket_path)
            .map_err(|e| crate::error::ProxyError::from(e))?;

        info!(
            "SOCKS5 proxy listening on {:?}",
            self.config.socket_path
        );

        loop {
            let (socket, _) = listener.accept().await
                .map_err(|e| crate::error::ProxyError::from(e))?;

            let config = self.config.clone();

            // Spawn a task for each connection
            tokio::spawn(async move {
                if let Err(e) = handle_client(socket, config).await {
                    warn!("Error handling client: {}", e);
                }
            });
        }
    }
}

/// Handle a single SOCKS5 client connection
async fn handle_client(
    _stream: tokio::net::UnixStream,
    _config: ProxyServerConfig,
) -> Result<()> {
    // For now, just accept the connection and close it
    // This is a placeholder - full SOCKS5 implementation would parse the protocol here
    // In Phase 2, bw-relay will handle the SOCKS5 protocol translation
    // This outside proxy will just receive connections from bw-relay over the UDS

    debug!("Client connected via Unix domain socket");

    // TODO: Implement SOCKS5 protocol handling
    // For Phase 1, we're just accepting connections
    // Phase 2 (bw-relay) will handle actual SOCKS5 protocol

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_proxy_server_creation() {
        let socket_path = NamedTempFile::new().unwrap().path().to_path_buf();
        let config = ProxyServerConfig {
            socket_path: socket_path.clone(),
            network_config: Arc::new(Default::default()),
            policy_engine: None,
            learning_recorder: None,
        };

        let server = ProxyServer::new(config);
        // Just verify it can be created without panicking
        assert_eq!(
            server.config.socket_path,
            socket_path
        );
    }
}
