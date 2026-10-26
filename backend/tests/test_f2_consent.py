import pytest

from conftest import CHANGING_TOOL, INAPP_HEADER, MCP_PATH, READ_TOOL, consent_refusals, inapp_token, set_policy


def rpc(client, method, params=None, headers=None, rid=1):
    r = client.post(MCP_PATH, json={"jsonrpc": "2.0", "id": rid, "method": method, "params": params or {}}, headers=headers or {})
    assert r.status_code == 200, r.text
    return r.json()


def session(client, headers=None):
    r = client.post(MCP_PATH, json={"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "family-test-agent", "version": "1"}}}, headers=headers or {})
    assert r.status_code == 200, r.text
    sid = r.headers.get("mcp-session-id")
    out = dict(headers or {})
    if sid:
        out["mcp-session-id"] = sid
    return out


def call(client, tool, headers=None):
    name, args = tool
    hdrs = session(client, headers)
    return rpc(client, "tools/call", {"name": name, "arguments": args}, hdrs, rid=2)


def text_of(reply):
    result = reply.get("result") or {}
    return " ".join(c.get("text", "") for c in result.get("content", []) if isinstance(c, dict))


def refused(reply, kind):
    return consent_refusals()[kind] in text_of(reply)


@pytest.fixture(autouse=True)
def policy_reset():
    set_policy("ask")
    yield
    set_policy("ask")


def test_read_only_tool_never_asks(client):
    reply = call(client, READ_TOOL)
    assert "error" not in reply
    assert not refused(reply, "no_window")
    assert not reply["result"].get("isError")


def test_changing_tool_with_no_window_is_refused(client):
    reply = call(client, CHANGING_TOOL)
    assert refused(reply, "no_window")
    assert reply["result"].get("isError")


def test_blocked_policy_refuses_a_changing_tool(client):
    set_policy("blocked")
    reply = call(client, CHANGING_TOOL)
    assert refused(reply, "blocked") or refused(reply, "no_window")
    assert reply["result"].get("isError")


def test_blocked_policy_still_refuses_with_the_inapp_token_missing(client):
    set_policy("blocked")
    assert call(client, CHANGING_TOOL, {INAPP_HEADER: "wrong-token"})["result"].get("isError")


def test_allowed_policy_lets_a_changing_tool_through(client):
    set_policy("allowed")
    reply = call(client, CHANGING_TOOL)
    assert not refused(reply, "no_window")
    assert not refused(reply, "blocked")


def test_inapp_token_skips_the_ask(client):
    reply = call(client, CHANGING_TOOL, {INAPP_HEADER: inapp_token()})
    assert not refused(reply, "no_window")


@pytest.mark.parametrize("token", ["", "wrong-token", "x" * 64])
def test_wrong_inapp_token_does_not_skip_the_ask(client, token):
    assert refused(call(client, CHANGING_TOOL, {INAPP_HEADER: token}), "no_window")
