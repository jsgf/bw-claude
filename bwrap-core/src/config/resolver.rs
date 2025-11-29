//! Configuration resolution with support for composition and validation

use super::schema::{Config, FilesystemSpec, Policy};
use crate::error::{Result, SandboxError};
use std::collections::HashSet;

/// Resolve a filesystem config by name, handling extends/composition
pub fn resolve_filesystem_config(
    config: &Config,
    name: &str,
) -> Result<FilesystemSpec> {
    let mut visited = HashSet::new();
    resolve_filesystem_recursive(config, name, &mut visited)
}

fn resolve_filesystem_recursive(
    config: &Config,
    name: &str,
    visited: &mut HashSet<String>,
) -> Result<FilesystemSpec> {
    if visited.contains(name) {
        return Err(SandboxError::ConfigError(format!(
            "Circular reference in filesystem config: {}",
            name
        )));
    }
    visited.insert(name.to_string());

    let spec = config
        .filesystem
        .configs
        .get(name)
        .ok_or_else(|| {
            SandboxError::ConfigError(format!("Filesystem config not found: {}", name))
        })?
        .clone();

    // If no extends, return as-is
    if spec.extends.is_empty() {
        return Ok(spec);
    }

    // Resolve all extended configs and merge
    let mut merged = FilesystemSpec::default();

    for parent_name in &spec.extends {
        let parent = resolve_filesystem_recursive(config, parent_name, visited)?;
        merged = merge_filesystem_specs(merged, parent);
    }

    // Current spec overrides parents
    merged = merge_filesystem_specs(merged, spec);

    Ok(merged)
}

fn merge_filesystem_specs(
    base: FilesystemSpec,
    override_spec: FilesystemSpec,
) -> FilesystemSpec {
    // For filesystem configs, we extend arrays rather than replace them
    // This allows building up configurations by composing smaller pieces
    let mut ro_home_dirs = base.ro_home_dirs;
    ro_home_dirs.extend(override_spec.ro_home_dirs);

    let mut rw_home_dirs = base.rw_home_dirs;
    rw_home_dirs.extend(override_spec.rw_home_dirs);

    let mut ro_home_files = base.ro_home_files;
    ro_home_files.extend(override_spec.ro_home_files);

    let mut rw_home_files = base.rw_home_files;
    rw_home_files.extend(override_spec.rw_home_files);

    let mut essential_etc_files = base.essential_etc_files;
    essential_etc_files.extend(override_spec.essential_etc_files);

    let mut essential_etc_dirs = base.essential_etc_dirs;
    essential_etc_dirs.extend(override_spec.essential_etc_dirs);

    let mut system_paths = base.system_paths;
    system_paths.extend(override_spec.system_paths);

    let mut ro_paths = base.ro_paths;
    ro_paths.extend(override_spec.ro_paths);

    let mut rw_paths = base.rw_paths;
    rw_paths.extend(override_spec.rw_paths);

    FilesystemSpec {
        description: override_spec.description.or(base.description),
        ro_home_dirs,
        rw_home_dirs,
        ro_home_files,
        rw_home_files,
        essential_etc_files,
        essential_etc_dirs,
        system_paths,
        ro_paths,
        rw_paths,
        extends: vec![], // Resolved, so clear extends
    }
}

/// Resolve a policy by name
pub fn resolve_policy(config: &Config, name: &str) -> Result<Policy> {
    config
        .policy
        .policies
        .get(name)
        .cloned()
        .ok_or_else(|| SandboxError::ConfigError(format!("Policy not found: {}", name)))
}
