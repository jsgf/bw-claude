# bw-claude: Bubblewrap Sandboxing for Claude

A Python script that wraps the Claude CLI with [bubblewrap](https://github.com/containers/bubblewrap) to provide a sandboxed execution environment with controlled filesystem access.

## Default Configuration (Safe Mode)

By default, `bw-claude` provides a secure sandbox with restricted access:

### Read-Only Access (Safe Directories Only)
- **Home subdirectories** (read-only):
  - `.config` - Configuration files
  - `.local/share`, `.local/bin` - Application data and user binaries
  - `Documents`, `Downloads`, `Projects` - User files
  - `.viminfo` - Vim history and settings
  - `.gitconfig` - Git configuration
  - **Development tools**:
    - `.cargo`, `.rustup` - Rust toolchain
    - `.npm` - npm cache/config
    - `.gem` - Ruby gems
    - `.gradle`, `.m2` - Java/Kotlin builds
    - `.nvm` - Node Version Manager
    - `.go` - Go workspace
- **System binaries and libraries**: `/usr`, `/lib`, `/lib64`, `/bin`
- **Essential /etc files**: `hostname`, `hosts`, `resolv.conf`, `passwd`, `group`

### Read-Write Access
- **Isolated /tmp**: Unique temporary directory per session (for state export)
- **~/.claude**: Read-write (Claude manages settings, state, telemetry, debug logs)
- **$PWD/.claude**: Read-write (project-specific Claude config)

### Network
- **Enabled by default** (required for Claude API calls)

### Explicitly Excluded (Even in Safe Mode)
- `~/.ssh` - SSH keys
- `~/.aws` - AWS credentials
- `~/.kube` - Kubernetes config
- `~/.gnupg` - GPG keys
- `~/.password-store` - Password manager
- Other home directories (Documents is allowed, but not arbitrary paths)

## Security Options

### `--no-network`
Disables network access completely. Use when you don't need API calls or want maximum isolation:

```bash
./bw-claude --no-network code myfile.py
```

### `--full-home-access`
Allows full home directory access (read-only). Use only if Claude needs access to files outside the safe directories. This is less secure:

```bash
./bw-claude --full-home-access code myfile.py
```

Safe directories (default mode) include configuration, development tools, and user files:
- `.config`, `.local/share`, `.local/bin`
- `Documents`, `Downloads`, `Projects`
- `.viminfo` (Vim history)
- `.cargo`, `.rustup` (Rust)
- `.npm`, `.gem`, `.gradle`, `.m2`, `.nvm`, `.go` (other languages)

### `--verbose` / `-v`
Prints sandbox configuration and the complete bwrap command to stderr for debugging:

```bash
./bw-claude -v code myfile.py
# Output:
# [bw-claude] Export /tmp: /tmp/bw-claude-abc12345
# [bw-claude] Network: enabled
# [bw-claude] Home access: safe (restricted)
# [bw-claude] Command: bwrap --die-with-parent --unshare-pid ... ~/.claude/local/claude code myfile.py
```

### `--shell`
Launches an interactive shell in the sandbox instead of Claude. Useful for debugging the sandbox environment and testing what's accessible:

```bash
./bw-claude --shell
# Now you're in a shell inside the sandbox - test what's accessible
$ ls ~
$ cat /etc/hosts
$ curl https://api.example.com  # Only works without --no-network
$ exit
```

### `--allow-ro PATH` and `--allow-rw PATH`
Mount additional paths into the sandbox (read-only or read-write). Can be specified multiple times. These override all other constraints:

```bash
# Allow Claude to read system logs (read-only)
./bw-claude --allow-ro /var/log code analyze.py

# Allow Claude to write to a custom directory (read-write)
./bw-claude --allow-rw /tmp/claude-workspace code process.py

# Multiple additional paths
./bw-claude --allow-ro /var/log --allow-rw /tmp/work --allow-ro /opt/config code main.py

# With explicit Claude argument separator
./bw-claude --allow-ro /var/log -- code myfile.py
```

### `--no-skip-permissions`
By default, `--allow-dangerously-skip-permissions` is passed to Claude to allow it to operate without permission errors in the sandbox. Use this flag to disable that behavior:

```bash
# Run without --allow-dangerously-skip-permissions
./bw-claude --no-skip-permissions code myfile.py
```

**Notes:**
- Paths must exist on the host system
- Read-only mounts are added with `--ro-bind`
- Read-write mounts are added with `--bind`
- These are processed after all other mounts

### Combined Usage

```bash
# Default (safe): restricted home + network enabled
./bw-claude code myfile.py

# Less secure: full home access + network
./bw-claude --full-home-access code myfile.py

# Maximum isolation: safe home + no network
./bw-claude --no-network code myfile.py

# Debug sandbox with verbose output
./bw-claude --shell --verbose

# Allow additional paths and show what's being executed
./bw-claude --allow-ro /var/log --verbose -- code analyze.py

# Complex example: restricted home + extra paths + verbose
./bw-claude --no-network --allow-ro /opt/data --allow-rw /tmp/work -v -- code process.py
```

## Requirements

- Python 3.6 or later
- Bubblewrap 0.4.1 or later (`bwrap` command-line tool)
- Claude CLI installed at `~/.claude/local/claude`

### Installation

```bash
# Install bubblewrap (Fedora/RHEL)
sudo dnf install bubblewrap

# Install bubblewrap (Debian/Ubuntu)
sudo apt install bubblewrap

# Install bubblewrap (Arch)
sudo pacman -S bubblewrap
```

## Usage

The `bw-claude` script is a drop-in replacement for the Claude CLI:

```bash
# Run Claude with sandboxing
./bw-claude [security-options] [claude-args]

# Examples
./bw-claude --version
./bw-claude code myfile.py
./bw-claude --no-network chat
./bw-claude --no-home-access --no-network code myfile.py
```

### Setup

1. Clone or download this repository
2. Ensure `bw-claude` is executable: `chmod +x bw-claude`
3. Add to PATH or run directly: `./bw-claude`

### Project-Specific Claude Configuration

The script automatically creates a `.claude` directory in the current working directory. This is the **only writable location** within the sandbox for global Claude state.

Additionally, a unique isolated `/tmp` directory is created per session:
- **Location**: `/tmp/bw-claude-{session-id}/`
- **Purpose**: Export state, temporary files, logs
- **Lifetime**: Until you clean it up (not automatically deleted)
- **Access**: Can be inspected from the host filesystem

This separation allows:
- Project-specific Claude settings in `$PWD/.claude`
- Isolated temporary state per session
- Easy file transfer in/out of the sandbox
- Inspection of what Claude created during execution

### Accessing Export Files

After running Claude, check the session's `/tmp` directory:

```bash
# Find the latest session
ls -lt /tmp/bw-claude-* | head -1

# Or capture the directory name with --verbose
./bw-claude --verbose code myfile.py 2>&1 | grep "Export /tmp"

# Access exported files
ls /tmp/bw-claude-abc12345/
cat /tmp/bw-claude-abc12345/exported_data.json
```

## How It Works

The script:

1. **Validates** that Claude CLI is installed at `~/.claude/local/claude`
2. **Creates** directories:
   - `$PWD/.claude` for project-specific state
   - `/tmp/bw-claude-{session-id}/` for isolated temporary files
3. **Parses** security options (`--no-network`, `--no-home-access`)
4. **Builds** a bubblewrap sandbox with:
   - Selective or full home directory (read-only)
   - System binaries and libraries (read-only)
   - Minimal /etc files (read-only)
   - Isolated /tmp per session (read-write)
   - Global ~/.claude (read-only)
   - Project-specific $PWD/.claude (read-write)
   - Process and device access
5. **Executes** Claude within the sandbox
6. **Forwards** exit code and output back to caller

### Mount Configuration (Safe Mode Default)

```
Read-Only (Safe Directories):
  $HOME/.config        -> $HOME/.config
  $HOME/.local/share   -> $HOME/.local/share
  $HOME/.local/bin     -> $HOME/.local/bin
  $HOME/Documents      -> $HOME/Documents
  $HOME/Downloads      -> $HOME/Downloads
  $HOME/Projects       -> $HOME/Projects
  $HOME/.viminfo       -> $HOME/.viminfo
  $HOME/.gitconfig     -> $HOME/.gitconfig
  $HOME/.cargo         -> $HOME/.cargo
  $HOME/.rustup        -> $HOME/.rustup
  $HOME/.npm           -> $HOME/.npm
  $HOME/.gem           -> $HOME/.gem
  $HOME/.gradle        -> $HOME/.gradle
  $HOME/.m2            -> $HOME/.m2
  $HOME/.nvm           -> $HOME/.nvm
  $HOME/.go            -> $HOME/.go
  /usr                 -> /usr
  /lib, /lib64         -> /lib, /lib64
  /etc/hostname        -> /etc/hostname
  /etc/hosts           -> /etc/hosts
  /etc/resolv.conf     -> /etc/resolv.conf
  /etc/passwd          -> /etc/passwd
  /etc/group           -> /etc/group
  /etc/pki             -> /etc/pki (CA certificates for Fedora/RHEL)
  /etc/ssl             -> /etc/ssl (CA certificates for Debian/Ubuntu and compatibility)
  /etc/crypto-policies -> /etc/crypto-policies (OpenSSL config for Fedora/RHEL)

Read-Write:
  /tmp/bw-claude-{id}  -> /tmp
  $HOME/.claude.json   -> $HOME/.claude.json (Claude state file, created if needed)
  $HOME/.claude        -> $HOME/.claude (if exists)
  $PWD/.claude         -> $PWD/.claude

Virtual Mounts:
  /proc                -> /proc (process info)
  /dev                 -> /dev  (device files)
  /bin                 -> /usr/bin (symlink)
```

### Mount Configuration (with `--full-home-access`)

Same as above, except full home directory instead of safe subdirs:

```
Read-Only (Full Home):
  $HOME                -> $HOME (entire directory)
  /usr, /lib*, /etc    -> (same as safe mode)

Read-Write:
  /tmp/bw-claude-{id}  -> /tmp
  $HOME/.claude.json   -> $HOME/.claude.json (Claude state file, created if needed)
  $HOME/.claude        -> $HOME/.claude (if exists)
  $PWD/.claude         -> $PWD/.claude
```

## Environment Variables

The sandbox sets the following essential environment variables:

- `HOME`: User's home directory
- `PWD`: Current working directory
- `USER`: Current username
- `TERM`: Terminal type (from parent)
- `PATH`: Full PATH from parent environment (preserves all entries)

Other environment variables from the parent (LANG, LC_ALL, DISPLAY, etc.) are not passed through due to compatibility with bubblewrap 0.11.0. If you need specific environment variables, you can:

1. Set them before running bw-claude (will be available in the sandbox)
2. Modify the script to add additional `--setenv` flags
3. Use `--shell` to test and debug

## Debugging the Sandbox

Use the `--shell` option to test and inspect the sandbox environment:

```bash
# Launch a shell in the sandbox with default settings
./bw-claude --shell

# Test what directories are accessible
$ ls ~
$ ls ~/.ssh
$ cat /etc/hosts

# Check network connectivity
$ ping -c 1 api.anthropic.com
$ curl https://api.anthropic.com/health
```

### Testing Security Options

Verify that restrictions are working as expected:

```bash
# Test default safe mode
./bw-claude --shell
$ ls ~/Documents              # Should work (safe dir)
$ ls ~/.ssh                   # Should fail (not mounted)
$ cat ~/.aws/config           # Should fail (not mounted)

# Test full home access (less secure)
./bw-claude --full-home-access --shell
$ cat ~/.aws/config           # Should now work
$ cat ~/.ssh/id_rsa           # Should now work (careful!)

# Test without network
./bw-claude --no-network --shell
$ ping 8.8.8.8               # Should fail
$ curl https://example.com   # Should fail

# Test with verbose output
./bw-claude --shell --verbose 2>&1 | grep "\[bw-claude\]"
```

### Checking Writable Directories

```bash
./bw-claude --shell
$ touch /tmp/test.txt                    # Should work (isolated /tmp)
$ touch ~/.claude/test                   # Should fail (RO)
$ touch .claude/test                     # Should work (RW in project dir)
$ ls -la /tmp/bw-claude-*/               # View session dir from inside
```

## Session Management

### Cleaning Up Sessions

Each session creates a directory in `/tmp`. You can safely delete old sessions:

```bash
# Remove old sessions (older than 7 days)
find /tmp/bw-claude-* -maxdepth 0 -mtime +7 -exec rm -rf {} \;

# Remove all sessions
rm -rf /tmp/bw-claude-*

# Or per-directory cleanup
rm -rf /tmp/bw-claude-abc12345/
```

### Finding Session Data

To find what Claude wrote during a session:

```bash
# Show all sessions and their size
du -sh /tmp/bw-claude-*

# Find the most recent session
ls -t /tmp/bw-claude-* | head -1

# Find large sessions
du -sh /tmp/bw-claude-* | sort -h | tail -5
```

## Troubleshooting

### bwrap not found
**Error**: `Error: bwrap not found. Please install bubblewrap.`

**Solution**: Install bubblewrap for your Linux distribution (see Requirements section).

### Claude CLI not found
**Error**: `Error: Claude CLI not found at ~/.claude/local/claude`

**Solution**: Ensure Claude is properly installed. The CLI should be available at `~/.claude/local/claude`.

### Permission denied
**Error**: `Error: Permission denied`

**Possible causes**:
- The bw-claude script is not executable
- Insufficient permissions for sandboxing (may require certain Linux capabilities)
- File permissions in `~/.claude` or `$PWD/.claude`

**Solution**:
```bash
chmod +x bw-claude
# Check home directory permissions
ls -la ~/.claude/
ls -la .claude/
```

### Network disabled but Claude needs API access
**Error**: Claude cannot connect to API

**Cause**: You ran with `--no-network` but Claude needs to make API calls

**Solution**: Remove the `--no-network` flag. The default safe mode keeps network enabled.

### File access errors
**Error**: "Permission denied" or "No such file or directory"

**Possible causes**:
- Accessing files outside safe directories (default mode)
- Trying to write outside of `$PWD/.claude` and `/tmp`
- Accessing restricted directories like `~/.ssh`, `~/.aws`, or `~/.gnupg`

**Solution**:
- Check which security options are enabled: `./bw-claude --verbose`
- Verify file locations are within safe directories or use `--full-home-access` if needed
- Use `--shell` to test what's accessible in the sandbox

### Debugging Mount Configuration

Use `--verbose` flag to see the sandbox setup:

```bash
./bw-claude --verbose -h 2>&1 | grep -E "\[bw-claude\]"
```

Output will show:
- Export /tmp directory location
- Network status (enabled/disabled)
- Home access mode (full/restricted)

## Technical Details

### Namespace Configuration
- **PID**: Isolated (Claude cannot see or interact with host processes)
- **IPC**: Isolated (prevents access to System V IPC, message queues, semaphores, shared memory)
- **Network**: Shared by default, can be disabled with `--no-network`
- **UTS**: Unique (hostname isolation)
- **User**: Host user mapping (not isolated)
- **Mount**: Unique with controlled mounts

### Directory Isolation
- **Read-only mounts** prevent accidental modifications to system/config
- **Project-specific state** in `$PWD/.claude` for portability
- **Session-isolated /tmp** prevents cross-session contamination
- **Explicit exclusions** (SSH, AWS, GPG) prevent credential leaks

### Security Layers
1. **Filesystem isolation**: Controlled mounts with ro/rw separation
2. **Network control**: Optional network isolation
3. **Directory whitelisting**: `--no-home-access` restricts to safe paths
4. **Minimal /etc**: Only essential config files mounted

## Contributing

For issues, improvements, or security concerns, please report them in the project repository.

## License

Provided as-is for sandboxing the Claude CLI.
