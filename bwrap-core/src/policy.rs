//! Policy resolution and setup

use crate::args::CommonArgs;
use crate::config::{Config, FilesystemSpec, NetworkMode, Policy};
use crate::proxy::create_proxy_task;
use anyhow::Result;

/// Policy setup result containing all needed configuration
pub struct PolicySetup {
    /// The resolved policy
    pub policy: Policy,
    /// The policy name that was used
    pub policy_name: String,
    /// Filesystem spec resolved from the policy
    pub filesystem_spec: FilesystemSpec,
    /// Network mode configured from the policy
    pub network_mode: NetworkMode,
}

/// Set up policy configuration for a tool
///
/// This determines which policy to use, loads it from config, resolves the filesystem spec,
/// and sets up the network mode with proxy if needed.
///
/// # Arguments
/// * `config` - The loaded application configuration
/// * `common` - Common CLI arguments
/// * `default_policy` - Tool-specific default policy name (e.g., "claude", "gemini")
pub async fn setup_policy(
    config: &Config,
    common: &CommonArgs,
    default_policy: &str,
) -> Result<PolicySetup> {
    // Determine the policy to use
    let policy_name_str = common.policy.as_deref().unwrap_or(default_policy);

    // Load the policy to get its network configuration
    let policy = crate::resolve_policy(config, policy_name_str)
        .unwrap_or_else(|_| {
            tracing::warn!("Policy '{}' not found, using default", policy_name_str);
            Policy::default()
        });

    // Resolve filesystem spec based on the policy
    let filesystem_spec = if let Some(fs_config_name) = &policy.filesystem {
        crate::resolve_filesystem_config(config, fs_config_name)
            .unwrap_or_else(|_| FilesystemSpec::default())
    } else {
        FilesystemSpec::default()
    };

    // Determine network mode based on CLI flags and policy network settings
    let network_mode = if common.no_network {
        NetworkMode::Disabled
    } else if common.policy.is_some() || common.learn.is_some() || common.learn_deny.is_some() {
        // Explicit network mode specified via CLI flags, use determine_network_mode
        let (mode, _, _) = crate::determine_network_mode(common, config).await?;
        mode
    } else {
        // Use the network policy from the config
        match policy.network.network {
            bwrap_proxy::config::NetworkMode::Open => NetworkMode::Enabled,
            bwrap_proxy::config::NetworkMode::Disabled => NetworkMode::Disabled,
            bwrap_proxy::config::NetworkMode::Proxy => {
                // Policy requires proxy - create it with the policy's settings
                let (socket_path, _) = create_proxy_task(
                    config,
                    Some(policy_name_str),
                    None,
                    None,
                )
                .await
                .map_err(|e| {
                    tracing::warn!("Failed to create proxy for policy '{}': {}", policy_name_str, e);
                    e
                })?;

                NetworkMode::Filtered {
                    proxy_socket: socket_path,
                    policy_name: policy_name_str.to_string(),
                    learning_output: None,
                    learning_mode: None,
                    allowed_domains: vec![],
                }
            }
        }
    };

    Ok(PolicySetup {
        policy,
        policy_name: policy_name_str.to_string(),
        filesystem_spec,
        network_mode,
    })
}
