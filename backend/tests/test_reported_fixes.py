import json
import os
import stat
from pathlib import Path

import pytest

from app.engine.core import core
from conftest import INAPP_HEADER, MCP_PATH, inapp_token, own_origin, set_policy


def docs_and_pages(client):
    st = client.portal.call(core.state)
    return len(st["docs"]), client.portal.call(core.inspect)["pageCount"]


def new_document(client, pages=1):
    r = client.post("/api/documents/new", json={"preset": "Letter", "pages": pages, "facingPages": False})
    assert r.status_code == 200, r.text
    return docs_and_pages(client)


def mcp_call(client, name, args, headers=None):
    init = {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "family-test-agent", "version": "1"}}}
    r = client.post(MCP_PATH, json=init, headers=headers or {})
    hdrs = {**(headers or {}), **({"mcp-session-id": r.headers["mcp-session-id"]} if r.headers.get("mcp-session-id") else {})}
    r = client.post(MCP_PATH, json={"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": name, "arguments": args}}, headers=hdrs)
    assert r.status_code == 200, r.text
    return r.json()


def test_rest_command_runs_in_the_active_document(client):
    docs, pages = new_document(client)
    r = client.post("/api/commands/layout.pages.insert", json={"params": {"count": 2}}, headers={"origin": own_origin()})
    assert r.status_code == 200, r.text
    assert docs_and_pages(client) == (docs, pages + 2)


def test_mcp_run_command_runs_in_the_active_document(client):
    docs, pages = new_document(client)
    set_policy("allowed")
    try:
        reply = mcp_call(client, "run_command", {"id": "layout.pages.insert", "params": {"count": 2}})
    finally:
        set_policy("ask")
    assert not reply["result"].get("isError"), reply
    assert docs_and_pages(client) == (docs, pages + 2)


def test_routed_commands_still_open_new_documents(client):
    docs, _ = new_document(client)
    r = client.post("/api/commands/file.new", json={"params": {"preset": "A4", "pages": 3}}, headers={"origin": own_origin()})
    assert r.status_code == 200, r.text
    assert docs_and_pages(client) == (docs + 1, 3)


def test_save_into_a_folder_that_cannot_be_made_answers_422(client, tmp_path):
    new_document(client)
    blocker = tmp_path / "a-file"
    blocker.write_text("not a folder", encoding="utf-8")
    r = client.post("/api/documents/save", json={"path": str(blocker / "sub" / "layout.fdes")})
    assert r.status_code == 422, r.text
    assert "cannot save" in r.json()["detail"]


def test_mcp_tool_name_that_is_not_a_string_is_a_jsonrpc_error(client):
    for name in (["outline"], {"n": 1}, 7, None):
        reply = mcp_call(client, name, {})
        assert reply["error"]["code"] == -32602, reply


def test_mcp_arguments_that_are_not_an_object_are_a_jsonrpc_error(client):
    reply = mcp_call(client, "outline", ["page", 1])
    assert reply["error"]["code"] == -32602, reply
    assert "arguments" in reply["error"]["message"]


def export_png(client, page):
    return client.post("/api/documents/export", json={"format": "png", "options": {"page": page, "scale": 0.25}})


def core_png(client, index):
    return client.portal.call(lambda: core.call("page.png", {"page": index, "scale": 0.25, "bleed": False, "format": "png"})).bin


def test_png_export_counts_pages_from_1_like_svg(client):
    assert client.post("/api/templates/magazine-longread-swiss/new", json={}).status_code == 200
    assert client.portal.call(core.inspect)["pageCount"] >= 2
    first, second = export_png(client, 1), export_png(client, 2)
    assert first.status_code == 200 and second.status_code == 200, (first.text[:300], second.text[:300])
    assert first.content == core_png(client, 0)
    assert second.content == core_png(client, 1)
    assert first.content != second.content
    zero = export_png(client, 0)
    assert zero.status_code == 422 and "1-based" in zero.json()["detail"]
    assert export_png(client, "two").status_code == 422
    svg = client.post("/api/documents/export", json={"format": "svg", "options": {"page": 0}})
    assert svg.status_code == 422 and "1-based" in svg.json()["detail"]


def test_mcp_export_description_says_pages_count_from_1():
    from app.routers.mcp import TOOLS
    desc = next(t["description"] for t in TOOLS if t["name"] == "export_document")
    assert "options.page counts from 1" in desc and "from 0" not in desc


def config_of(argv):
    i = argv.index("--mcp-config") if "--mcp-config" in argv else -1
    return argv[i + 1] if i >= 0 else None


@pytest.mark.parametrize("launch", ["terminal", "chat"])
def test_in_app_token_is_never_on_the_claude_command_line(launch, monkeypatch):
    from app.services import claude_chat, claude_code
    monkeypatch.setattr(claude_code, "wrap", lambda cmd: cmd)
    argv = claude_code.argv("/usr/bin/claude", "hello") if launch == "terminal" else claude_chat._argv("/usr/bin/claude", "hello", None)
    token = inapp_token()
    assert not any(token in a for a in argv), "the token is readable by any process that lists command lines"
    path = Path(config_of(argv))
    assert path.is_file()
    servers = json.loads(path.read_text(encoding="utf-8"))["mcpServers"]
    assert servers["formdesign"]["headers"][INAPP_HEADER] == token
    if os.name != "nt":
        assert stat.S_IMODE(path.stat().st_mode) == 0o600
