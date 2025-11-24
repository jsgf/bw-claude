#!/usr/bin/env python3
"""
Bubblewrap sandboxing wrapper for Claude CLI.
"""

import sys
import os
from pathlib import Path
import subprocess
import bw_lib

def get_claude_path():
    """Return the path to the Claude CLI executable."""
    claude_path = Path.home() / ".claude" / "local" / "claude"
    if not claude_path.exists():
        print(f"Error: Claude CLI not found at {claude_path}", file=sys.stderr)
        sys.exit(1)
    return str(claude_path)

CLAUDE_CONFIG = {
    "name": "claude",
    "get_cli_path": get_claude_path,
    "home_dot_file": ".claude.json",
    "default_args": ["--dangerously-skip-permissions"],
    "default_args_flag": "no_skip_permissions",
    "help_text": """
Claude options:
  By default, --dangerously-skip-permissions is passed to Claude.
  Use --no-skip-permissions to disable this behavior.
"""
}

def main():
    """Main entry point."""
    prog_name = f"bw-{CLAUDE_CONFIG['name']}"
    args = bw_lib.parse_args(prog_name, CLAUDE_CONFIG['name'], CLAUDE_CONFIG)

    target_dir = Path(args.dir).absolute() if args.dir else Path.cwd()
    if args.dir and not target_dir.is_dir():
        print(f"Error: Directory does not exist: {target_dir}", file=sys.stderr)
        sys.exit(1)

    try:
        cli_path = None
        if not args.shell:
            cli_path = CLAUDE_CONFIG["get_cli_path"]()

        bwrap_cmd = bw_lib.build_bwrap_command(
            cli_path, args, CLAUDE_CONFIG, target_dir
        )

        proc = subprocess.run(bwrap_cmd, text=args.shell)
        sys.exit(proc.returncode)

    except FileNotFoundError:
        print("Error: bwrap not found. Please install bubblewrap.", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        sys.exit(130)


if __name__ == "__main__":
    main()