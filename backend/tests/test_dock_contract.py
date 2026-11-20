import json
import os
import stat
import threading

import pytest

from conftest import CHANGING_TOOL, ENGINE_PORT, MCP_PATH, WINDOW_ORIGINS, set_policy

WINDOW = {"origin": WINDOW_ORIGINS[0]}
LOOPBACK_HOST = {"host": f"127.0.0.1:{ENGINE_PORT}"}
AGENT = "family-test-agent"


def call(client, name, args, agent=AGENT, headers=None):
    init = {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": agent, "version": "1"}}}
    sid = client.post(MCP_PATH, json=init, headers=headers or {}).headers.get("mcp-session-id")
    body = {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": name, "arguments": args}}
    return client.post(MCP_PATH, json=body, headers={"mcp-session-id": sid, **(headers or {})}).json()["result"]


def text_of(result):
    return " ".join(c.get("text", "") for c in result.get("content", []))


def policy_of(client, name):
    return next((c["policy"] for c in client.get("/api/agents").json()["clients"] if c["name"] == name), None)


def wait_for(ws, what):
    for _ in range(400):
        msg = ws.receive_json()
        if msg.get("type") == "agents" and msg.get("what") == what:
            return msg
    raise AssertionError(f"no {what} event reached the window")


def ask(client, answers):
    from app.core import window_key

    out = {"answers": []}
    with client.websocket_connect(f"/api/ws?ticket={window_key.ticket()}", headers={**LOOPBACK_HOST, **WINDOW}) as ws:
        worker = threading.Thread(target=lambda: out.setdefault("result", call(client, *CHANGING_TOOL)))
        worker.start()
        out["event"] = wait_for(ws, "approval")
        out["listed"] = client.get("/api/agents").json()["approvals"]
        for answer in answers:
            out["answers"].append(client.post(f"/api/agents/approvals/{out['event']['id']}", json=answer, headers=WINDOW))
        out["gone"] = wait_for(ws, "approvals")
        worker.join(30)
    return out


@pytest.fixture(autouse=True)
def policy_reset(client):
    set_policy("ask")
    yield
    set_policy("ask")
    client.post("/api/agents/enabled", json={"enabled": True}, headers=WINDOW)


@pytest.fixture
def stand_ins(tmp_path, monkeypatch):
    from app.services import agent_programs, claude_code

    folder = tmp_path / "bin"
    folder.mkdir()
    lines = {"claude": "2.9.1 (Claude Code)", "codex": "codex-cli 0.77.0", "cursor-agent": "2026.11.01-abc"}
    for name, line in lines.items():
        script = folder / name
        script.write_text(f'#!/bin/sh\nif [ "$1" = "--version" ]; then echo "{line}"; exit 0; fi\nif [ "$1" = "auth" ]; then echo \'{{"loggedIn": true}}\'; exit 0; fi\necho started\nread line\n')
        script.chmod(0o755)
    monkeypatch.setenv("PATH", f"{folder}{os.pathsep}{os.environ.get('PATH', '')}")
    monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
    monkeypatch.delenv("CLAUDE_CODE_OAUTH_TOKEN", raising=False)
    monkeypatch.setattr(claude_code, "_cache", {"at": 0.0, "value": None})
    monkeypatch.setattr(agent_programs, "_cache", {"at": 0.0, "value": None})
    return folder


def test_the_question_says_who_asks_what_and_which_answers_it_takes(client):
    out = ask(client, [{"approve": True}])
    event = out["event"]
    assert event["client"] == AGENT and event["tool"] == CHANGING_TOOL[0] and event["reason"].startswith("overwrites ")
    assert event["agent"] == AGENT
    assert event["asked"] == f"run {CHANGING_TOOL[0]}: it {event['reason']}"
    assert json.loads(event["detail"]) == CHANGING_TOOL[1]
    assert event["answers"] == ["deny", "once", "always"] and event["alwaysScope"] == "agent"
    assert event["expiresAt"] == pytest.approx(event["at"] + event["wait"]) and event["wait"] == 300.0
    listed = out["listed"][0]
    assert {k: listed[k] for k in ("id", "agent", "asked", "detail", "answers", "alwaysScope", "expiresAt")} == {k: event[k] for k in ("id", "agent", "asked", "detail", "answers", "alwaysScope", "expiresAt")}


def test_an_approval_is_for_one_step_and_the_question_is_withdrawn_by_id(client):
    out = ask(client, [{"approve": True}])
    assert out["answers"][0].json() == {"ok": True, "always": False}
    assert "did not approve" not in text_of(out["result"])
    assert out["gone"] == {"type": "agents", "what": "approvals", "id": out["event"]["id"], "withdrawn": True, "outcome": "approved"}
    assert policy_of(client, AGENT) == "ask"
    assert "No FormDesign window is open" in text_of(call(client, *CHANGING_TOOL))


def test_a_declined_question_is_withdrawn_as_declined(client):
    out = ask(client, [{"approve": False}])
    assert out["result"].get("isError") and "did not approve" in text_of(out["result"])
    assert out["gone"]["id"] == out["event"]["id"] and out["gone"]["outcome"] == "declined"


def test_always_allows_this_agent_and_no_other(client):
    out = ask(client, [{"approve": True, "always": True}])
    assert out["answers"][0].json() == {"ok": True, "always": True}
    assert policy_of(client, AGENT) == "allow"
    assert "No FormDesign window is open" not in text_of(call(client, *CHANGING_TOOL))
    assert "No FormDesign window is open" in text_of(call(client, *CHANGING_TOOL, agent="another-agent"))


def test_always_cannot_go_with_a_refusal(client):
    out = ask(client, [{"approve": False, "always": True}, {"approve": False}])
    assert out["answers"][0].status_code == 400 and out["answers"][1].status_code == 200
    assert policy_of(client, AGENT) == "ask"


def test_an_unanswered_question_expires_and_is_withdrawn(client, monkeypatch):
    from app.services import agents

    monkeypatch.setattr(agents, "APPROVAL_WAIT", 0.3)
    out = ask(client, [])
    assert out["gone"] == {"type": "agents", "what": "approvals", "id": out["event"]["id"], "withdrawn": True, "outcome": "expired"}
    assert out["result"].get("isError") and "did not approve" in text_of(out["result"])
    assert client.post(f"/api/agents/approvals/{out['event']['id']}", json={"approve": True}, headers=WINDOW).status_code == 404


@pytest.mark.parametrize("kind,name", [("claude", "Claude Code in FormDesign"), ("codex", "Codex in FormDesign"), ("cursor", "Cursor in FormDesign")])
def test_turning_agents_off_stops_the_apps_own_tabs_too(client, kind, name):
    from app.services import claude_code

    token = claude_code.issue_for(kind)
    try:
        own = {"x-formdesign-inapp": token}
        assert not call(client, "list_templates", {}, headers=own).get("isError")
        assert policy_of(client, name) == "allow"
        assert client.post("/api/agents/enabled", json={"enabled": False}, headers=WINDOW).status_code == 200
        refused = call(client, "list_templates", {}, headers=own)
        assert refused.get("isError") and "turned off" in text_of(refused)
    finally:
        claude_code.revoke(token)
    assert claude_code.inapp_kind(token) is None


def test_an_outside_client_cannot_take_an_own_tabs_name(client):
    call(client, "list_templates", {}, agent="Codex in FormDesign")
    names = [c["name"] for c in client.get("/api/agents").json()["clients"]]
    assert "Codex in FormDesign (unverified)" in names


def test_agent_status_needs_the_window_key(bare):
    assert bare.get("/api/agents/programs").status_code == 403


def test_agent_status_reports_each_program_in_one_answer(client, stand_ins):
    found = client.get("/api/agents/programs?fresh=true").json()
    assert set(found) == {"claude", "codex", "cursor"}
    assert found["claude"] == found["claude"] | {"installed": True, "path": str(stand_ins / "claude"), "version": "2.9.1", "signedIn": True, "install": "npm install -g @anthropic-ai/claude-code"}
    assert found["codex"] == found["codex"] | {"installed": True, "path": str(stand_ins / "codex"), "version": "0.77.0", "signedIn": None, "install": "npm install -g @openai/codex"}
    assert found["cursor"] == found["cursor"] | {"installed": True, "version": "2026.11.01-abc", "signedIn": None, "install": "curl https://cursor.com/install -fsS | bash"}


def test_agent_status_gives_the_install_line_for_a_missing_program(client, monkeypatch):
    from app.services import agent_programs

    monkeypatch.setattr(agent_programs, "_cache", {"at": 0.0, "value": None})
    monkeypatch.setattr(agent_programs, "find", lambda kind: None)
    found = client.get("/api/agents/programs").json()
    for kind in ("codex", "cursor"):
        assert found[kind]["installed"] is False and found[kind]["path"] is None and found[kind]["version"] is None
        assert found[kind]["install"] and " is " not in found[kind]["install"]


@pytest.mark.skipif(os.name == "nt", reason="the stand-ins are shell scripts")
@pytest.mark.parametrize("kind,config,holder", [("codex", ".codex/config.toml", "FORMDESIGN_INAPP_TOKEN"), ("cursor", ".cursor/mcp.json", "${env:FORMDESIGN_INAPP_TOKEN}")])
def test_a_program_tab_gets_its_own_token_in_its_environment_only(client, stand_ins, kind, config, holder):
    from app.services import claude_code, pty

    res = client.post("/api/term", json={"preset": kind, "cols": 80, "rows": 20})
    assert res.status_code == 200 and res.json()["preset"] == kind
    session = pty.terminals.sessions[res.json()["id"]]
    try:
        token = session.env["FORMDESIGN_INAPP_TOKEN"]
        assert claude_code.inapp_kind(token) == kind
        assert all(token not in part for part in session.argv)
        assert session.env["FORMDESIGN_MCP_URL"].endswith("/mcp")
        assert session.cwd == str(claude_code.workdir())
        written = os.path.join(session.cwd, config)
        assert stat.S_IMODE(os.stat(written).st_mode) == 0o600
        text = open(written, encoding="utf-8").read()
        assert holder in text and token not in text and "x-formdesign-inapp" in text
    finally:
        assert client.delete(f"/api/term/{session.id}").status_code == 200
    assert claude_code.inapp_kind(token) is None


def test_the_tab_entry_keeps_other_servers_and_leaves_an_entry_the_person_wrote(tmp_path):
    from app.services import agent_programs

    shared = tmp_path / "shared"
    (shared / ".codex").mkdir(parents=True)
    (shared / ".cursor").mkdir()
    (shared / ".codex" / "config.toml").write_text('model = "x"\n\n[mcp_servers.other]\ncommand = "other"\n', encoding="utf-8")
    (shared / ".cursor" / "mcp.json").write_text(json.dumps({"mcpServers": {"other": {"command": "other"}}}), encoding="utf-8")
    assert agent_programs.connect("codex", shared) and agent_programs.connect("cursor", shared)
    toml = (shared / ".codex" / "config.toml").read_text(encoding="utf-8")
    assert toml.startswith('model = "x"\n\n[mcp_servers.other]\ncommand = "other"\n') and "[mcp_servers.formdesign_tab]" in toml and "[mcp_servers.formdesign]" not in toml
    servers = json.loads((shared / ".cursor" / "mcp.json").read_text(encoding="utf-8"))["mcpServers"]
    assert servers["other"] == {"command": "other"} and "formdesign_tab" in servers and "formdesign" not in servers

    outside = tmp_path / "outside"
    (outside / ".codex").mkdir(parents=True)
    (outside / ".cursor").mkdir()
    outside_toml = '[mcp_servers.formdesign]\ncommand = "./formdesign"\nargs = ["mcp"]\n'
    outside_entry = {"command": "./formdesign", "args": ["mcp"]}
    (outside / ".codex" / "config.toml").write_text(outside_toml, encoding="utf-8")
    (outside / ".cursor" / "mcp.json").write_text(json.dumps({"mcpServers": {"formdesign": outside_entry}}), encoding="utf-8")
    assert agent_programs.connect("codex", outside) and agent_programs.connect("cursor", outside)
    toml = (outside / ".codex" / "config.toml").read_text(encoding="utf-8")
    assert toml.startswith(outside_toml) and "[mcp_servers.formdesign_tab]" in toml
    servers = json.loads((outside / ".cursor" / "mcp.json").read_text(encoding="utf-8"))["mcpServers"]
    assert servers["formdesign"] == outside_entry and "formdesign_tab" in servers

    own = tmp_path / "own"
    (own / ".codex").mkdir(parents=True)
    (own / ".cursor").mkdir()
    own_toml = '[mcp_servers.formdesign_tab]\ncommand = "./formdesign"\nargs = ["mcp"]\n'
    own_json = json.dumps({"mcpServers": {"formdesign_tab": {"command": "./formdesign", "args": ["mcp"]}}})
    (own / ".codex" / "config.toml").write_text(own_toml, encoding="utf-8")
    (own / ".cursor" / "mcp.json").write_text(own_json, encoding="utf-8")
    assert agent_programs.connect("codex", own) is None and agent_programs.connect("cursor", own) is None
    assert (own / ".codex" / "config.toml").read_text(encoding="utf-8") == own_toml
    assert (own / ".cursor" / "mcp.json").read_text(encoding="utf-8") == own_json


def test_a_missing_program_tab_is_refused_with_its_install_line(client, monkeypatch):
    from app.services import agent_programs

    monkeypatch.setattr(agent_programs, "find", lambda kind: None)
    res = client.post("/api/term", json={"preset": "codex"})
    assert res.status_code == 400 and res.json()["detail"] == "Codex is not installed. Install it with: npm install -g @openai/codex"


def test_a_tab_that_ends_tells_a_typed_listener_and_prints_for_a_plain_one():
    import asyncio

    from app.services import pty

    session = pty.Session("ended", "shell", None)
    plain, typed = asyncio.Queue(), asyncio.Queue()
    session.listeners.update({plain, typed})
    session.typed.add(typed)
    session.feed(b"output")
    pty.terminals._exit(session, 3)
    assert plain.get_nowait() == b"output" and typed.get_nowait() == b"output"
    printed = plain.get_nowait()
    assert printed.endswith("Restart runs it again]\r\n".encode()) and b"code 3" in printed
    assert typed.get_nowait() == {"type": "exit", "code": 3}
    assert session.replay() == b"output" + printed and session.replay(True) == b"output"


def test_attaching_to_an_ended_tab_replays_and_says_it_ended(client):
    from app.core import window_key
    from app.services import pty

    session = pty.Session("ended2", "shell", None)
    session.feed(b"output")
    pty.terminals._exit(session, 0)
    pty.terminals.sessions["ended2"] = session
    try:
        with client.websocket_connect(f"/api/term/ended2/ws?ticket={window_key.ticket()}&notices=1", headers=LOOPBACK_HOST) as ws:
            assert json.loads(ws.receive_text()) == {"type": "hello", "alive": False, "kind": "shell"}
            assert ws.receive_bytes() == b"output"
            assert json.loads(ws.receive_text()) == {"type": "exit", "code": 0}
        with client.websocket_connect(f"/api/term/ended2/ws?ticket={window_key.ticket()}", headers=LOOPBACK_HOST) as ws:
            assert ws.receive_bytes() == session.replay() and session.replay().endswith("Restart runs it again]\r\n".encode())
    finally:
        pty.terminals.sessions.pop("ended2", None)
