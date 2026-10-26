import pytest
from starlette.websockets import WebSocketDisconnect

from conftest import ENGINE_PORT, HEALTH_PATH, WINDOW_ORIGINS, WS_PATH, own_origin

FOREIGN_ORIGINS = [
    "http://evil.example",
    "https://evil.example",
    f"http://localhost.evil.com:{ENGINE_PORT}",
    f"http://127.0.0.1.evil.com:{ENGINE_PORT}",
    "http://localhost:3099",
    "http://127.0.0.1:3099",
    "null",
    "file://",
]

FOREIGN_HOSTS = ["evil.example", f"evil.example:{ENGINE_PORT}", f"localhost.evil.com:{ENGINE_PORT}", f"192.168.1.10:{ENGINE_PORT}"]


def test_foreign_origins_are_not_in_the_apps_own_list():
    assert not set(FOREIGN_ORIGINS) & set(WINDOW_ORIGINS)


@pytest.mark.parametrize("host", FOREIGN_HOSTS)
def test_foreign_host_is_refused(client, host):
    assert client.get(HEALTH_PATH, headers={"host": host}).status_code == 403


@pytest.mark.parametrize("origin", FOREIGN_ORIGINS)
def test_foreign_origin_is_refused(client, origin):
    assert client.get(HEALTH_PATH, headers={"origin": origin}).status_code == 403


@pytest.mark.parametrize("origin", FOREIGN_ORIGINS)
def test_foreign_origin_cannot_post(client, origin):
    assert client.post(HEALTH_PATH, headers={"origin": origin}).status_code == 403


def test_cross_site_request_without_origin_is_refused(client):
    assert client.get(HEALTH_PATH, headers={"sec-fetch-site": "cross-site"}).status_code == 403


@pytest.mark.parametrize("value", ["CROSS-SITE", " cross-site "])
def test_cross_site_marker_is_read_loosely(client, value):
    assert client.get(HEALTH_PATH, headers={"sec-fetch-site": value}).status_code == 403


@pytest.mark.parametrize("origin", [own_origin(), *WINDOW_ORIGINS])
def test_allowed_origin_passes(client, origin):
    assert client.get(HEALTH_PATH, headers={"origin": origin}).status_code == 200


@pytest.mark.parametrize("host", [f"localhost:{ENGINE_PORT}", f"[::1]:{ENGINE_PORT}"])
def test_own_origin_under_another_loopback_name_passes(client, host):
    assert client.get(HEALTH_PATH, headers={"host": host, "origin": f"http://{host}"}).status_code == 200


def test_own_origin_must_match_the_host_it_was_sent_to(client):
    assert client.get(HEALTH_PATH, headers={"host": f"127.0.0.1:{ENGINE_PORT}", "origin": f"http://127.0.0.1:{ENGINE_PORT + 1}"}).status_code == 403


@pytest.mark.parametrize("site", ["same-origin", "same-site", "none"])
def test_browser_request_without_origin_passes_unless_cross_site(client, site):
    assert client.get(HEALTH_PATH, headers={"sec-fetch-site": site}).status_code == 200


def test_plain_client_without_origin_passes(client):
    assert client.get(HEALTH_PATH).status_code == 200


@pytest.mark.parametrize("host", [f"127.0.0.1:{ENGINE_PORT}", f"localhost:{ENGINE_PORT}", f"[::1]:{ENGINE_PORT}"])
def test_loopback_hosts_pass(client, host):
    assert client.get(HEALTH_PATH, headers={"host": host}).status_code == 200


@pytest.mark.skipif(WS_PATH is None, reason="this app has no websocket")
@pytest.mark.parametrize("headers", [{"host": "evil.example"}, {"origin": "http://evil.example"}, {"origin": "null"}])
def test_foreign_websocket_closes_4403(client, headers):
    with pytest.raises(WebSocketDisconnect) as refused:
        with client.websocket_connect(WS_PATH, headers=headers) as ws:
            ws.receive()
    assert refused.value.code == 4403
