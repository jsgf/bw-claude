use crate::config::schema::NetworkConfig;
use crate::error::Result;
use crate::filter::{LearningRecorder, PolicyEngine};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::UnixListener;
use tracing::{debug, info};

/// Policy filtering proxy server configuration
/// Communicates with bw-relay via a simple text protocol over Unix Domain Socket
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
    /// Optional path to save learning data on shutdown
    pub learning_output: Option<PathBuf>,
}

/// Policy filtering proxy server
/// Listens for connection requests from bw-relay (HTTP CONNECT proxy)
/// and applies policy-based filtering before allowing connections
pub struct ProxyServer {
    config: ProxyServerConfig,
}

impl ProxyServer {
    /// Create a new proxy server with the given configuration
    pub fn new(config: ProxyServerConfig) -> Self {
        Self { config }
    }

    /// Start the proxy server listening on the Unix domain socket
    /// Saves learning data on shutdown if learning_output is configured
    pub async fn start(&self) -> Result<()> {
        // Remove existing socket if it exists
        let _ = std::fs::remove_file(&self.config.socket_path);

        // Create Unix domain socket listener
        let listener = UnixListener::bind(&self.config.socket_path)
            .map_err(|e| crate::error::ProxyError::from(e))?;

        info!(
            "Proxy listening on {:?}",
            self.config.socket_path
        );

        // Set up signal handlers for graceful shutdown
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;

        let mut first_connection = true;
        loop {
            tokio::select! {
                // Accept new connections
                result = listener.accept() => {
                    let (socket, _) = result.map_err(|e| crate::error::ProxyError::from(e))?;

                    // After accepting the first connection, unlink the socket on the host.
                    // The bind mount inside the container continues to work (kernel keeps inode alive).
                    // This prevents other processes from connecting to this socket.
                    if first_connection {
                        first_connection = false;
                        let _ = std::fs::remove_file(&self.config.socket_path);
                        debug!("Socket unlinked after first connection");
                    }

                    let config = self.config.clone();

                    // Spawn a task for each connection
                    tokio::spawn(async move {
                        let _ = handle_client(socket, config).await;
                    });
                }

                // Handle signals for graceful shutdown
                _ = sigterm.recv() => {
                    info!("Received SIGTERM, saving learning data and shutting down");
                    self.save_learning_data();
                    break;
                }

                _ = sigint.recv() => {
                    info!("Received SIGINT, saving learning data and shutting down");
                    self.save_learning_data();
                    break;
                }
            }
        }

        Ok(())
    }

    /// Save learning data to file if learning mode is active
    fn save_learning_data(&self) {
        if let (Some(ref recorder), Some(ref output_path)) = (&self.config.learning_recorder, &self.config.learning_output) {
            match recorder.save_to_file(output_path) {
                Ok(_) => {
                    info!("Learning data saved to {:?}", output_path);
                }
                Err(e) => {
                    tracing::error!("Failed to save learning data: {}", e);
                }
            }
        }
    }
}

/// Handle a single client connection over UDS
///
/// Protocol: Simple text-based CONNECT protocol
/// Format: "CONNECT host port\n"
/// Response: "OK\n", "BLOCKED\n", "FAIL\n", or "ERROR\n"
///
/// Filters connections based on the policy engine before allowing them through.
async fn handle_client(
    mut stream: tokio::net::UnixStream,
    config: ProxyServerConfig,
) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    debug!("Client connected via Unix domain socket");

    // Read the CONNECT request from bw-relay via UDS
    // Format: "CONNECT host port\n"
    let mut buf = vec![0u8; 1024];
    debug!("Reading request from client");
    let n = stream.read(&mut buf).await?;
    debug!("Received {} bytes from client", n);

    if n == 0 {
        debug!("No data received from client");
        return Ok(());
    }

    let request_str = String::from_utf8_lossy(&buf[..n]);
    debug!("Raw request: {:?}", request_str);
    let request_str = request_str.trim();
    debug!("Trimmed request: {:?}", request_str);

    if !request_str.starts_with("CONNECT ") {
        debug!("Invalid request format: {:?}", request_str);
        let _ = stream.write_all(b"ERROR\n").await;
        return Ok(());
    }

    // Parse "CONNECT host port"
    let parts: Vec<&str> = request_str.split_whitespace().collect();
    debug!("Parsed request parts: {:?}", parts);
    if parts.len() != 3 {
        debug!("Invalid request format: expected 3 parts, got {}", parts.len());
        let _ = stream.write_all(b"ERROR\n").await;
        return Ok(());
    }

    let host = parts[1];
    let port: u16 = match parts[2].parse() {
        Ok(p) => p,
        Err(_) => {
            debug!("Invalid port number: {}", parts[2]);
            let _ = stream.write_all(b"ERROR\n").await;
            return Ok(());
        }
    };

    debug!("CONNECT request: {}:{}", host, port);

    // Apply policy filtering if a policy engine is configured
    if let Some(ref policy_engine) = config.policy_engine {
        let allowed = policy_engine.allow(host, None);
        debug!("Policy check for {}: allowed={}", host, allowed);

        if !allowed {
            debug!("Connection blocked by policy: {}:{}", host, port);
            let _ = stream.write_all(b"BLOCKED\n").await;
            return Ok(());
        }
    }

    // Record access in learning mode if enabled
    if let Some(ref learning_recorder) = config.learning_recorder {
        learning_recorder.record(host, None);

        // Save learning data immediately after recording
        if let Some(ref output_path) = config.learning_output {
            if let Err(e) = learning_recorder.save_to_file(output_path) {
                debug!("Failed to save learning data: {}", e);
            }
        }
    }

    // Try to connect to the destination
    debug!("Attempting to connect to {}:{}", host, port);
    match tokio::net::TcpStream::connect(format!("{}:{}", host, port)).await {
        Ok(mut remote) => {
            debug!("Connection succeeded to {}:{}", host, port);
            // Send success response
            stream.write_all(b"OK\n").await?;
            stream.flush().await?;

            // Tunnel data bidirectionally between client and remote
            if let Err(e) = tokio::io::copy_bidirectional(&mut stream, &mut remote).await {
                debug!("Tunnel error: {}", e);
            }

            debug!("Tunnel closed for {}:{}", host, port);
            Ok(())
        }
        Err(e) => {
            // Only log remote connection failures at debug level (not failures)
            debug!("Remote connection failed to {}:{}: {}", host, port, e);
            let _ = stream.write_all(b"FAIL\n").await;
            Ok(())
        }
    }
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
            learning_output: None,
        };

        let server = ProxyServer::new(config);
        // Just verify it can be created without panicking
        assert_eq!(
            server.config.socket_path,
            socket_path
        );
    }
}
