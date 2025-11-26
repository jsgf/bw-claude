use crate::config::schema::NetworkConfig;
use crate::error::Result;
use crate::filter::{LearningRecorder, PolicyEngine};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::UnixListener;
use tracing::{debug, info};

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
            "Proxy listening on {:?}",
            self.config.socket_path
        );

        let mut first_connection = true;
        loop {
            let (socket, _) = listener.accept().await
                .map_err(|e| crate::error::ProxyError::from(e))?;

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
    }
}

/// Handle a single client connection over UDS
///
/// Currently uses a simple text protocol: "CONNECT host port\n"
/// TODO: Implement proper SOCKS5 protocol
async fn handle_client(
    mut stream: tokio::net::UnixStream,
    _config: ProxyServerConfig,
) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    debug!("Client connected via Unix domain socket");

    // Read the CONNECT request from bw-relay
    // Format: "CONNECT host port\n" (simple text protocol)
    // TODO: Use proper SOCKS5 protocol here
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
        };

        let server = ProxyServer::new(config);
        // Just verify it can be created without panicking
        assert_eq!(
            server.config.socket_path,
            socket_path
        );
    }
}
