//! Proxy server initialization and management

use anyhow::{Context, Result};
use bwrap_proxy::{PolicyEngine, ProxyServer, ProxyServerConfig};
use crate::config::{Config, LearningRecorder, resolve_policy};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

/// Create and spawn the proxy server as an async task
///
/// This function:
/// 1. Generates a unique socket path in /tmp
/// 2. Creates a PolicyEngine for the specified policy (using already-loaded config)
/// 3. Creates a LearningRecorder if learning_output is specified
/// 4. Spawns the proxy server as a tokio task (runs until parent exits)
/// 5. Waits for the proxy to be ready (listening on the socket)
/// 6. Returns the socket path and learning mode (if active)
///
/// Note: The proxy will save learning data on shutdown via a cleanup function
pub async fn create_proxy_task(
    config: &Config,
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

    // Create PolicyEngine if a policy name is specified
    let policy_engine = if let Some(policy_name) = policy_name {
        let resolved_policy = resolve_policy(&config, policy_name)
            .context(format!("Failed to load policy: {}", policy_name))?;

        // Only create PolicyEngine if the policy requires filtering (Proxy mode with deny rules)
        if matches!(resolved_policy.network.network, bwrap_proxy::config::NetworkMode::Proxy) {
            Some(Arc::new(
                PolicyEngine::from_network_policy(
                    resolved_policy.network.effective_allow_groups(),
                    resolved_policy.network.deny_groups.clone(),
                    resolved_policy.network.default.clone(),
                    &config.network,
                )
                .context(format!("Failed to initialize policy engine for: {}", policy_name))?,
            ))
        } else {
            // For Open or Disabled network modes, no filtering engine needed
            None
        }
    } else {
        None
    };

    // Create LearningRecorder if learning output path is specified
    // Load existing config file if it exists and set the output path
    let learning_recorder = if let Some(output_path) = learning_output {
        let session_name = format!("learned_session_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("Failed to get system time")?
            .as_secs());

        match LearningRecorder::with_output_path(&session_name, output_path.clone()) {
            Ok(recorder) => Some(Arc::new(recorder)),
            Err(e) => {
                tracing::warn!("Failed to initialize learning recorder with existing file: {e}");
                // Fall back to a new recorder but still set the output path
                let recorder = LearningRecorder::new();
                if let Err(e) = recorder.set_output_path(output_path.clone()) {
                    tracing::warn!("Failed to set output path for learning recorder: {e}");
                }
                Some(Arc::new(recorder))
            }
        }
    } else {
        None
    };

    // Create proxy server
    let learning_recorder_trait: Option<Arc<dyn bwrap_proxy::filter::LearningRecorderTrait>> =
        learning_recorder.as_ref().map(|lr| lr.clone() as Arc<dyn bwrap_proxy::filter::LearningRecorderTrait>);

    let proxy_config = ProxyServerConfig {
        socket_path: socket_path.clone(),
        network_config: Arc::new(config.network.clone()),
        policy_engine,
        learning_recorder: learning_recorder_trait,
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
