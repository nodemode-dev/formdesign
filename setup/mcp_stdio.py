#!/usr/bin/env python3
"""FormDesign over MCP's stdio transport, for clients that launch a server as a program (Claude Desktop, Codex, Cursor).
Each line from the client, a JSON-RPC message, goes to the running engine's /mcp; each reply comes back as one line.
Notifications get no reply. Only protocol messages go to stdout; anything else goes to stderr, as stdio requires.
The engine's address: FORMDESIGN_MCP_URL, or http://127.0.0.1:8196/mcp. With the default address an engine that is not
running is started once (FORMDESIGN_MCP_AUTOSTART=0 turns that off)."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

URL = os.environ.get("FORMDESIGN_MCP_URL", "http://127.0.0.1:8196/mcp")
_started = False
_session = ""


def forward(line: str) -> str | None:
    try:
        msg = json.loads(line)
    except ValueError:
        return json.dumps({"jsonrpc": "2.0", "id": None, "error": {"code": -32700, "message": "Parse error"}})
    params = msg.get("params") or {}
    version = (params.get("_meta") or {}).get("io.modelcontextprotocol/protocolVersion") or params.get("protocolVersion") or "2025-11-25"
    global _session
    headers = {"Content-Type": "application/json", "Accept": "application/json", "MCP-Protocol-Version": version, "X-FormDesign-Transport": "stdio"}
    if _session:
        headers["Mcp-Session-Id"] = _session
    req = urllib.request.Request(URL, data=line.encode(), method="POST", headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=660) as r:
            body = r.read().decode()
            _session = r.headers.get("Mcp-Session-Id") or _session
    except urllib.error.HTTPError as exc:
        body = exc.read().decode() or ""
    except (urllib.error.URLError, OSError) as exc:
        print(f"[formdesign mcp] the engine is not answering at {URL}: {exc}", file=sys.stderr, flush=True)
        if start_engine():
            return forward(line)
        if msg.get("id") is None:
            return None
        return json.dumps({"jsonrpc": "2.0", "id": msg.get("id"), "error": {"code": -32000,
                           "message": "The FormDesign engine is not running and could not be started: run ./formdesign boot in the formdesign folder, then retry."}})
    if msg.get("id") is None:
        return None
    return body.strip() or None


def start_engine() -> bool:
    global _started
    if _started or os.environ.get("FORMDESIGN_MCP_AUTOSTART", "1") == "0" or os.environ.get("FORMDESIGN_MCP_URL"):
        return False
    _started = True
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    built = os.path.isfile(os.path.join(root, "frontend", "dist", "index.html"))
    launcher = os.path.join(root, "formdesign.bat" if os.name == "nt" else "formdesign")
    log = open(os.path.join(tempfile.gettempdir(), "formdesign-engine.log"), "ab")
    print(f"[formdesign mcp] starting the engine ({'serve' if built else 'backend'}); its log: {log.name}", file=sys.stderr, flush=True)
    kw = {"creationflags": 0x00000200} if os.name == "nt" else {"start_new_session": True}
    subprocess.Popen([launcher, "serve" if built else "backend"], cwd=root, stdin=subprocess.DEVNULL, stdout=log, stderr=log, **kw)
    for _ in range(120):
        time.sleep(0.5)
        try:
            with urllib.request.urlopen(URL.rsplit("/mcp", 1)[0] + "/api/version", timeout=2) as r:
                if r.status == 200:
                    return True
        except (urllib.error.URLError, OSError):
            continue
    return False


def main() -> None:
    for line in sys.stdin:
        if not line.strip():
            continue
        out = forward(line)
        if out:
            sys.stdout.write(out + "\n")
            sys.stdout.flush()


if __name__ == "__main__":
    main()
