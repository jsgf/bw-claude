#!/usr/bin/env python3
"""
Common library for Bubblewrap sandboxing wrappers.
"""

import os
import sys
import argparse
import subprocess
import uuid
from pathlib import Path
import tempfile

# Directories safe to mount from home when using safe mode (default)
SAFE_HOME_DIRS = [
    ".local/share",
    ".local/bin",           # User-installed binaries
    "Documents",
    "Downloads",
    "Projects",
    ".cargo",               # Rust package manager
    ".rustup",              # Rust toolchain manager
    ".npm",                 # npm cache/config
    ".gem",                 # Ruby gems
    ".gradle",              # Gradle (Java/Kotlin builds)
    ".m2",                  # Maven (Java builds)
    ".nvm",                 # Node Version Manager
    ".go",                  # Go workspace
    ".viminfo",             # Vim history and settings
    ".gitconfig",           # Git configuration
]

# Safe subdirectories within ~/.config/ to mount (excludes browsers and sensitive data)
SAFE_CONFIG_DIRS = [
    "git", "nvim", "vim", "htop", "nano", "less", "lsd", "bat",
    "zsh", "bash", "fish", "alacritty", "kitty",
]

# Essential /etc files to mount (minimal /etc)
ESSENTIAL_ETC_FILES = ["hostname", "hosts", "resolv.conf", "passwd", "group"]

# Additional directories to mount (handled separately in mount_minimal_etc)
ESSENTIAL_ETC_DIRS = [
    "pki", "ssl", "crypto-policies",
]


def create_tmp_export_dir(tool_name):
    """Create an isolated /tmp export directory in the real /tmp."""
    session_id = str(uuid.uuid4())[:8]
    export_dir = Path("/tmp") / f"bw-{tool_name}-{session_id}"
    export_dir.mkdir(exist_ok=True, mode=0o755)
    return str(export_dir)


def ensure_tool_dir_exists(target_dir, tool_name):
    """Create the project .<tool_name> directory if it doesn't exist."""
    tool_dir = target_dir / f".{tool_name}"
    if not tool_dir.exists():
        tool_dir.mkdir(exist_ok=True)


def mount_minimal_etc(cmd):
    """Create a minimal /etc with only essential files.

    Creates an empty tmpfs for /etc, then bind mounts only the specific
    files and directories needed for basic operation. This minimizes
    exposure of sensitive files like /etc/shadow.
    """
    # Create empty /etc
    cmd.extend(["--tmpfs", "/etc"])

    # Mount individual essential files
    for filename in ESSENTIAL_ETC_FILES:
        filepath = f"/etc/{filename}"
        cmd.extend(["--ro-bind-try", filepath, filepath])

    # Mount essential directories
    for dirname in ESSENTIAL_ETC_DIRS:
        dirpath = f"/etc/{dirname}"
        cmd.extend(["--ro-bind-try", dirpath, dirpath])

    # Special handling for /etc/resolv.conf if it's a symlink
    resolv_conf = Path("/etc/resolv.conf")
    if resolv_conf.exists() and resolv_conf.is_symlink():
        # Resolve the symlink and bind the real file
        real_resolv = resolv_conf.resolve()
        cmd.extend(["--ro-bind-try", str(real_resolv), "/etc/resolv.conf"])


def mount_safe_home_dirs(cmd, home):
    """Mount only safe directories from home."""
    for dir_name in SAFE_HOME_DIRS:
        dir_path = os.path.join(home, dir_name)
        if os.path.exists(dir_path):
            cmd.extend(["--ro-bind", dir_path, dir_path])

def mount_safe_config_dirs(cmd, home):
    """Mount only safe subdirectories from ~/.config."""
    config_dir = os.path.join(home, ".config")
    for subdir in SAFE_CONFIG_DIRS:
        subdir_path = os.path.join(config_dir, subdir)
        if os.path.exists(subdir_path):
            cmd.extend(["--ro-bind", subdir_path, subdir_path])



