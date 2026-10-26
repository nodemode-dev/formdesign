import json
import os
import subprocess

from conftest import APP_ID, ENV_PREFIX, MCP_PATH, READ_TOOL, STDIO_CMD, tool_catalogue


def initialize(client, headers=None):
    body = {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "family-test", "version": "1"}}}
    return client.post(MCP_PATH, json=body, headers=headers or {})


def test_initialize_without_origin_answers(client):
    r = initialize(client)
    assert r.status_code == 200
    result = r.json()["result"]
    assert result["protocolVersion"]
    assert result["serverInfo"]["name"]
    assert "tools" in result["capabilities"]


def test_initialize_from_a_foreign_origin_is_refused(client):
    assert initialize(client, {"origin": "http://evil.example"}).status_code == 403


def test_tools_list_matches_the_catalogue(client):
    r = initialize(client)
    sid = r.headers.get("mcp-session-id")
    headers = {"mcp-session-id": sid} if sid else {}
    listed = client.post(MCP_PATH, json={"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}, headers=headers).json()["result"]["tools"]
    catalogue = tool_catalogue()
    assert len(listed) == len(catalogue)
    assert sorted(t["name"] for t in listed) == sorted(t["name"] for t in catalogue)
    for tool in listed:
        assert tool["description"].strip()
        assert tool["inputSchema"]["type"] == "object"


def test_read_only_tool_returns_a_valid_result(client):
    r = initialize(client)
    sid = r.headers.get("mcp-session-id")
    headers = {"mcp-session-id": sid} if sid else {}
    name, args = READ_TOOL
    reply = client.post(MCP_PATH, json={"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": name, "arguments": args}}, headers=headers).json()
    result = reply["result"]
    assert not result.get("isError")
    assert result["content"] and all(c["type"] in ("text", "image", "resource", "resource_link") for c in result["content"])


def test_unknown_method_is_a_jsonrpc_error(client):
    reply = client.post(MCP_PATH, json={"jsonrpc": "2.0", "id": 4, "method": "no/such/method", "params": {}}).json()
    assert reply["error"]["code"] == -32601


def test_stdio_relay_reaches_the_running_engine(engine):
    env = dict(engine["env"])
    env.pop(f"{ENV_PREFIX}_PORT", None)
    env[f"{ENV_PREFIX}_MCP_URL"] = f"{engine['base']}{MCP_PATH}"
    env[f"{ENV_PREFIX}_MCP_AUTOSTART"] = "0"
    lines = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "family-test", "version": "1"}}},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
    ]
    proc = subprocess.run(STDIO_CMD, input="".join(json.dumps(m) + "\n" for m in lines), capture_output=True, text=True, timeout=60, env=env, cwd=os.path.dirname(STDIO_CMD[-1]) if len(STDIO_CMD) > 1 else None)
    replies = {m.get("id"): m for m in (json.loads(l) for l in proc.stdout.splitlines() if l.strip().startswith("{"))}
    assert 2 in replies, f"{APP_ID} relay gave no tools/list reply: {proc.stdout[-2000:]} {proc.stderr[-2000:]}"
    assert len(replies[2]["result"]["tools"]) == len(tool_catalogue())
