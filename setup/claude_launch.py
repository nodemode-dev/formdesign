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
    sys.path.insert(0, os.path.join(ROOT, "backend", "app", "services"))
    import agent_lookup
    found = agent_lookup.check(agent_lookup.find("claude", override_vars=("FORMDESIGN_CLAUDE_BIN",)))
    if found.state != "runs":
        print("[formdesign] " + agent_lookup.sentence(found, agent_lookup.install("claude")), file=sys.stderr)
        return 1
    if "--print-config" in argv:
        print(config())
        return 0
    cmd = [found.path, "--mcp-config", config(), "--allowedTools", "mcp__formdesign", "--append-system-prompt", NOTE, *argv]
    return subprocess.call(agent_lookup.start_argv(cmd), cwd=os.environ.get("FORMDESIGN_CWD") or ROOT, env=agent_lookup.child_env(found.path))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
