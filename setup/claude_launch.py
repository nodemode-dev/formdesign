#!/usr/bin/env python3
"""./formdesign claude and formdesign.bat claude: Claude Code in this terminal with FormDesign's MCP tools attached through the stdio bridge, which starts the engine when it is not running."""

import json
import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NOTE = ("You are Claude Code with FormDesign's tools (the formdesign MCP server). Work on the document open in FormDesign: "
        "start with get_document or outline, find commands with find_commands, change things with run_commands, and look with preview or look. "
        "Use the formdesign-harness skill when it is available.")


def config() -> str:
    server = {"type": "stdio", "command": sys.executable, "args": [os.path.join(ROOT, "setup", "mcp_stdio.py")]}
    return json.dumps({"mcpServers": {"formdesign": server}})


def main(argv: list[str]) -> int:
    claude = os.environ.get("FORMDESIGN_CLAUDE_BIN") or shutil.which("claude")
    if not claude:
        print("[formdesign] Claude Code is not installed: npm install -g @anthropic-ai/claude-code (https://claude.com/claude-code)", file=sys.stderr)
        return 1
    if "--print-config" in argv:
        print(config())
        return 0
    return subprocess.call([claude, "--mcp-config", config(), "--allowedTools", "mcp__formdesign", "--append-system-prompt", NOTE, *argv],
                           cwd=os.environ.get("FORMDESIGN_CWD") or ROOT)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
