# Future Enhancements

## Planned Features

### Generalize for other llms
- First target: gemini
- Refactor bw-claude so that as much code as possible is shared
- End up with bw-claude and bw-gemini
- As much feature parity as possible

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

## MCP
- Extend for MCP? Agents? Other things?

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

## Security
- /etc/shadow still visible
- ~/.claude/local/node_modules should be RO
  - also disable auto-update
- drop many caps: see `man 7 capabilities`
- from `man 8 mount`:
  > It’s also possible to change nosuid, nodev, noexec, noatime, nodiratime,
  > relatime and nosymfollow VFS entry flags via a "remount,bind" operation. The
  > other flags (for example filesystem-specific flags) are silently ignored.
  > The classic mount(2) system call does not allow to change mount options
  > recursively (for example with -o rbind,ro). The recursive semantic is
  > possible with a new mount_setattr(2) kernel system call and it’s supported
  > since libmount from util-linux v2.39 by a new experimental "recursive"
  > option argument (e.g. -o rbind,ro=recursive). For more details see the
  > FILESYSTEM-INDEPENDENT MOUNT OPTIONS section.
- Use --bind-try to handle non-existent src dirs?

## Style
- consistent about using pathlib?
- types

# Reference

## Claude

Full claude options
```
$ claude --help
Usage: claude [options] [command] [prompt]

Claude Code - starts an interactive session by default, use -p/--print for non-interactive output

Arguments:
  prompt                                            Your prompt

Options:
  -d, --debug [filter]                              Enable debug mode with optional category filtering (e.g., "api,hooks" or "!statsig,!file")
  --verbose                                         Override verbose mode setting from config
  -p, --print                                       Print response and exit (useful for pipes). Note: The workspace trust dialog is skipped when Claude is run with the -p mode. Only use this
                                                    flag in directories you trust.
  --output-format <format>                          Output format (only works with --print): "text" (default), "json" (single result), or "stream-json" (realtime streaming) (choices: "text",
                                                    "json", "stream-json")
  --json-schema <schema>                            JSON Schema for structured output validation. Example: {"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}
  --include-partial-messages                        Include partial message chunks as they arrive (only works with --print and --output-format=stream-json)
  --input-format <format>                           Input format (only works with --print): "text" (default), or "stream-json" (realtime streaming input) (choices: "text", "stream-json")
  --mcp-debug                                       [DEPRECATED. Use --debug instead] Enable MCP debug mode (shows MCP server errors)
  --dangerously-skip-permissions                    Bypass all permission checks. Recommended only for sandboxes with no internet access.
  --allow-dangerously-skip-permissions              Enable bypassing all permission checks as an option, without it being enabled by default. Recommended only for sandboxes with no internet
                                                    access.
  --replay-user-messages                            Re-emit user messages from stdin back on stdout for acknowledgment (only works with --input-format=stream-json and
                                                    --output-format=stream-json)
  --allowedTools, --allowed-tools <tools...>        Comma or space-separated list of tool names to allow (e.g. "Bash(git:*) Edit")
  --tools <tools...>                                Specify the list of available tools from the built-in set. Use "" to disable all tools, "default" to use all tools, or specify tool names
                                                    (e.g. "Bash,Edit,Read"). Only works with --print mode.
  --disallowedTools, --disallowed-tools <tools...>  Comma or space-separated list of tool names to deny (e.g. "Bash(git:*) Edit")
  --mcp-config <configs...>                         Load MCP servers from JSON files or strings (space-separated)
  --system-prompt <prompt>                          System prompt to use for the session
  --append-system-prompt <prompt>                   Append a system prompt to the default system prompt
  --permission-mode <mode>                          Permission mode to use for the session (choices: "acceptEdits", "bypassPermissions", "default", "dontAsk", "plan")
  -c, --continue                                    Continue the most recent conversation
  -r, --resume [sessionId]                          Resume a conversation - provide a session ID or interactively select a conversation to resume
  --fork-session                                    When resuming, create a new session ID instead of reusing the original (use with --resume or --continue)
  --model <model>                                   Model for the current session. Provide an alias for the latest model (e.g. 'sonnet' or 'opus') or a model's full name (e.g.
                                                    'claude-sonnet-4-5-20250929').
  --fallback-model <model>                          Enable automatic fallback to specified model when default model is overloaded (only works with --print)
  --settings <file-or-json>                         Path to a settings JSON file or a JSON string to load additional settings from
  --add-dir <directories...>                        Additional directories to allow tool access to
  --ide                                             Automatically connect to IDE on startup if exactly one valid IDE is available
  --strict-mcp-config                               Only use MCP servers from --mcp-config, ignoring all other MCP configurations
  --session-id <uuid>                               Use a specific session ID for the conversation (must be a valid UUID)
  --agents <json>                                   JSON object defining custom agents (e.g. '{"reviewer": {"description": "Reviews code", "prompt": "You are a code reviewer"}}')
  --setting-sources <sources>                       Comma-separated list of setting sources to load (user, project, local).
  --plugin-dir <paths...>                           Load plugins from directories for this session only (repeatable)
  -v, --version                                     Output the version number
  -h, --help                                        Display help for command

Commands:
  mcp                                               Configure and manage MCP servers
  plugin                                            Manage Claude Code plugins
  migrate-installer                                 Migrate from global npm installation to local installation
  setup-token                                       Set up a long-lived authentication token (requires Claude subscription)
  doctor                                            Check the health of your Claude Code auto-updater
  update                                            Check for updates and install if available
  install [options] [target]                        Install Claude Code native build. Use [target] to specify version (stable, latest, or specific version)
```

