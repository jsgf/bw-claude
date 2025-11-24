#!/usr/bin/env python3
"""
Bubblewrap sandboxing wrapper for Gemini CLI.
"""

import sys
import os
from pathlib import Path
import subprocess
import bw_lib

def get_gemini_path():
    """Return the path to the Gemini CLI executable."""
    # This is a placeholder path.
    # User may need to adjust this depending on how Gemini CLI is installed.
    gemini_path = Path.home() / ".local" / "bin" / "gemini"
    if not gemini_path.exists():
        # Fallback to checking PATH
        import shutil
        if shutil.which("gemini"):
            return "gemini"
        print(f"Error: Gemini CLI not found at {gemini_path} or in PATH", file=sys.stderr)
        sys.exit(1)
    return str(gemini_path)

GEMINI_CONFIG = {
    "name": "gemini",
    "get_cli_path": get_gemini_path,
    "help_text": """
Gemini arguments are passed through unchanged.

For authentication, you may need to pass environment variables into the sandbox.
Use the --pass-env argument for each variable you need.

Examples:
  bw-gemini --pass-env OPENAI_ENDPOINT_API_KEY -- ...
  bw-gemini --pass-env GOOGLE_APPLICATION_CREDENTIALS -- ...
  bw-gemini --pass-env LDR_LLM__OPENAI_ENDPOINT_API_KEY -- ...
"""
}

def main():
    """Main entry point."""
    prog_name = f"bw-{GEMINI_CONFIG['name']}"
    args = bw_lib.parse_args(prog_name, GEMINI_CONFIG['name'], GEMINI_CONFIG)

    target_dir = Path(args.dir).absolute() if args.dir else Path.cwd()
    if args.dir and not target_dir.is_dir():
        print(f"Error: Directory does not exist: {target_dir}", file=sys.stderr)
        sys.exit(1)

    try:
        cli_path = None
        if not args.shell:
            cli_path = GEMINI_CONFIG["get_cli_path"]()

        bwrap_cmd = bw_lib.build_bwrap_command(
            cli_path, args, GEMINI_CONFIG, target_dir
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
