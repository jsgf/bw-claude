# SOCKS Proxy via Unix Domain Socket - Implementation Research

## Summary

A SOCKS5 proxy listening on a unix domain socket can provide network filtering for the sandbox **even with `--unshare-net`** network isolation, because unix domain sockets are filesystem objects that work across network namespaces.

## Key Technical Findings

### 1. Unix Sockets Work Across Network Namespaces

- Network namespaces isolate: network devices, IP stacks, routing tables, sockets
- **Exception**: Unix domain sockets (filesystem-based) are NOT isolated by network namespaces
- This allows communication between isolated sandbox and host proxy

### 2. SOCKS Proxy Implementations with Unix Socket Support

**Recommended: pproxy (Python)**
- PyPI package: `pproxy`
- Supports listening on unix domain sockets
- Built-in domain filtering with regex
- Pure Python, easy integration
- Command: `pproxy -l socks5://user:pass@/tmp/proxy.sock`

**Alternative: soxidizer (Rust)**
- Purpose-built for SOCKS5 over unix sockets
- Directory-based filtering
- Better performance but harder to integrate

### 3. Application Support

**Native unix socket SOCKS support:**
- curl (v7.84.0+): `curl --proxy socks5h://localhost/tmp/proxy.sock https://example.com`
- Firefox/FoxyProxy

**Applications needing bridge:**
- Most applications expect host:port format
- Solution: Use `socat` to bridge unix socket → TCP localhost
- Then set `ALL_PROXY=socks5://127.0.0.1:1080`

```bash
socat TCP-LISTEN:1080,bind=127.0.0.1,reuseaddr,fork UNIX-CONNECT:/tmp/proxy.sock &
export ALL_PROXY="socks5://127.0.0.1:1080"
```

## Proposed Architecture

```
Host (has network):
  └─ socks_filter.py (pproxy daemon)
      └─ Listens on /tmp/bw-claude-SESSION.sock
      └─ Whitelist: *.anthropic.com, *.claude.ai
      └─ Forwards allowed → real network
      └─ Blocks/logs denied connections

Unix Socket: /tmp/bw-claude-SESSION.sock
  └─ Bind mounted into sandbox

Sandbox (--unshare-net, no network):
  └─ Socket available at /proxy.sock
  └─ socat bridges /proxy.sock → 127.0.0.1:1080
  └─ ALL_PROXY=socks5://127.0.0.1:1080
  └─ Applications use proxy → controlled network access
```

## Implementation Plan

### 1. Create `socks_filter.py` - Filtering Proxy Daemon

```python
#!/usr/bin/env python3
"""
Filtering SOCKS5 proxy for bw-claude sandbox.
Listens on unix domain socket, filters by domain whitelist.
"""

import asyncio
import re
from pproxy import server

ALLOWED_DOMAINS = [
    r'.*\.anthropic\.com',
    r'.*\.claude\.ai',
    r'api\.anthropic\.com',
]

def is_allowed(hostname):
    """Check if hostname matches whitelist"""
    for pattern in ALLOWED_DOMAINS:
        if re.match(pattern, hostname):
            return True
    return False

# TODO: Implement filtering logic in pproxy
# Listen on unix socket
# Parse SOCKS5 CONNECT requests
# Check destination against whitelist
# Log denied attempts
```

### 2. Update `bw_lib.py`

**Add argument:**
```python
parser.add_argument(
    "--filter-network",
    action="store_true",
    help="Enable network filtering via SOCKS proxy (allows only whitelisted domains)",
)
```

**Add proxy management:**
```python
def start_proxy_daemon(socket_path):
    """Start filtering SOCKS proxy on unix socket"""
    # Start pproxy daemon process
    # Return process handle for cleanup
    pass

def build_bwrap_command(...):
    # If args.filter_network:
    #   1. Start proxy daemon
    #   2. Bind mount socket: --ro-bind /tmp/proxy.sock /proxy.sock
    #   3. Create socat bridge script in sandbox
    #   4. Set ALL_PROXY environment variable
    pass
```

### 3. Files to Create/Modify

**New files:**
- `socks_filter.py` - Proxy daemon with domain filtering
- `socat_bridge.sh` - Helper script to bridge unix socket → TCP localhost

**Modified files:**
- `bw_lib.py` - Add --filter-network flag, proxy management
- `bw-claude`, `bw-gemini` - Handle proxy process lifecycle

### 4. Usage

```bash
# Enable network filtering
./bw-claude --filter-network

# Combine with network isolation
./bw-claude --filter-network --unshare-net

# Without filtering (current behavior)
./bw-claude
```

## Security Benefits

1. **Network access control** without requiring root privileges
2. **Works with `--unshare-net`** - Provides controlled access even in isolated namespace
3. **Domain whitelisting** prevents exfiltration to unauthorized domains
4. **Audit logging** of all connection attempts
5. **Filesystem permissions** control proxy access (socket file permissions)

## Limitations

1. **Application support** - Limited native support, most need socat bridge
2. **Performance overhead** - Extra layer of proxying
3. **Complexity** - More moving parts (daemon, socket, bridge)
4. **Dependency** - Requires pproxy installation

## Testing Strategy

1. Test proxy listening: `ls -la /tmp/bw-proxy.sock`
2. Test from host: `curl --proxy socks5h://localhost/tmp/bw-proxy.sock https://api.anthropic.com`
3. Test in sandbox with `--shell`: Verify socket mounted and accessible
4. Test filtering: Try allowed and denied domains
5. Test with real Claude CLI
6. Test with `--unshare-net` to verify unix socket works across namespaces

## Alternative: Simple Approach

If pproxy is too complex, a simpler approach:

1. Use `--share-net` (network enabled) by default
2. Add `--no-network` for complete isolation
3. Document that fine-grained filtering requires external tools
4. Users can set up their own firewall rules if needed

This avoids the complexity while still providing two modes: full access or no access.

## References

- pproxy: https://pypi.org/project/pproxy/
- soxidizer: https://github.com/randomstuff/soxidizer
- curl SOCKS: https://everything.curl.dev/usingcurl/proxies/socks.html
- Network namespaces: https://man7.org/linux/man-pages/man7/network_namespaces.7.html
- Unix sockets across namespaces: https://lwn.net/Articles/580893/
