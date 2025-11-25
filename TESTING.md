# Testing Guide for bw-claude/bw-gemini with Proxy Filtering

This document outlines the testing strategy for the Rust implementation of bw-claude with integrated proxy filtering.

## Architecture Overview for Testing

The system has several components that need testing:

1. **bwrap-core**: Bubblewrap sandbox configuration and execution
2. **bwrap-proxy**: Network policy enforcement (library)
3. **bw-relay**: Network relay inside the sandbox
4. **bw-claude/bw-gemini**: CLI wrappers with proxy lifecycle management

## Unit Tests

### bwrap-core Tests

The `bwrap-core` crate includes unit tests for:

- **Startup Script Generation** (`bwrap-core/src/startup_script.rs`):
  ```bash
  cargo test -p bwrap-core startup_script
  ```
  Tests that the generated shell script:
  - Contains correct relay binary path
  - Contains correct socket path
  - Sets environment variables properly
  - Includes target binary and arguments

Run all bwrap-core tests:
```bash
cargo test -p bwrap-core
```

### bwrap-proxy Tests

The `bwrap-proxy` crate includes tests for:

- Policy engine filtering logic
- Learning mode recording
- Configuration parsing

```bash
cargo test -p bwrap-proxy
```

## Integration Tests

### Test 1: Basic Sandbox Execution (No Proxy)

```bash
# Build the binaries first
cargo build --release

# Test basic sandbox without proxy
./target/release/bw-claude --help

# Test with a simple command
cd /tmp
./target/release/bw-claude -- echo "Hello from sandbox"
```

Expected behavior:
- Shows Claude help or runs the command
- Sandbox properly isolates the environment
- Command completes successfully

### Test 2: Proxy Daemon Startup and Cleanup

```bash
# Test with proxy enabled (should fail gracefully if bwrap-proxy not in PATH)
./target/release/bw-claude --use-filter-proxy -- echo "test"
```

Expected behavior:
- If `bwrap-proxy` is in PATH: proxy starts, tool runs, proxy cleaned up
- If `bwrap-proxy` not in PATH: appropriate error message

### Test 3: Network Isolation Verification

```bash
# Test network disabled (no proxy)
./target/release/bw-claude --no-network -- curl https://api.openai.com

# Test network enabled (no proxy)
./target/release/bw-claude -- curl https://api.openai.com  # might succeed if curl available

# Test with filtering proxy
./target/release/bw-claude --use-filter-proxy -- curl https://api.openai.com
```

