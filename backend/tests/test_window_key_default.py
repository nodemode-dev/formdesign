import re

import pytest

READS = {"GET", "HEAD", "OPTIONS"}
OPEN = {
    ("POST", "/mcp"),
    ("POST", "/api/window/key"),
    ("POST", "/api/agents/turn"),
    ("POST", "/api/review/public/{token}/comments"),
    ("POST", "/api/review/public/{token}/comments/{cid}/replies"),
}


def write_operations(app):
    return [(m.upper(), path) for path, item in app.openapi()["paths"].items() for m in item if m.upper() not in READS]


def concrete(path):
    return re.sub(r"\{[^}]+\}", "1", path)


def test_the_open_list_is_short_and_named():
    from app.core import window_key

    assert set(window_key.OPEN) == {"/mcp", "/api/window/key", "/api/agents/turn"}
    assert len(window_key.OPEN_PATTERNS) == 1


def test_every_write_route_needs_the_window_key(app, bare):
    ops = write_operations(app)
    assert len(ops) > 100
    answered = []
    for method, path in ops:
        if (method, path) in OPEN:
            continue
        for headers in ({}, {"x-formdesign-window-key": "x" * 43}):
            r = bare.request(method, concrete(path), json={}, headers=headers)
            if r.status_code != 403 or r.json() != {"detail": "Only the formdesign window can do this"}:
                answered.append(f"{method} {path} {r.status_code}")
    assert not answered, "write routes that answer without the window key (add the key, or name the route in window_key.OPEN with its caller): " + ", ".join(answered)


def test_every_open_write_route_is_on_the_list(app):
    from app.core import window_key

    open_now = {(m, p) for m, p in write_operations(app) if window_key.is_open(concrete(p))}
    assert open_now == OPEN & {(m, p) for m, p in write_operations(app)}


@pytest.mark.parametrize("path", ["/api/term", "/api/documents/new", "/api/claude/prefs", "/api/agents/policy", "/api/agents/install", "/api/core/exec", "/api/commands/file.package"])
def test_named_write_routes_refuse_without_the_key(bare, path):
    assert bare.post(path, json={}).status_code == 403


def test_open_routes_still_answer_without_the_key(bare):
    r = bare.post("/mcp", json={"jsonrpc": "2.0", "id": 1, "method": "ping", "params": {}}, headers={"mcp-protocol-version": "2025-11-25"})
    assert r.status_code == 200
    assert bare.post("/api/agents/turn").status_code == 200


def test_the_window_key_lets_a_write_through(client):
    assert client.post("/api/agents/turn").status_code == 200
    assert client.post("/api/documents/new", json={}).status_code != 403
