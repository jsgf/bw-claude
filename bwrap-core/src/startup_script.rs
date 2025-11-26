//! Utilities for relay configuration in filtered proxy mode

use std::path::Path;

/// Build the command arguments for bw-relay
///
/// bw-relay is executed directly (no shell script) with:
/// - --socket <proxy_socket>
/// - -- <target_binary> [target_args...]
///
/// The relay handles starting proxy servers, setting environment variables,
/// and exec'ing the target binary.
pub fn build_relay_command(
    proxy_socket: &Path,
    relay_binary: &Path,
    target_binary: &Path,
    target_args: &[String],
) -> Vec<String> {
    let mut cmd = vec![];

    // The relay binary itself
    cmd.push(relay_binary.display().to_string());

    // Socket path argument
    cmd.push("--socket".to_string());
    cmd.push(proxy_socket.display().to_string());

    // Separator before target command
    cmd.push("--".to_string());

    // Target binary and its arguments
    cmd.push(target_binary.display().to_string());
    cmd.extend(target_args.iter().cloned());

    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_relay_command() {
        use std::path::PathBuf;

        let cmd = build_relay_command(
            &PathBuf::from("/proxy.sock"),
            &PathBuf::from("/bw-relay"),
            &PathBuf::from("/usr/bin/claude"),
            &["arg1".to_string(), "arg2".to_string()],
        );

        assert_eq!(cmd[0], "/bw-relay");
        assert_eq!(cmd[1], "--socket");
        assert_eq!(cmd[2], "/proxy.sock");
        assert_eq!(cmd[3], "--");
        assert_eq!(cmd[4], "/usr/bin/claude");
        assert_eq!(cmd[5], "arg1");
        assert_eq!(cmd[6], "arg2");
    }
}
