import threading

import pytest

from conftest import CHANGING_TOOL, ENGINE_PORT, MCP_PATH, WINDOW_ORIGINS, own_origin, set_policy

WINDOW = {"origin": WINDOW_ORIGINS[0]}
LOOPBACK_HOST = {"host": f"127.0.0.1:{ENGINE_PORT}"}


def call(client, name, args, agent="family-test-agent"):
    init = {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": agent, "version": "1"}}}
    sid = client.post(MCP_PATH, json=init).headers.get("mcp-session-id")
    body = {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": name, "arguments": args}}
    return client.post(MCP_PATH, json=body, headers={"mcp-session-id": sid}).json()["result"]


def text_of(result):
    return " ".join(c.get("text", "") for c in result.get("content", []))


@pytest.fixture(autouse=True)
def policy_reset(client):
    set_policy("ask")
    yield
    set_policy("ask")
    client.post("/api/agents/enabled", json={"enabled": True}, headers=WINDOW)


@pytest.mark.parametrize("path,body", [
    ("/api/agents/policy", {"client": "family-test-agent", "policy": "allow"}),
    ("/api/agents/enabled", {"enabled": True}),
    ("/api/agents/approvals/0123456789ab", {"approve": True}),
    ("/api/agents/install", {"client": "claude-code", "dry": True}),
    ("/api/mcp/settings", {"enabled": True}),
])
def test_an_outside_agent_cannot_change_its_own_access(client, path, body):
    assert client.post(path, json=body).status_code == 403


def test_a_refused_policy_change_leaves_the_policy_alone(client):
    client.post("/api/agents/policy", json={"client": "family-test-agent", "policy": "allow"})
    assert "No FormDesign window is open" in text_of(call(client, *CHANGING_TOOL))


@pytest.mark.parametrize("origin", [WINDOW_ORIGINS[0], WINDOW_ORIGINS[-1], own_origin()])
def test_the_window_changes_a_clients_policy(client, origin):
    r = client.post("/api/agents/policy", json={"client": "family-test-agent", "policy": "allow"}, headers={"origin": origin})
    assert r.status_code == 200
    assert "No FormDesign window" not in text_of(call(client, *CHANGING_TOOL))


def test_the_prompt_hook_still_splits_undo_steps_without_an_origin(client):
    assert client.post("/api/agents/turn").status_code == 200


def test_reading_the_agent_list_needs_no_origin(client):
    assert client.get("/api/agents").status_code == 200


def test_turning_agents_off_refuses_every_tool(client):
    assert client.post("/api/agents/enabled", json={"enabled": False}, headers=WINDOW).status_code == 200
    result = call(client, "list_templates", {})
    assert result.get("isError") and "turned off" in text_of(result)


def test_a_denied_client_is_refused_and_others_are_not(client):
    set_policy("blocked")
    assert "set to Deny" in text_of(call(client, "list_templates", {}))
    assert not call(client, "list_templates", {}, agent="another-agent").get("isError")


def test_an_ungated_tool_needs_no_approval(client):
    result = call(client, "list_templates", {})
    assert not result.get("isError")


def ask_with_window(client, approve):
    out = {}
    with client.websocket_connect("/api/ws", headers={**LOOPBACK_HOST, **WINDOW}) as ws:
        worker = threading.Thread(target=lambda: out.setdefault("result", call(client, *CHANGING_TOOL)))
        worker.start()
        aid = None
        for _ in range(200):
            msg = ws.receive_json()
            if msg.get("type") == "agents" and msg.get("what") == "approval":
                aid = msg["id"]
                break
        assert aid, "the window was asked"
        assert client.post(f"/api/agents/approvals/{aid}", json={"approve": True}).status_code == 403, "an outside agent cannot answer"
        assert client.post(f"/api/agents/approvals/{aid}", json={"approve": approve}, headers=WINDOW).status_code == 200
        worker.join(30)
    return out["result"]


def test_with_a_window_open_an_approved_call_goes_through(client):
    result = ask_with_window(client, True)
    assert "did not approve" not in text_of(result) and "No FormDesign window" not in text_of(result)


def test_with_a_window_open_a_declined_call_is_refused(client):
    result = ask_with_window(client, False)
    assert result.get("isError") and "did not approve" in text_of(result)