Expected behavior:
- `--no-network`: All network calls fail (can't resolve hosts)
- No proxy: Network calls depend on system configuration
- With proxy: Network calls go through proxy filtering

### Test 4: Filesystem Isolation

```bash
# Create a test project
mkdir -p /tmp/test-project
cd /tmp/test-project

# Test read-only access to project directory
./target/release/bw-claude -- sh -c "cat /proc/1/environ | grep PWD"

# Test read-write access to .claude directory
./target/release/bw-claude -- sh -c "echo test > .claude/test.txt && cat .claude/test.txt"

# Verify original file not modified
ls -la .claude/test.txt  # should exist in sandbox only

# Test safe home directory access
./target/release/bw-claude -- sh -c "ls ~/.local/bin"

# Test blocked home directory access
./target/release/bw-claude -- sh -c "ls ~/.ssh 2>&1 || echo 'Access denied (expected)'"
```

Expected behavior:
- Project directory is readable
- `.claude` directory is writable
- Safe home dirs are accessible
- SSH/AWS/Kube dirs are not accessible

### Test 5: Environment Variable Passthrough

```bash
# Set an environment variable
export TEST_VAR="test_value"

# Pass it through
./target/release/bw-claude --pass-env TEST_VAR -- sh -c 'echo $TEST_VAR'

# Without --pass-env, variable should not be present
./target/release/bw-claude -- sh -c 'echo $TEST_VAR'
```

Expected behavior:
- With `--pass-env`: Variable is accessible in sandbox
- Without `--pass-env`: Variable is not set (or has default value)

### Test 6: Proxy Configuration Loading

```bash
# Create a test configuration
cat > /tmp/test-proxy-config.toml << 'EOF'
[mode]
type = "open"

[[host_groups]]
name = "test"
domains = ["example.com"]

[[policies]]
name = "allow_test"
mode = "open"
action = "allow"
host_groups = ["test"]
EOF

# Test with config (will fail if bwrap-proxy not available)
./target/release/bw-claude --use-filter-proxy --proxy-config /tmp/test-proxy-config.toml -- echo "Config loaded"
```

Expected behavior:
- Config file is parsed and passed to proxy daemon
- Tool executes with policy applied

## Manual Testing Checklist

- [ ] `bw-claude --help` shows all proxy options
- [ ] `bw-gemini --help` shows all proxy options
- [ ] Basic sandbox execution works without proxy
- [ ] Proxy startup/cleanup works (with bwrap-proxy installed)
- [ ] Network isolation works with `--no-network`
- [ ] Filesystem isolation allows reading project files
- [ ] Filesystem isolation prevents writing to project
- [ ] Filesystem isolation allows `.claude` directory writes
- [ ] Home directory safe lists work correctly
- [ ] Environment variable passthrough works
- [ ] Configuration file is accepted and validated
- [ ] Verbose logging shows expected messages
- [ ] Multiple instances can run simultaneously
- [ ] Proxy daemon cleanup happens on tool exit
- [ ] Cleanup happens even when tool crashes (trap handlers)

## Continuous Integration

### Build Testing

```bash
# Check compilation for all crates
cargo check --all

# Run all tests
cargo test --all

# Run clippy for code quality
cargo clippy --all -- -D warnings

# Check code formatting
cargo fmt --all -- --check
```

### Release Build

```bash
# Build optimized release binary
cargo build --release

# Verify binary exists and is executable
ls -lh target/release/bw-claude
./target/release/bw-claude --version
```

## Performance Testing

### Baseline Performance (No Proxy)

```bash
# Measure sandbox startup time
time ./target/release/bw-claude -- true

# Measure memory usage
/usr/bin/time -v ./target/release/bw-claude -- true
```

### Proxy Overhead

```bash
# With proxy enabled (bwrap-proxy not actually running)
time ./target/release/bw-claude --use-filter-proxy -- true 2>/dev/null
```

Expected overhead:
- Socket creation: ~5-10ms
- Proxy daemon startup: ~50-100ms
- Total: <200ms additional latency

## Stress Testing

### Concurrent Execution

```bash
# Run multiple instances in parallel
for i in {1..5}; do
  ./target/release/bw-claude -- sleep 1 &
done
wait
```

Expected behavior:
- All instances complete successfully
- No socket conflicts
- No leftover proxy processes

### Long-Running Tests

```bash
# Run tool for extended period
timeout 60 ./target/release/bw-claude -- sh -c 'while true; do sleep 1; done'
```

Expected behavior:
- Proxy cleanup still works
- No resource leaks

## Debugging Failed Tests

### Enable Debug Logging

```bash
RUST_LOG=debug ./target/release/bw-claude --use-filter-proxy --verbose -- your-command
```

### Check Proxy Daemon Logs

```bash
# Run with foreground proxy (requires custom setup)
bwrap-proxy --socket /tmp/test.sock --mode open --verbose
```

### Monitor Network Activity

```bash
# In another terminal, monitor socket activity
strace -e openat,connect ./target/release/bw-claude --use-filter-proxy -- your-command
```

### Inspect Generated Commands

```bash
# Use --verbose to see bwrap command
./target/release/bw-claude --verbose -- echo test 2>&1 | grep -i "command\|mount"
```

## Known Limitations and Workarounds

1. **bwrap-proxy not in PATH**
   - Workaround: Place binary in same directory as bw-claude, or install to system PATH
   - Limitation: Proxy features not available in this case

2. **Insufficient Permissions**
   - Workaround: Run with `sudo` or add user to appropriate groups
   - Note: Sandbox itself may require elevated permissions

3. **SELinux Restrictions**
   - Workaround: Disable SELinux or add appropriate policy
   - Note: Specific policy rules may be needed

## Future Testing

Once bw-relay and bwrap-proxy are fully implemented:

- [ ] SOCKS5 protocol compliance testing
- [ ] HTTP CONNECT protocol testing
- [ ] Policy decision correctness testing
- [ ] Learning mode accuracy testing
- [ ] Configuration validation testing
- [ ] Policy conflict resolution testing

See `TODO.md` for Phase 2 implementation tasks.
