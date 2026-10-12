import json
import os
import sys
import urllib.error
import urllib.request

ENGINE = os.environ.get("FORMDES_ENGINE_URL", "http://127.0.0.1:8196").rstrip("/")


def _post(path: str, body: dict) -> dict:
    req = urllib.request.Request(ENGINE + path, data=json.dumps(body).encode(), method="POST",
                                 headers={"content-type": "application/json", "accept": "application/json, text/event-stream"})
    try:
        with urllib.request.urlopen(req, timeout=600) as r:
            return json.loads(r.read() or b"{}")
    except urllib.error.HTTPError as e:
        detail = e.read().decode("utf-8", "replace")
        try:
            detail = json.loads(detail).get("detail", detail)
        except ValueError:
            pass
        raise SystemExit(f"formdes: {detail}")
    except urllib.error.URLError:
        raise SystemExit(f"formdes: no engine at {ENGINE}. Start one with ./formdes or ./formdes backend.")


def _value(raw: str):
    try:
        return json.loads(raw)
    except ValueError:
        return raw


def params_of(pairs: list[str], extra: str | None) -> dict:
    out = json.loads(extra) if extra else {}
    for p in pairs:
        if "=" not in p:
            raise SystemExit(f"formdes: `{p}` is not key=value")
        k, v = p.split("=", 1)
        out[k] = _value(v)
    return out


def cmd_run(a) -> int:
    r = _post("/api/core/exec", {"command": a.id, "params": params_of(a.pairs, a.params)})
    res = r.get("result", r)
    print(json.dumps(res, indent=1, ensure_ascii=False) if a.json or not isinstance(res, str) else res)
    return 0


def cmd_commands(a) -> int:
    r = _post("/api/core/commands", {})
    items = r.get("result", r)
    items = items.get("commands", items) if isinstance(items, dict) else items
    q = (a.q or "").lower()
    rows = [c for c in items if not q or q in c["id"].lower() or q in str(c.get("label", "")).lower() or q in " ".join(c.get("menu") or []).lower()]
    if a.json:
        print(json.dumps(rows, indent=1, ensure_ascii=False))
        return 0
    for c in rows:
        menu = " > ".join(c.get("menu") or [])
        print(f"{c['id']:<34} {c.get('label', ''):<36} {menu}  {c.get('params') or ''}".rstrip())
    print(f"{len(rows)} of {len(items)} commands", file=sys.stderr)
    return 0


def _rpc(method: str, params: dict) -> dict:
    r = _post("/mcp", {"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
    if "error" in r:
        raise SystemExit(f"formdes: {r['error'].get('message')}")
    return r.get("result", {})


def cmd_tools(a) -> int:
    tools = _rpc("tools/list", {}).get("tools", [])
    if a.json:
        print(json.dumps(tools, indent=1, ensure_ascii=False))
        return 0
    for t in tools:
        print(f"{t['name']:<20} {(t.get('description') or '').split('. ')[0]}")
    return 0


def cmd_tool(a) -> int:
    args = json.loads(a.args) if a.args else {}
    res = _rpc("tools/call", {"name": a.name, "arguments": args})
    if a.json:
        print(json.dumps(res, indent=1, ensure_ascii=False))
    else:
        for c in res.get("content", []):
            print(c.get("text") if c.get("type") == "text" else f"[{c.get('type')} {c.get('mimeType', '')}]")
    return 1 if res.get("isError") else 0


def add_parsers(sub) -> dict:
    c = sub.add_parser("cmd", help="run one engine command on the running engine's open document, e.g. formdes cmd layout.pages.add count=2")
    c.add_argument("id")
    c.add_argument("pairs", nargs="*", help="key=value parameters; values are read as JSON when they parse")
    c.add_argument("--params", help="parameters as one JSON object")
    c.add_argument("--json", action="store_true")
    l = sub.add_parser("commands", help="list the engine's commands with their menus and parameters")
    l.add_argument("--q", help="filter by id, label or menu")
    l.add_argument("--json", action="store_true")
    t = sub.add_parser("tools", help="list the MCP tools agents get")
    t.add_argument("--json", action="store_true")
    o = sub.add_parser("tool", help="call one MCP tool on the running engine, e.g. formdes tool preview '{\"page\": 1}'")
    o.add_argument("name")
    o.add_argument("args", nargs="?")
    o.add_argument("--json", action="store_true")
    return {"cmd": cmd_run, "commands": cmd_commands, "tools": cmd_tools, "tool": cmd_tool}
