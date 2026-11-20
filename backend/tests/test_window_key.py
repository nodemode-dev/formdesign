import json
import os
import stat
import subprocess
import sys

import pytest
from starlette.websockets import WebSocketDisconnect

from conftest import BACKEND, ENGINE_PORT, WINDOW_ORIGINS, free_port, window_headers

LOOPBACK = {"host": f"127.0.0.1:{ENGINE_PORT}", "origin": WINDOW_ORIGINS[0]}
WINDOW = {"origin": WINDOW_ORIGINS[0]}
KEYED_READS = ["/api/term", "/api/agents", "/api/claude/status", "/api/claude/chat", "/api/claude/sessions"]
KEYED_WRITES = [
    ("POST", "/api/agents/enabled", {"enabled": True}),
    ("POST", "/api/agents/policy", {"client": "probe", "policy": "ask"}),
    ("POST", "/api/claude/prefs", {"claudeFolder": ""}),
    ("POST", "/api/mcp/settings", {"enabled": True}),
    ("POST", "/api/window/ticket", {}),
]


@pytest.mark.parametrize("path", KEYED_READS)
def test_secret_reads_refuse_without_the_key(bare, path):
    for headers in ({}, WINDOW, {"x-formdesign-window-key": "x" * 43}):
        r = bare.get(path, headers=headers)
        assert r.status_code == 403
        assert r.json()["detail"] == "Only the formdesign window can do this"


@pytest.mark.parametrize("path", KEYED_READS)
def test_secret_reads_work_with_the_key(client, path):
    assert client.get(path, headers=window_headers()).status_code != 403


@pytest.mark.parametrize("method,path,body", KEYED_WRITES)
def test_window_only_write_refuses_a_window_origin_without_the_key(bare, method, path, body):
    assert bare.request(method, path, json=body, headers=WINDOW).status_code == 403


@pytest.mark.parametrize("method,path,body", KEYED_WRITES)
def test_window_only_write_works_with_the_key(client, method, path, body):
    assert client.request(method, path, json=body, headers={**window_headers(), **WINDOW}).status_code != 403


def test_the_terminal_token_is_not_handed_out(client):
    assert "token" not in client.get("/api/term").json()


def test_bootstrap_answers_once_then_refuses(bare, monkeypatch):
    from app.core import window_key

    monkeypatch.setitem(window_key.STATE, "claimed", False)
    monkeypatch.setitem(window_key.STATE, "secondClaim", False)
    first = bare.post("/api/window/key")
    assert first.status_code == 200 and first.json()["key"] == window_key.KEY
    assert bare.post("/api/window/key").status_code == 409
    assert bare.get("/api/capabilities").json()["secondWindowClaim"] is True


def test_bootstrap_does_not_exist_when_the_shell_reads_the_key(bare, monkeypatch):
    from app.core import window_key

    monkeypatch.setenv(window_key.STDOUT_ENV, "1")
    monkeypatch.setitem(window_key.STATE, "claimed", False)
    assert bare.post("/api/window/key").status_code == 404


def test_a_ticket_works_once(client):
    from app.core import window_key

    ticket = window_key.ticket()
    assert window_key.spend(ticket)
    assert not window_key.spend(ticket)
    assert not window_key.spend("")


def test_live_socket_without_a_ticket_is_refused(client):
    with pytest.raises(WebSocketDisconnect) as refused:
        with client.websocket_connect("/api/ws", headers=LOOPBACK) as ws:
            ws.receive_json()
    assert refused.value.code == 4403


def test_live_socket_ticket_is_spent_by_one_connection(client):
    from app.core import window_key

    ticket = window_key.ticket()
    with client.websocket_connect(f"/api/ws?ticket={ticket}", headers=LOOPBACK) as ws:
        assert ws.receive_json()["type"] == "hello"
    with pytest.raises(WebSocketDisconnect) as refused:
        with client.websocket_connect(f"/api/ws?ticket={ticket}", headers=LOOPBACK) as ws:
            ws.receive_json()
    assert refused.value.code == 4403


def test_a_socket_without_the_key_cannot_hold_the_window_seat(client):
    from app.engine.core import core

    before = len(core.listeners)
    with pytest.raises(WebSocketDisconnect):
        with client.websocket_connect("/api/ws", headers=LOOPBACK) as ws:
            ws.receive_json()
    assert len(core.listeners) == before


def test_terminal_socket_needs_a_ticket(client):
    from app.core import window_key

    with pytest.raises(WebSocketDisconnect) as refused:
        with client.websocket_connect("/api/term/none/ws", headers=LOOPBACK) as ws:
            ws.receive()
    assert refused.value.code == 4403
    with pytest.raises(WebSocketDisconnect) as missing:
        with client.websocket_connect(f"/api/term/none/ws?ticket={window_key.ticket()}", headers=LOOPBACK) as ws:
            ws.receive()
    assert missing.value.code == 4404


