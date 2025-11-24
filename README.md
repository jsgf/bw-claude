# bw-claude: Bubblewrap Sandboxing for Claude

A Python script that wraps the Claude CLI with [bubblewrap](https://github.com/containers/bubblewrap) to provide a sandboxed execution environment with controlled filesystem access.

## Default Configuration (Safe Mode)

By default, `bw-claude` provides a secure sandbox with restricted access:

### Read-Only Access (Safe Directories Only)
- **Home subdirectories** (read-only):
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

- **`.config` subdirectories** (read-only, selectively mounted for security):
  - **Safe tools**: `git`, `nvim`, `vim`, `htop`, `nano`, `less`, `lsd`, `bat`, `zsh`, `bash`, `fish`, `alacritty`, `kitty`
  - **NOT mounted**: Browser configs (protects Chrome/Brave/Edge cookies and credentials)
  - To access additional `.config` subdirectories, use: `./bw-claude --allow-ro ~/.config/myapp code file.py`
- **System binaries and libraries**: `/usr`, `/lib`, `/lib64`, `/bin`
- **Essential /etc files**: `hostname`, `hosts`, `resolv.conf`, `passwd`, `group`

### Read-Write Access
- **Current directory ($PWD)**: **Read-only** (Claude can read all project files)
  - `$PWD/.claude`: Read-write overlay (Claude can write project-specific state/config)
- **Isolated /tmp**: Unique temporary directory per session (for state export)
- **~/.claude**: Read-write (Claude manages settings, state, telemetry, debug logs)

This design allows Claude to:
- Read all project files and dependencies
- Write only to `.claude` directory for project-specific state
- Prevent accidental file modifications in the project

### Network
- **Enabled by default** (required for Claude API calls)

### Explicitly Excluded (Even in Safe Mode)

**Home directories (for security):**
- `~/.ssh` - SSH keys
- `~/.aws` - AWS credentials
- `~/.kube` - Kubernetes config
- `~/.gnupg` - GPG keys
- `~/.password-store` - Password manager

**Dangerous suid/privileged binaries (unavailable in sandbox):**
- `su`, `sudo`, `sudoedit` - User switching and privilege escalation
- `chsh`, `chfn`, `passwd`, `chpasswd` - Account management
- `mount`, `umount`, `fusermount` - Filesystem operations
- `useradd`, `usermod`, `groupadd`, `groupmod`, `userdel`, `groupdel` - User/group management
- `chown`, `chgrp`, `setcap`, `setfacl` - Permission management
- `ip` - Network configuration

These are unavailable because:
- They require elevated privileges
- Filesystem is mostly read-only
- They could be used to escape or bypass sandbox restrictions

## Security Notes

### Browser Cookies and Credentials
**Firefox users**: Your browser data is completely protected. Firefox stores cookies/history in `~/.mozilla/` which is NOT mounted.

**Chrome/Chromium/Brave/Edge users**: These browsers store authentication cookies in `~/.config/google-chrome/`, `~/.config/BraveSoftware/`, etc.
- By default, bw-claude **does NOT** mount these directories in safe mode
- If you use `--full-home-access`, your browser cookies WILL be accessible to Claude
- To safely use specific `.config` tools, use: `./bw-claude --allow-ro ~/.config/git code file.py`

### Why This Matters
Browser cookies may contain:
- GitHub authentication tokens
- AWS/Google Cloud credentials
- Banking and payment session tokens
- Email and personal account credentials

A compromised or untrusted Claude instance could read these and impersonate you on authenticated websites.

### Filesystem Write Protection
- **`/etc` is mounted read-only** to prevent forging system files (`/etc/shadow`, `/etc/sudoers`, etc.)
- Only essential subdirectories like `/etc/pki` and `/etc/ssl` are accessible
- All system binaries and libraries (`/usr`, `/lib`) are mounted read-only
- Claude cannot write to system directories or create forged configuration files

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
By default, `--dangerously-skip-permissions` is passed to Claude to skip all permission prompts. This allows Claude to execute commands and access files without asking for confirmation on each operation. Use this flag to disable that behavior:

```bash
# Run without --dangerously-skip-permissions (Claude will prompt for permissions)
./bw-claude --no-skip-permissions code myfile.py
```

**Why enabled by default:**
- In a sandbox, permission prompts become tedious
- The sandbox filesystem restrictions provide safety
- Claude still respects read-only mounts and other sandbox limits
- Only disable this if you want interactive permission prompts

### `--snapshot`
Create a lightweight reflink snapshot of your project directory for safe experimentation. Only works on filesystems that support reflinks (btrfs, xfs, ocfs2). Changes are isolated in the snapshot—you can review and decide whether to apply them:

```bash
# Run in snapshot mode (btrfs/xfs only)
./bw-claude --snapshot code myproject.py

# After the session, changes are in:
# ./myproject.py.bw-claude-snapshot-<id>/

# Review changes
diff -r ./myproject.py ./myproject.py.bw-claude-snapshot-abc12345/

# Apply changes if satisfied
cp -r ./myproject.py.bw-claude-snapshot-abc12345/* ./myproject.py/

# Or discard by removing the snapshot directory
rm -rf ./myproject.py.bw-claude-snapshot-abc12345/
```

**Benefits:**
- Claude has full write access without risk
- Easy rollback—just delete the snapshot
- Compare changes before committing
- Great for experimental refactoring

**Requirements:**
- Filesystem must support reflinks (btrfs, xfs)
- For btrfs: `btrfs-progs` tools installed
- Check filesystem: `df -T <path>`

**Notes:**
- Paths must exist on the host system
- Read-only mounts are added with `--ro-bind`
- Read-write mounts are added with `--bind`
- These are processed after all other mounts

### `--dir PATH`
Set the working directory for Claude in the sandbox. By default, Claude runs in the current directory. Use this to sandbox a different directory without changing your current shell directory:

```bash
# Run Claude on a different project without cd-ing
./bw-claude --dir ~/projects/myrepo code main.py

# Audit a directory while staying in your current location
./bw-claude --dir /path/to/audit --shell

# Combine with other options
./bw-claude --dir ~/work/project --no-network --verbose code script.py

# Works with snapshots too
./bw-claude --dir /data/project --snapshot code refactor.py
```

**Use cases:**
- Audit code in a specific directory without changing pwd
- Run Claude on multiple projects in sequence
- Sandbox a directory that's not in your current path
- Combine with `--snapshot` to safely experiment on a specific project

**Behavior:**
- If `--dir` is not specified, Claude runs in your current directory
- The specified directory is mounted read-only in the sandbox
- `$PWD/.claude` in the sandbox will be the project state directory
- Directory must exist on the host system

### Combined Usage

```bash
# Default (safe): restricted home + network enabled
./bw-claude code myfile.py

# Less secure: full home access + network
./bw-claude --full-home-access code myfile.py

# Maximum isolation: safe home + no network
./bw-claude --no-network code myfile.py

# Run Claude on a different directory
./bw-claude --dir ~/projects/other code main.py

# Audit a directory with verbose output
./bw-claude --dir /path/to/audit --verbose -- code review.py

# Safe snapshot experimentation on a specific project
./bw-claude --dir ~/experimental-project --snapshot code refactor.py

# Debug sandbox with verbose output
./bw-claude --shell --verbose

# Allow additional paths and show what's being executed
./bw-claude --allow-ro /var/log --verbose -- code analyze.py

# Complex example: different dir + extra paths + verbose
./bw-claude --dir ~/work/project --no-network --allow-ro /opt/data -v -- code process.py
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
# Run Claude with sandboxing (in current directory)
./bw-claude [security-options] [claude-args]

# Run Claude on a different directory
./bw-claude --dir /path/to/project [security-options] [claude-args]

# Examples
./bw-claude --version
./bw-claude code myfile.py
./bw-claude --no-network chat
./bw-claude --dir ~/other-project code myfile.py
./bw-claude --dir /audit-path --shell
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
  $PWD                 -> $PWD (entire project directory)
  Safe .config subdirs  -> $HOME/.config/* (git, nvim, vim, htop, etc. - excludes browsers)
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
  /etc (entire)        -> /etc (read-only to prevent forged files)
  /etc/pki             -> /etc/pki (overridable, CA certificates for Fedora/RHEL)
  /etc/ssl             -> /etc/ssl (overridable, CA certificates for Debian/Ubuntu)
  /etc/crypto-policies -> /etc/crypto-policies (overridable, OpenSSL config for Fedora/RHEL)

Read-Write:
  /tmp/bw-claude-{id}  -> /tmp
  $HOME/.claude.json   -> $HOME/.claude.json (Claude state file, created if needed)
  $HOME/.claude        -> $HOME/.claude (if exists)
  $PWD/.claude         -> $PWD/.claude (project-specific state, overlay on RO $PWD)

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
