import hmac
import json
import os
import re
import secrets
import time

from fastapi import HTTPException, Request

HEADER = "x-formdesign-window-key"
STDOUT_ENV = "FORMDESIGN_WINDOW_KEY_STDOUT"
LINE = "formdesign-window-key: "
REFUSED = "Only the formdesign window can do this"
TICKET_LIFE = 30.0
READS = ("GET", "HEAD", "OPTIONS")
OPEN = ("/mcp", "/api/window/key", "/api/agents/turn")
OPEN_PATTERNS = (re.compile(r"^/api/review/public/[^/]+/comments(/[0-9]+/replies)?$"),)
KEY = secrets.token_urlsafe(32)
STATE = {"claimed": False, "secondClaim": False}
TICKETS: dict[str, float] = {}


def stdout_mode() -> bool:
    return os.environ.get(STDOUT_ENV) == "1"


def valid(value) -> bool:
    return isinstance(value, str) and bool(value) and hmac.compare_digest(value.encode("utf-8"), KEY.encode("utf-8"))


def require(request: Request) -> None:
    if not valid(request.headers.get(HEADER)):
        raise HTTPException(403, REFUSED)


def is_open(path: str) -> bool:
    return path in OPEN or any(p.match(path) for p in OPEN_PATTERNS)


def claim() -> str | None:
    if STATE["claimed"]:
        STATE["secondClaim"] = True
        return None
    STATE["claimed"] = True
    return KEY


def ticket() -> str:
    now = time.monotonic()
    for old in [t for t, at in TICKETS.items() if now - at > TICKET_LIFE]:
        TICKETS.pop(old, None)
    value = secrets.token_urlsafe(24)
    TICKETS[value] = now
    return value


def spend(value) -> bool:
    if not isinstance(value, str) or not value:
        return False
    at = TICKETS.pop(value, None)
    return at is not None and time.monotonic() - at <= TICKET_LIFE


class WindowKeyGuard:
    def __init__(self, app) -> None:
        self.app = app

    async def __call__(self, scope, receive, send) -> None:
        if scope["type"] != "http" or scope.get("method") in READS or is_open(str(scope.get("path", ""))):
            await self.app(scope, receive, send)
            return
        given = next((v.decode("latin-1") for k, v in scope.get("headers", []) if k.decode("latin-1").lower() == HEADER), None)
        if valid(given):
            await self.app(scope, receive, send)
            return
        body = json.dumps({"detail": REFUSED}).encode("utf-8")
        await send({"type": "http.response.start", "status": 403, "headers": [(b"content-type", b"application/json"), (b"content-length", str(len(body)).encode())]})
        await send({"type": "http.response.body", "body": body})
