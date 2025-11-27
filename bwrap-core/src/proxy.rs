//! Proxy server initialization and management

use anyhow::{Context, Result};
use bwrap_proxy::{ConfigLoader, LearningRecorder, PolicyEngine, ProxyServer, ProxyServerConfig};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

/// Create and spawn the proxy server as an async task
///
/// This function:
/// 1. Generates a unique socket path in /tmp
/// 2. Loads proxy configuration from file or uses defaults
/// 3. Creates a PolicyEngine for the specified policy
/// 4. Creates a LearningRecorder if learning_output is specified
/// 5. Spawns the proxy server as a tokio task (runs until parent exits)
/// 6. Waits for the proxy to be ready (listening on the socket)
/// 7. Returns the socket path and learning mode (if active)
///
/// Note: The proxy will save learning data on shutdown via a cleanup function
pub async fn create_proxy_task(
    config_path: &Option<PathBuf>,
    policy_name: Option<&str>,
    learning_output: Option<&PathBuf>,
    learning_mode: Option<String>,
) -> Result<(PathBuf, Option<String>)> {
    // Generate a unique socket path in /tmp
    let session_id = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .context("Failed to get system time")?
        .as_nanos();
    let socket_path = PathBuf::from(format!("/tmp/bw-proxy-{}.sock", session_id));

    // Load configuration
    let config = ConfigLoader::load_or_default(config_path.clone())
        .context("Failed to load proxy configuration")?;

    // Create PolicyEngine if a policy name is specified
    let policy_engine = if let Some(policy_name) = policy_name {
        match policy_name {
            "open" => None, // "open" means no filtering
            policy => {
                Some(Arc::new(
                    PolicyEngine::from_policy(policy, &config.network)
                        .context(format!("Failed to load policy: {}", policy))?,
                ))
            }
        }
    } else {
        None
    };

    // Create LearningRecorder if learning output path is specified
    // Note: The output path will be used when the recorder is saved
    let learning_recorder = if learning_output.is_some() {
        Some(Arc::new(LearningRecorder::new()))
    } else {
        None
    };

    // Create proxy server
    let proxy_config = ProxyServerConfig {
        socket_path: socket_path.clone(),
        network_config: Arc::new(config.network),
        policy_engine,
        learning_recorder: learning_recorder.clone(),
        learning_output: learning_output.cloned(),
        learning_mode: learning_mode.clone(),
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

    Ok((socket_path, learning_mode))
}
