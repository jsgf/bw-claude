# Future Enhancements

## Planned Features

### Overlayfs Snapshotting
- Implement copy-on-write filesystem for writable directories
- Allow capturing, inspecting, and selectively applying changes
- Useful for testing with rollback capability
- Requires overlayfs kernel feature and elevated privileges

### Additional Isolation Options
- Optional PID namespace isolation (for process isolation)
- Optional IPC namespace isolation
- Optional UTS namespace isolation (hostname/domainname)
- Optional user namespace mapping

### Configuration File Support
- Read bwrap options from config file (`.bw-claude.yml` or similar)
- Per-project sandbox customization
- Preset profiles (e.g., "strict", "permissive", "network-isolated")

### Mounting Options
- CLI flags to control which directories are writable
- Option to make directories read-only
- Option to completely hide directories
- Environment variable overrides

### Security Enhancements
- Seccomp filter profiles (restrict system calls)
- Capability limiting
- Resource limits (memory, CPU)
- Filesystem quotas

### Logging and Auditing
- Log filesystem access (read/write operations)
- Track which files were modified
- Generate diffs of changed files
- Audit trail of Claude invocations

### Testing
- Integration tests for sandbox isolation
- Security regression tests
- Performance benchmarks
- Compatibility testing across Linux distributions

## Completed
- [x] Basic bwrap wrapper script
- [x] README documentation
- [x] .gitignore