def test_a_terminal_cannot_be_started_without_the_key(bare, client):
    assert bare.post("/api/term", json={"preset": "shell"}).status_code == 403
    assert bare.post("/api/term", json={"preset": "shell"}, headers=WINDOW).status_code == 403
    assert client.get("/api/term").json()["sessions"] == []


def test_inapp_token_lives_in_a_private_file_until_revoked():
    from app.services import claude_code

    token, path = claude_code.issue()
    assert claude_code.is_inapp(token)
    if os.name != "nt":
        assert stat.S_IMODE(os.stat(path).st_mode) == 0o600
    assert json.loads(open(path, encoding="utf-8").read())["mcpServers"]["formdesign_tab"]["headers"]["x-formdesign-inapp"] == token
    claude_code.revoke(token)
    assert not os.path.exists(path)
    assert not claude_code.is_inapp(token)


def test_each_token_is_its_own():
    from app.services import claude_code

    a, pa = claude_code.issue()
    b, pb = claude_code.issue()
    try:
        assert a != b and pa != pb
        claude_code.revoke(a)
        assert not claude_code.is_inapp(a) and claude_code.is_inapp(b)
    finally:
        claude_code.revoke(a)
        claude_code.revoke(b)


def test_a_claude_tab_revokes_its_token_when_it_closes(monkeypatch):
    from app.services import claude_code, pty

    token, _ = claude_code.issue()
    session = pty.Session("t1", "claude", None)
    session.on_close.append(lambda: claude_code.revoke(token))
    pty.terminals.sessions["t1"] = session
    session.exited = 0
    assert claude_code.is_inapp(token)
    pty.terminals.close("t1")
    assert not claude_code.is_inapp(token)


def test_a_plain_terminal_environment_holds_no_token():
    from app.services import claude_code, pty

    token, _ = claude_code.issue()
    try:
        assert token not in "".join(pty.environment().values())
        assert token not in "".join(claude_code.child_env().values())
    finally:
        claude_code.revoke(token)


def test_engine_prints_the_key_line_once_before_its_address(tmp_path):
    env = {k: v for k, v in os.environ.items() if not k.startswith("FORMDESIGN_")}
    env.update(FORMDESIGN_DATA_DIR=str(tmp_path), FORMDESIGN_PORT="0", FORMDESIGN_WINDOW_KEY_STDOUT="1", HOME=str(tmp_path))
    proc = subprocess.Popen([sys.executable, "formdesign_engine.py"], cwd=BACKEND, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    lines = []
    try:
        while not any("http://127.0.0.1:" in line for line in lines):
            line = proc.stdout.readline()
            assert line, "\n".join(lines)
            lines.append(line)
    finally:
        proc.terminate()
        proc.wait(timeout=10)
    keyed = [i for i, line in enumerate(lines) if line.startswith("formdesign-window-key: ")]
    assert len(keyed) == 1
    assert keyed[0] < len(lines) - 1
    assert len(lines[keyed[0]].split(": ", 1)[1].strip()) >= 32


def test_engine_prints_no_key_line_when_the_shell_did_not_ask(tmp_path):
    env = {k: v for k, v in os.environ.items() if not k.startswith("FORMDESIGN_")}
    env.update(FORMDESIGN_DATA_DIR=str(tmp_path), FORMDESIGN_PORT="0", HOME=str(tmp_path))
    proc = subprocess.Popen([sys.executable, "formdesign_engine.py"], cwd=BACKEND, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    lines = []
    try:
        while not any("http://127.0.0.1:" in line for line in lines):
            line = proc.stdout.readline()
            assert line, "\n".join(lines)
            lines.append(line)
    finally:
        proc.terminate()
        proc.wait(timeout=10)
    assert not any("window-key" in line for line in lines)


def run_cli(engine, *args):
    env = {k: v for k, v in os.environ.items() if not k.startswith("FORMDESIGN_")}
    env.update(FORMDESIGN_ENGINE_URL=engine["base"], HOME=str(engine["data"]))
    return subprocess.run([sys.executable, "-m", "app.cli", *args], cwd=BACKEND, env=env, capture_output=True, text=True, timeout=120)


def test_the_command_line_follows_the_stored_choice_and_cannot_change_it(engine):
    out = run_cli(engine, "cmd", "file.package")
    assert out.returncode == 1
    assert "No FormDesign window is open to approve" in out.stdout + out.stderr
    assert not (engine["data"] / "agents.json").exists()
    assert run_cli(engine, "tool", "list_templates").returncode == 0
    assert not (engine["data"] / "agents.json").exists()
