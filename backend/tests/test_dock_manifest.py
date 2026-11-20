import hashlib
import importlib.util
import json
import re
import shutil
import subprocess
import sys

from conftest import REPO

DOCK = REPO / "frontend" / "src" / "dock"
SOURCE = REPO / "frontend" / "src"


def _agents():
    spec = importlib.util.spec_from_file_location("formdesign_setup_agents", REPO / "setup" / "agents.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_the_shared_dock_files_match_their_manifest():
    kept = json.loads((DOCK / "MANIFEST.json").read_text(encoding="utf-8"))
    assert kept["module"] == "dock" and kept["version"] == "2.1.0"
    now = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in DOCK.iterdir() if p.is_file() and p.name != "MANIFEST.json"}
    assert kept["files"] == now
    assert f"Module version {kept['version']}." in (DOCK / "CONTRACT.md").read_text(encoding="utf-8")
    assert _agents().dock_problems() == []


def test_agents_sync_names_a_shared_file_edited_in_place(tmp_path):
    agents = _agents()
    copy = tmp_path / "dock"
    shutil.copytree(DOCK, copy)
    assert agents.dock_problems(copy) == []
    with open(copy / "dock.ts", "a", encoding="utf-8") as f:
        f.write("\n")
    assert agents.dock_problems(copy) == ["frontend/src/dock/dock.ts: does not match MANIFEST.json (the shared dock module changes only as a whole new version)"]
    (copy / "extra.ts").write_text("import { api } from '../engine/client';\n", encoding="utf-8")
    said = agents.dock_problems(copy)
    assert "frontend/src/dock/extra.ts: does not match MANIFEST.json (the shared dock module changes only as a whole new version)" in said
    assert "frontend/src/dock/extra.ts: imports ../engine/client from outside the shared folder" in said
    (copy / "MANIFEST.json").unlink()
    assert agents.dock_problems(copy) == ["frontend/src/dock/MANIFEST.json: missing or unreadable"]


def test_sync_check_passes_on_the_committed_folder_and_the_manifest_names_the_dock():
    r = subprocess.run([sys.executable, str(REPO / "setup" / "agents.py"), "sync", "--check"], capture_output=True, text=True, cwd=str(REPO))
    assert r.returncode == 0, r.stdout + r.stderr
    doc = json.loads((REPO / "agents" / "manifest.json").read_text(encoding="utf-8"))
    assert doc["dock"] == {"present": True, "positions": ["bottom", "right"], "tabs": ["chat", "terminal", "claude", "codex", "cursor"]}


def test_nothing_in_the_shared_folder_imports_from_the_app():
    for path in sorted(DOCK.glob("*.ts")):
        for target in re.findall(r"(?:from|import)\s*\(?'([^']+)'", path.read_text(encoding="utf-8")):
            assert not target.startswith("..") and not target.startswith("/"), f"{path.name} imports {target}"
            if target.startswith("./"):
                assert (DOCK / (target[2:] + ".ts")).is_file() or (DOCK / target[2:]).is_file(), f"{path.name} imports {target}"


def test_the_window_keeps_its_layout_key_apart_from_the_panel_layout():
    adapter = (SOURCE / "dock-adapter.ts").read_text(encoding="utf-8")
    assert "export const DOCK_KEY = 'formdesign.agentDock';" in adapter and "export const DOCK_OLD_HEIGHT = 'formdesign.dock.height';" in adapter
    assert "const KEY = 'formdesign.dock';" in (SOURCE / "ui" / "dock.ts").read_text(encoding="utf-8")
    head = (REPO / "frontend" / "index.html").read_text(encoding="utf-8")
    assert "localStorage.getItem('formdesign.agentDock')" in head and "localStorage.getItem('formdesign.dock.height')" in head


def test_the_engine_names_its_own_tab_entry_formdesign_tab(tmp_path):
    from app.services import agent_programs, claude_code

    token, path = claude_code.issue()
    try:
        assert list(json.loads(open(path, encoding="utf-8").read())["mcpServers"]) == ["formdesign_tab"]
    finally:
        claude_code.revoke(token)
    assert any("--allowedTools mcp__formdesign_tab " in part or part == "mcp__formdesign_tab" for part in claude_code.argv("claude", path))
    (tmp_path / ".cursor").mkdir()
    assert agent_programs.connect("cursor", tmp_path) and agent_programs.connect("codex", tmp_path)
    assert list(json.loads((tmp_path / ".cursor" / "mcp.json").read_text(encoding="utf-8"))["mcpServers"]) == ["formdesign_tab"]
    assert (tmp_path / ".codex" / "config.toml").read_text(encoding="utf-8").startswith("[mcp_servers.formdesign_tab]\n")


def test_a_caller_that_hangs_up_withdraws_its_question(monkeypatch):
    import asyncio

    from app.services import agents as A

    said = []
    monkeypatch.setattr(A, "push", lambda what, **more: said.append((what, more)))
    asked = {"times": 0}

    async def gone():
        asked["times"] += 1
        return asked["times"] > 1

    async def run():
        return await A.ask("hang-up-agent", "export_document", {"path": "x"}, "overwrites x", gone)

    assert asyncio.run(run()) is False
    assert said[0][0] == "approval"
    assert said[-1] == ("approvals", {"id": said[0][1]["id"], "withdrawn": True, "outcome": "cancelled"})
    assert not A.approvals and A.policy("hang-up-agent") == "ask"
