//! Proxy server initialization and management

use anyhow::{Context, Result};
use bwrap_proxy::{ConfigLoader, ProxyServer, ProxyServerConfig};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

/// Create and spawn the proxy server as an async task
///
/// This function:
/// 1. Generates a unique socket path in /tmp
/// 2. Loads proxy configuration from file or uses defaults
/// 3. Spawns the proxy server as a tokio task (runs until parent exits)
/// 4. Waits for the proxy to be ready (listening on the socket)
/// 5. Returns the socket path for mounting in the sandbox
pub async fn create_proxy_task(config_path: &Option<PathBuf>) -> Result<PathBuf> {
    // Generate a unique socket path in /tmp
    let session_id = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .context("Failed to get system time")?
        .as_nanos();
    let socket_path = PathBuf::from(format!("/tmp/bw-proxy-{}.sock", session_id));

    // Load configuration
    let config = ConfigLoader::load_or_default(config_path.clone())
        .context("Failed to load proxy configuration")?;

    // Create proxy server
    let proxy_config = ProxyServerConfig {
        socket_path: socket_path.clone(),
        network_config: Arc::new(config.network),
        policy_engine: None, // Default to open mode
        learning_recorder: None,
    };

    let proxy = ProxyServer::new(proxy_config);

    // Spawn the proxy as an async task (will run until parent exits)
    tokio::spawn(async move {
        if let Err(e) = proxy.start().await {
            tracing::error!("Proxy server error: {}", e);
        }
    });

    // Wait for the socket to be created and bound
    let socket_check = socket_path.clone();
    loop {
        if socket_check.exists() {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    }

    Ok(socket_path)
}
