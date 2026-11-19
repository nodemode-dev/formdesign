import json
import subprocess
import sys

from conftest import REPO


def run_sync_check():
    return subprocess.run([sys.executable, str(REPO / "setup" / "agents.py"), "sync", "--check"], capture_output=True, text=True, cwd=str(REPO))


def test_the_committed_manifest_is_what_sync_would_write():
    r = run_sync_check()
    assert r.returncode == 0, r.stdout + r.stderr
    assert "agents/manifest.json" not in r.stdout


def test_the_manifest_names_every_tool_the_server_lists():
    from app.routers.mcp import TOOLS
    doc = json.loads((REPO / "agents" / "manifest.json").read_text(encoding="utf-8"))
    assert doc["format"] == "form-agent-manifest" and doc["app"] == "formdesign"
    assert doc["mcp"]["tools"] == [t["name"] for t in TOOLS]
    assert set(doc["mcp"]["read_only"]) | set(doc["mcp"]["approval_gated"]) <= set(doc["mcp"]["tools"])
    assert not set(doc["mcp"]["read_only"]) & set(doc["mcp"]["approval_gated"])


def test_only_the_gated_tools_can_ask_for_approval():
    from app.services import agents as A
    from app.routers.mcp import TOOLS
    reach = {"action": "share", "path": str(REPO / "README.md"), "id": "file.print", "commands": [{"command": "file.print"}]}
    for t in TOOLS:
        if t["name"] not in A.GATED_TOOLS:
            assert A.gate_reason(t["name"], {**reach, "action": "publish"}) is None, t["name"]
    assert A.gate_reason("review", {"action": "share"})
    assert A.gate_reason("run_commands", {"commands": [{"command": "file.print"}]})
