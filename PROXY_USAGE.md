# Proxy Filtering Usage Guide

This document explains how to use the `--use-filter-proxy` feature in `bw-claude` and `bw-gemini` for fine-grained network filtering.

## Overview

The proxy filtering system provides two-layer network isolation:

1. **Inside the sandbox**: `bw-relay` runs as a minimal localhost SOCKS5/HTTP proxy
2. **Outside the sandbox**: `bwrap-proxy` enforces network policies via a Unix domain socket

This architecture enables fine-grained control over:
- Which domains the tool can access
- Which IP addresses can be reached
- Learning mode to discover required network access
- Different filtering modes (open by default, restrictive)

## Basic Usage

### Enable filtered proxy mode

```bash
# Simple usage with no configuration (open mode - allow all by default)
bw-claude --use-filter-proxy

# With a custom configuration file
bw-claude --use-filter-proxy --proxy-config ~/.config/claude/proxy-policy.toml
```

### What happens when you enable proxy filtering

1. `bw-claude` spawns a `bwrap-proxy` daemon before starting the sandbox
2. The daemon listens on a Unix domain socket in `/tmp/`
3. Inside the sandbox, `bw-relay` starts and connects to this socket
4. `bw-relay` provides SOCKS5 on port 1080 and HTTP CONNECT on port 3128
5. The tool's environment variables are set to use these proxies:
   - `http_proxy=http://127.0.0.1:3128`
   - `https_proxy=http://127.0.0.1:3128`
   - `all_proxy=socks5://127.0.0.1:1080`
6. When the tool exits, `bw-claude` kills the proxy daemon

## Configuration Files

### File Format

Proxy configuration uses TOML format with the following sections:

#### Mode Section

```toml
[mode]
type = "open"                    # "open" (allow by default) or "restrictive" (deny by default)
learning_enabled = false         # Enable learning mode to discover network access
learning_file = ".learning.json" # Where to save discovered domains
```

#### Host Groups

Define sets of domains that can be referred to by policies:

```toml
[[host_groups]]
name = "apis"
description = "API servers"
domains = [
  "api.example.com",
  "*.example.com",              # Wildcard matching
]
```

Wildcards support `*` for any subdomain level.

#### IP Groups

Define IP ranges using CIDR notation:

```toml
[[ip_groups]]
name = "local"
description = "Local network"
ranges = [
  "127.0.0.1/8",
  "192.168.0.0/16",
]
```

#### Policies

Define what traffic is allowed or denied:

```toml
[[policies]]
name = "apis_allowed"
mode = "open"                        # Which mode this applies to
action = "allow"                     # "allow" or "deny"
host_groups = ["apis"]
ip_groups = ["local"]
```

### Example Configuration (Open Mode)

In "open" mode, everything is allowed by default. You specify what to deny:

```toml
[mode]
type = "open"

[[host_groups]]
name = "sensitive"
domains = ["credit-card-processor.com"]

[[policies]]
name = "block_sensitive"
mode = "open"
action = "deny"
host_groups = ["sensitive"]
```

### Example Configuration (Restrictive Mode)

In "restrictive" mode, everything is denied by default. You specify what to allow:

```toml
[mode]
type = "restrictive"

[[host_groups]]
name = "apis"
domains = [
  "api.openai.com",
  "*.openai.com",
]

[[policies]]
name = "allow_apis"
mode = "restrictive"
action = "allow"
host_groups = ["apis"]
```

## Learning Mode

Learning mode helps you discover what network access your tool actually needs:

```toml
[mode]
type = "restrictive"
learning_enabled = true
learning_file = ".claude-network-learning.json"
```

When enabled:
1. The proxy records all connection attempts
2. Both allowed and denied connections are logged
3. The log file shows what domains/IPs were accessed
4. You can use this to build your policy file

### Using Learning Output

```bash
# Run with learning enabled
bw-claude --use-filter-proxy --proxy-config learning-config.toml -- your-command

# Check what was accessed
cat .claude-network-learning.json | jq '.accessed_domains'

# Build your policy based on the results
```

## Advanced Examples

### Development Tool Access

```toml
[mode]
type = "open"

[[host_groups]]
name = "development"
domains = [
  "github.com",
  "*.github.com",
  "registry.npmjs.org",
  "crates.io",
  "pypi.org",
]

[[policies]]
name = "dev_allowed"
mode = "open"
action = "allow"
host_groups = ["development"]
```

### Strict Security Policy

```toml
[mode]
type = "restrictive"

[[host_groups]]
name = "required_apis"
domains = ["api.openai.com"]

[[ip_groups]]
name = "office_only"
ranges = ["203.0.113.0/24"]

[[policies]]
name = "office_access_only"
mode = "restrictive"
action = "allow"
ip_groups = ["office_only"]

[[policies]]
name = "apis_allowed"
mode = "restrictive"
action = "allow"
host_groups = ["required_apis"]
```

### Mixed Mode

```toml
[mode]
type = "open"  # Allow by default

[[host_groups]]
name = "blocked_domains"
domains = [
  "malicious-site.com",
  "data-exfiltration.com",
]

[[host_groups]]
name = "local_services"
domains = [
  "localhost",
  "*.local",
]

# Deny specific domains
[[policies]]
name = "block_malicious"
mode = "open"
action = "deny"
host_groups = ["blocked_domains"]

# Allow local access
[[policies]]
name = "allow_local"
mode = "open"
action = "allow"
host_groups = ["local_services"]
```

## Debugging

### Enable verbose logging

```bash
bw-claude --use-filter-proxy --verbose -- your-command
```

This will show:
- Proxy daemon startup messages
- Network filtering decisions
- Connection attempts

### Check proxy daemon status

The proxy daemon runs in the foreground but outputs to stderr. Monitor it with:

```bash
bw-claude --use-filter-proxy --verbose 2>&1 | grep -i proxy
```

### Network Troubleshooting

If network access fails:

1. Check the configuration file syntax with a TOML validator
2. Verify domain patterns with wildcards
3. Test with learning mode to see what's being blocked
4. Check system logs for bwrap-proxy errors

```bash
# Test with open mode (allow all)
bw-claude --use-filter-proxy -- your-command

# If that works, the issue is your policy file
# If that fails, there's a system issue
```

## Performance Considerations

- Proxy filtering adds minimal overhead (~1ms per connection)
- The Unix domain socket is efficient for local communication
- DNS lookups are cached by the policy engine
- IP range matching uses efficient CIDR lookup

## Security Notes

- Policies are evaluated in order - first match wins
- Wildcards (`*.example.com`) match one level by default
- Learning mode should be disabled in production
- The configuration file has the same access restrictions as the tool
- The Unix domain socket is only accessible by the tool and proxy daemon

## Troubleshooting

### "bwrap-proxy daemon failed to start"

Possible causes:
- `bwrap-proxy` binary not in PATH
- Permission issues creating the socket
- Port already in use (shouldn't happen with Unix sockets)

### "Connection refused" in the tool

Possible causes:
- Policy blocks the required domain
- Configuration file not found
- Learning mode found the domain but didn't log it

Enable learning mode to diagnose:

```bash
bw-claude --use-filter-proxy --proxy-config debug-learning.toml -- your-command
```

### Tool runs slowly with proxy

- Check if DNS resolution is slow (try IP groups instead of domains)
- Monitor with `--verbose` to see filtering overhead
- Profile with learning mode to find bottlenecks