def build_bwrap_command(cli_path, args, tool_config, target_dir=None):
    """Build the complete bwrap command with security options."""
    home = str(Path.home())
    tool_name = tool_config["name"]

    if target_dir:
        pwd = str(target_dir)
    elif args.dir:
        pwd = os.path.abspath(args.dir)
    else:
        pwd = str(Path.cwd())

    if not os.path.isdir(pwd):
        print(f"Error: Directory does not exist: {pwd}", file=sys.stderr)
        sys.exit(1)

    cmd = ["bwrap", "--die-with-parent", "--unshare-pid", "--unshare-ipc"]

    if not args.no_network:
        cmd.extend(["--share-net"])

    # Mount an isolated /tmp
    export_tmp = create_tmp_export_dir(tool_name)
    cmd.extend(["--bind", export_tmp, "/tmp"])

    # Mount minimal /etc with only essential files
    mount_minimal_etc(cmd)

    if args.full_home_access:
        # Full home access (unsafe)
        cmd.extend(["--bind", home, home]) 
    else:
        # Safe mode: restrict to safe directories only
        mount_safe_home_dirs(cmd, home)
        # Mount safe .config subdirectories (excludes browsers to protect cookies/credentials)
        mount_safe_config_dirs(cmd, home)

    # System binaries and libraries (read-only)
    for p in ["/usr", "/lib", "/lib64"]:
        if os.path.exists(p):
            cmd.extend(["--ro-bind", p, p])
            
    # Create /bin as symlink to /usr/bin for compatibility
    cmd.extend(["--symlink", "/usr/bin", "/bin"])

    # Tool-specific state directories (e.g., ~/.claude, ~/.gemini)
    global_tool_dir = f"{home}/.{tool_name}"
    if os.path.exists(global_tool_dir):
        cmd.extend(["--bind", global_tool_dir, global_tool_dir])

    # Tool-specific dot file in home (e.g., ~/.claude.json)
    if tool_config.get("home_dot_file"):
        dot_file = f"{home}/{tool_config['home_dot_file']}"
        if not os.path.exists(dot_file):
            # Create empty file so bind mount works if it doesn't exist
            Path(dot_file).touch()
        cmd.extend(["--bind", dot_file, dot_file])

    # $PWD: read-only project directory
    cmd.extend(["--ro-bind", pwd, pwd])
    # $PWD/.tool_name: read-write overlay on top of read-only $PWD
    project_tool_dir = Path(pwd) / f".{tool_name}"
    ensure_tool_dir_exists(Path(pwd), tool_name) # ensures the directory exists before binding
    cmd.extend(["--bind", str(project_tool_dir), str(project_tool_dir)])

    # Process and device access
    cmd.extend(["--proc", "/proc", "--dev-bind", "/dev", "/dev"])
    # Root filesystem setup
    cmd.extend(["--tmpfs", "/root"])
    # Set working directory
    cmd.extend(["--chdir", pwd])

    # Preserve essential environment variables
    path_env = os.getenv('PATH', '/usr/bin:/bin:/usr/sbin:/sbin')
    term_env = os.getenv('TERM', 'xterm')
    cmd.extend([
        "--clearenv",
        "--setenv", "HOME", home,
        "--setenv", "PWD", pwd,
        "--setenv", "USER", os.getenv('USER', 'user'),
        "--setenv", "PATH", path_env,
        "--setenv", "TERM", term_env,
    ])

    # Pass through specified environment variables
    for var_name in args.pass_env_vars:
        var_value = os.getenv(var_name)
        if var_value is not None:
            cmd.extend(["--setenv", var_name, var_value])

    # Shell or CLI command
    if args.shell:
        cmd.extend(["/bin/sh", "-i"])
    else:
        cmd.append(cli_path)
        # Apply default arguments unless explicitly disabled by its flag
        if tool_config.get("default_args") and tool_config.get("default_args_flag"):
            if not getattr(args, tool_config["default_args_flag"]):
                cmd.extend(tool_config["default_args"])
        cmd.extend(args.cli_args)

    # Mount additional paths (--allow-ro and --allow-rw)
    for ro_path in args.allow_ro_paths:
        if os.path.exists(ro_path):
            cmd.extend(["--ro-bind", ro_path, ro_path])
        else:
            print(f"[bw-{tool_name}] Warning: --allow-ro path does not exist: {ro_path}", file=sys.stderr)

    for rw_path in args.allow_rw_paths:
        if os.path.exists(rw_path):
            cmd.extend(["--bind", rw_path, rw_path])
        else:
            print(f"[bw-{tool_name}] Warning: --allow-rw path does not exist: {rw_path}", file=sys.stderr)

    # Print debug info if verbose
    if args.verbose:
        print(f"[bw-{tool_name}] Working directory: {pwd}", file=sys.stderr)
        print(f"[bw-{tool_name}] Export /tmp: {export_tmp}", file=sys.stderr)
        print(f"[bw-{tool_name}] Network: {'disabled' if args.no_network else 'enabled'}",
              file=sys.stderr)
        print(f"[bw-{tool_name}] Home access: {'full (unsafe)' if args.full_home_access else 'safe (restricted)'}",
              file=sys.stderr)
        if args.shell:
            print(f"[bw-{tool_name}] Mode: Interactive shell", file=sys.stderr)
        if args.allow_ro_paths:
            print(f"[bw-{tool_name}] Additional read-only paths: {', '.join(args.allow_ro_paths)}", file=sys.stderr)
        if args.allow_rw_paths:
            print(f"[bw-{tool_name}] Additional read-write paths: {', '.join(args.allow_rw_paths)}", file=sys.stderr)
        print(f"[bw-{tool_name}] Command: {' '.join(cmd)}", file=sys.stderr)

    return cmd

