//! Network mode determination and proxy setup

use crate::args::CommonArgs;
use crate::proxy::create_proxy_task;
use crate::config::{Config, NetworkMode};
use std::path::PathBuf;
use anyhow::Result;

/// Determine the network mode based on CLI flags
/// --policy, --learn, or --learn-deny enables HTTP CONNECT proxy with filtering
/// Returns (network_mode, proxy_socket, policy_name)
pub async fn determine_network_mode(
    common: &CommonArgs,
    config: &Config,
) -> Result<(NetworkMode, Option<PathBuf>, String)> {
    // Check if lockdown policy is specified (pure network isolation, no proxy)
    if let Some(ref policy) = common.policy {
        if policy == "lockdown" {
            return Ok((NetworkMode::Disabled, None, "lockdown".to_string()));
        }
    }

    // Check if proxy should be enabled
    let use_proxy = common.policy.is_some()
        || common.learn.is_some()
        || common.learn_deny.is_some();

    if use_proxy {
        // Determine policy name and learning mode:
        // - If --learn is specified: use "open" policy (allow all, record access)
        // - If --learn-deny is specified: use specified policy or default to "deny" (enforce policy, record denials)
        // - If --policy is specified: use that policy (no learning)
        let (policy_name, learning_mode, learning_output) = if let Some(_) = common.learn {
            ("open", Some("learn"), common.learn.as_ref())
        } else if let Some(_) = common.learn_deny {
            (
                common.policy.as_ref().map(|s| s.as_str()).unwrap_or("deny"),
                Some("learn_deny"),
                common.learn_deny.as_ref(),
            )
        } else {
            (
                common.policy.as_ref().map(|s| s.as_str()).unwrap_or("default"),
                None,
                None,
            )
        };

        // Create proxy task with policy and learning configuration
        let (socket_path, _) = create_proxy_task(
            config,
            Some(policy_name),
            learning_output,
            learning_mode.map(|s| s.to_string()),
        )
        .await?;

        let network_mode = NetworkMode::Filtered {
            proxy_socket: socket_path.clone(),
            policy_name: policy_name.to_string(),
            learning_output: learning_output.cloned(),
            learning_mode: learning_mode.map(|s| s.to_string()),
            allowed_domains: vec![], // Deprecated field, kept for compatibility
        };

        Ok((network_mode, Some(socket_path), policy_name.to_string()))
    } else if common.no_network {
        Ok((NetworkMode::Disabled, None, "lockdown".to_string()))
    } else {
        Ok((NetworkMode::Enabled, None, "default".to_string()))
    }
}