## Bwrap

```
$ bwrap --help
usage: bwrap [OPTIONS...] [--] COMMAND [ARGS...]

    --help                       Print this help
    --version                    Print version
    --args FD                    Parse NUL-separated args from FD
    --argv0 VALUE                Set argv[0] to the value VALUE before running the program
    --level-prefix               Prepend e.g. <3> to diagnostic messages
    --unshare-all                Unshare every namespace we support by default
    --share-net                  Retain the network namespace (can only combine with --unshare-all)
    --unshare-user               Create new user namespace (may be automatically implied if not setuid)
    --unshare-user-try           Create new user namespace if possible else continue by skipping it
    --unshare-ipc                Create new ipc namespace
    --unshare-pid                Create new pid namespace
    --unshare-net                Create new network namespace
    --unshare-uts                Create new uts namespace
    --unshare-cgroup             Create new cgroup namespace
    --unshare-cgroup-try         Create new cgroup namespace if possible else continue by skipping it
    --userns FD                  Use this user namespace (cannot combine with --unshare-user)
    --userns2 FD                 After setup switch to this user namespace, only useful with --userns
    --disable-userns             Disable further use of user namespaces inside sandbox
    --assert-userns-disabled     Fail unless further use of user namespace inside sandbox is disabled
    --pidns FD                   Use this pid namespace (as parent namespace if using --unshare-pid)
    --uid UID                    Custom uid in the sandbox (requires --unshare-user or --userns)
    --gid GID                    Custom gid in the sandbox (requires --unshare-user or --userns)
    --hostname NAME              Custom hostname in the sandbox (requires --unshare-uts)
    --chdir DIR                  Change directory to DIR
    --clearenv                   Unset all environment variables
    --setenv VAR VALUE           Set an environment variable
    --unsetenv VAR               Unset an environment variable
    --lock-file DEST             Take a lock on DEST while sandbox is running
    --sync-fd FD                 Keep this fd open while sandbox is running
    --bind SRC DEST              Bind mount the host path SRC on DEST
    --bind-try SRC DEST          Equal to --bind but ignores non-existent SRC
    --dev-bind SRC DEST          Bind mount the host path SRC on DEST, allowing device access
    --dev-bind-try SRC DEST      Equal to --dev-bind but ignores non-existent SRC
    --ro-bind SRC DEST           Bind mount the host path SRC readonly on DEST
    --ro-bind-try SRC DEST       Equal to --ro-bind but ignores non-existent SRC
    --bind-fd FD DEST            Bind open directory or path fd on DEST
    --ro-bind-fd FD DEST         Bind open directory or path fd read-only on DEST
    --remount-ro DEST            Remount DEST as readonly; does not recursively remount
    --overlay-src SRC            Read files from SRC in the following overlay
    --overlay RWSRC WORKDIR DEST Mount overlayfs on DEST, with RWSRC as the host path for writes and
                                 WORKDIR an empty directory on the same filesystem as RWSRC
    --tmp-overlay DEST           Mount overlayfs on DEST, with writes going to an invisible tmpfs
    --ro-overlay DEST            Mount overlayfs read-only on DEST
    --exec-label LABEL           Exec label for the sandbox
    --file-label LABEL           File label for temporary sandbox content
    --proc DEST                  Mount new procfs on DEST
    --dev DEST                   Mount new dev on DEST
    --tmpfs DEST                 Mount new tmpfs on DEST
    --mqueue DEST                Mount new mqueue on DEST
    --dir DEST                   Create dir at DEST
    --file FD DEST               Copy from FD to destination DEST
    --bind-data FD DEST          Copy from FD to file which is bind-mounted on DEST
    --ro-bind-data FD DEST       Copy from FD to file which is readonly bind-mounted on DEST
    --symlink SRC DEST           Create symlink at DEST with target SRC
    --seccomp FD                 Load and use seccomp rules from FD (not repeatable)
    --add-seccomp-fd FD          Load and use seccomp rules from FD (repeatable)
    --block-fd FD                Block on FD until some data to read is available
    --userns-block-fd FD         Block on FD until the user namespace is ready
    --info-fd FD                 Write information about the running container to FD
    --json-status-fd FD          Write container status to FD as multiple JSON documents
    --new-session                Create a new terminal session
    --die-with-parent            Kills with SIGKILL child process (COMMAND) when bwrap or bwrap's parent dies.
    --as-pid-1                   Do not install a reaper process with PID=1
    --cap-add CAP                Add cap CAP when running as privileged user
    --cap-drop CAP               Drop cap CAP when running as privileged user
    --perms OCTAL                Set permissions of next argument (--bind-data, --file, etc.)
    --size BYTES                 Set size of next argument (only for --tmpfs)
    --chmod OCTAL PATH           Change permissions of PATH (must already exist)
```