def parse_args(prog_name, tool_name, tool_config):
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(
        prog=prog_name,
        description=f"Bubblewrap sandboxing wrapper for {tool_name.capitalize()} CLI",
        add_help=False, # Custom help handling
    )

    # Generic arguments
    parser.add_argument(
        "--no-network",
        action="store_true",
        help="Disable network access (default: network enabled)",
    )
    parser.add_argument(
        "--full-home-access",
        action="store_true",
        help="Allow full home directory access (default: safe dirs only)",
    )
    parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="Print sandbox configuration and bwrap command to stderr",
    )
    parser.add_argument(
        "--shell",
        action="store_true",
        help="Launch an interactive shell in the sandbox (for debugging)",
    )
    parser.add_argument(
        "--allow-ro",
        action="append",
        dest="allow_ro_paths",
        default=[], # Set default to empty list
        metavar="PATH",
        help="Mount additional read-only path (can be used multiple times)",
    )
    parser.add_argument(
        "--allow-rw",
        action="append",
        dest="allow_rw_paths",
        default=[], # Set default to empty list
        metavar="PATH",
        help="Mount additional read-write path (can be used multiple times)",
    )
    parser.add_argument(
        "--dir",
        metavar="PATH",
        help="Set working directory in sandbox (default: current directory)",
    )
    parser.add_argument(
        "--help",
        "-h",
        action="store_true",
        help="Show this help message",
    )

    parser.add_argument(
        "--pass-env",
        action="append",
        dest="pass_env_vars",
        default=[],
        metavar="VAR_NAME",
        help="Pass an environment variable into the sandbox (can be used multiple times)",
    )

    # Tool-specific argument for disabling default args
    if tool_config.get("default_args_flag"):
        parser.add_argument(
            f"--{tool_config['default_args_flag'].replace('_', '-')}",
            action="store_true",
            help=f"Disable default arguments for {tool_name} (default: enabled)",
        )

    # Parse arguments, handling -- separator for CLI args
    args_list = sys.argv[1:]
    if "--" in args_list:
        sep_idx = args_list.index("--")
        bw_args = args_list[:sep_idx]
        cli_args = args_list[sep_idx + 1:]
        args = parser.parse_args(bw_args)
        args.cli_args = cli_args
    else:
        # Parse known arguments, rest go to CLI
        args, cli_args = parser.parse_known_args()
        args.cli_args = cli_args

    if args.help:
        parser.print_help()
        print(f"\n{tool_name.capitalize()} options:")
        if tool_config.get("help_text"):
            print(tool_config["help_text"])
        print(f"\n{tool_name.capitalize()} arguments are passed through unchanged.")
        print(f"Use -- to explicitly separate {prog_name} options from {tool_name.capitalize()} options:")
        print(f"  ./{prog_name} --no-network -- {tool_name}_command_and_args")
        print("\nAdditional paths can be mounted with --allow-ro and --allow-rw:")
        print(f"  ./{prog_name} --allow-ro /var/log --allow-rw /tmp/custom -- {tool_name}_command")
        sys.exit(0)

    return args

