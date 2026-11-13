#!/usr/bin/env python3
"""FormDesign over MCP's stdio transport, for clients that launch a server as a program (Claude Desktop, Codex, Cursor).
Each line from the client, a JSON-RPC message, goes to the running engine's /mcp; each reply comes back as one line.
Notifications get no reply. Only protocol messages go to stdout; anything else goes to stderr, as stdio requires.
The engine's address: FORMDESIGN_MCP_URL, or http://127.0.0.1:8196/mcp. With the default address an engine that is not
running is started once (FORMDESIGN_MCP_AUTOSTART=0 turns that off).

  ./formdesign mcp --attach   never start the engine: with none running only ping is offered, and the client is told
                              (notifications/tools/list_changed) once the engine answers. FORMDESIGN_MCP_ATTACH=1 does the same.
                              The desktop extension (./formdesign mcpb) runs this way."""

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
ATTACH = os.environ.get("FORMDESIGN_MCP_ATTACH", "0") == "1"
_down = False
_changed = False
OFFLINE = "FormDesign is not running. Start it with ./formdesign boot (or open the FormDesign app), then ask again."
PING_ONLY = [{"name": "ping", "description": "Reports whether FormDesign is running. It is not running now; start it with ./formdesign boot, and the other tools appear.",
              "inputSchema": {"type": "object", "properties": {}, "required": [], "additionalProperties": False},
              "annotations": {"readOnlyHint": True, "destructiveHint": False, "idempotentHint": True, "openWorldHint": False}}]


def forward(line: str) -> str | None:
    try:
        msg = json.loads(line)
    except ValueError:
        return json.dumps({"jsonrpc": "2.0", "id": None, "error": {"code": -32700, "message": "Parse error"}})
    params = msg.get("params") or {}
    version = (params.get("_meta") or {}).get("io.modelcontextprotocol/protocolVersion") or params.get("protocolVersion") or "2025-11-25"
    global _session, _down, _changed
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
        if ATTACH:
            return offline(msg, version)
        if start_engine():
            return forward(line)
        if msg.get("id") is None:
            return None
        return json.dumps({"jsonrpc": "2.0", "id": msg.get("id"), "error": {"code": -32000,
                           "message": "The FormDesign engine is not running and could not be started: run ./formdesign boot in the formdesign folder, then retry."}})
    if _down:
        _down, _changed = False, True
    if msg.get("id") is None:
        return None
    return body.strip() or None


def offline(msg: dict, version: str) -> str | None:
    global _down
    _down = True
    rid, method, params = msg.get("id"), msg.get("method"), msg.get("params") or {}
    if rid is None:
        return None
    done = {"resultType": "complete"} if version == "2026-07-28" else {}
    server = {"name": "formdesign", "version": "not running"}
    caps = {"tools": {"listChanged": True}, "prompts": {}}
    if method == "server/discover":
        result = {"supportedVersions": ["2026-07-28", "2025-11-25"], "capabilities": caps, "_meta": {"io.modelcontextprotocol/serverInfo": server}, "instructions": OFFLINE}
    elif method == "initialize":
        result = {"protocolVersion": params.get("protocolVersion") or "2025-11-25", "capabilities": caps, "serverInfo": server, "instructions": OFFLINE}
    elif method == "tools/list":
        result = {"tools": PING_ONLY}
    elif method == "tools/call":
        result = {"content": [{"type": "text", "text": OFFLINE}], "isError": params.get("name") != "ping"}
    elif method in ("ping", "prompts/list", "resources/list"):
        result = {"prompts": []} if method == "prompts/list" else {"resources": []} if method == "resources/list" else {}
    else:
        return json.dumps({"jsonrpc": "2.0", "id": rid, "error": {"code": -32601, "message": f"Method not found: {method}"}})
    return json.dumps({"jsonrpc": "2.0", "id": rid, "result": {**result, **done}})


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
    global ATTACH, _changed
    for flag in sys.argv[1:]:
        if flag != "--attach":
            print(f"{flag}: not an option. ./formdesign mcp takes --attach.", file=sys.stderr)
            sys.exit(2)
        ATTACH = True
    for line in sys.stdin:
        if not line.strip():
            continue
        out = forward(line)
        if _changed:
            _changed = False
            sys.stdout.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/tools/list_changed"}) + "\n")
            sys.stdout.flush()
        if out:
            sys.stdout.write(out + "\n")
            sys.stdout.flush()


if __name__ == "__main__":
    main()
