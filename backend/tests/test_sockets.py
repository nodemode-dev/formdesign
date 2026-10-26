import pytest
from starlette.websockets import WebSocketDisconnect

from conftest import ENGINE_PORT, WINDOW_ORIGINS, own_origin

LOOPBACK_HOST = {"host": f"127.0.0.1:{ENGINE_PORT}"}
FOREIGN = ["http://evil.example", "http://127.0.0.1:3099", "http://localhost:3099", "null"]


def close_code(client, path, headers):
    try:
        with client.websocket_connect(path, headers=headers) as ws:
            msg = ws.receive()
    except WebSocketDisconnect as e:
        return e.code
    return msg.get("code") if msg.get("type") == "websocket.close" else None


@pytest.mark.parametrize("origin", FOREIGN)
def test_live_socket_refuses_another_origin(client, origin):
    assert close_code(client, "/api/ws", {**LOOPBACK_HOST, "origin": origin}) == 4403


@pytest.mark.parametrize("origin", [own_origin(), *WINDOW_ORIGINS])
def test_live_socket_accepts_the_window(client, origin):
    with client.websocket_connect("/api/ws", headers={**LOOPBACK_HOST, "origin": origin}) as ws:
        assert ws.receive_json()


@pytest.mark.parametrize("origin", FOREIGN)
def test_terminal_socket_refuses_another_origin(client, origin):
    assert close_code(client, "/api/term/none/ws?token=x", {**LOOPBACK_HOST, "origin": origin}) == 4403


def test_terminal_socket_without_the_token_is_refused(client):
    assert close_code(client, "/api/term/none/ws?token=wrong", {**LOOPBACK_HOST, "origin": WINDOW_ORIGINS[0]}) == 4403


def test_terminal_socket_with_the_token_reaches_the_session_lookup(client):
    token = client.get("/api/term", headers={"origin": WINDOW_ORIGINS[0]}).json()["token"]
    assert close_code(client, f"/api/term/none/ws?token={token}", {**LOOPBACK_HOST, "origin": WINDOW_ORIGINS[0]}) == 4404
