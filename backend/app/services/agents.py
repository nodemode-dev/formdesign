import asyncio
import json
import secrets
import subprocess
import sys
import time
from collections import deque
from pathlib import Path

from app.core import config, seal
from app.engine.core import core

SETTINGS = config.DATA_DIR / "agents.json"
POLICIES = ("allow", "ask", "deny")
APPROVAL_WAIT = 300.0
KNOWN = {"claude-code": "Claude Code", "claude": "Claude Code", "codex": "Codex", "codex-mcp-client": "Codex", "cursor": "Cursor", "cursor-vscode": "Cursor"}
GATED_COMMANDS = {"file.package": "packages the document to disk", "file.print": "prints", "file.printBooklet": "prints a booklet"}
GATED_TOOLS = ("publish_website", "review", "save_document", "export_document", "book", "run_command", "run_commands")
INAPP_HEADER = "x-formdesign-inapp"
INAPP_CLIENT = "Claude Code in FormDesign"

sessions: dict[str, dict] = {}
clients: dict[str, dict] = {}
activity: deque = deque(maxlen=200)
approvals: dict[str, dict] = {}
_seq = 0

# One undo step per agent turn. A turn is an agent's run of tool calls: it ends when the person's own Claude Code
# sends a new prompt (the repo's UserPromptSubmit hook posts /api/agents/turn) or after TURN_IDLE seconds without a
# call from that agent, the fallback for clients without a hook. The core keeps a turn's changes in one undo entry
# only while that entry is on top, so a person's edit in between always stays its own step.
TURN_IDLE = 120.0
_turns: dict[str, list] = {}


def turn_key(who: str) -> str:
    """The turn key for a call from `who` (an MCP session id, or the client's name without one)."""
    now = time.monotonic()
    t = _turns.setdefault(who, [0, 0.0])
    if now - t[1] > TURN_IDLE:
        t[0] += 1
    t[1] = now
    return f"{who}#{t[0]}"


def end_turns(who: str | None = None) -> int:
    """Ends `who`'s open turn, or every open turn, so the next change starts a new undo step. Returns how many were open."""
    picked = [t for k, t in _turns.items() if who is None or k == who]
    n = sum(1 for t in picked if t[1])
    for t in picked:
        t[1] = 0.0
    return n


def _read() -> dict:
    try:
        d = json.loads(SETTINGS.read_text())
        return d if isinstance(d, dict) else {}
    except (OSError, ValueError):
        return {}


def _write(d: dict) -> None:
    SETTINGS.parent.mkdir(parents=True, exist_ok=True)
    SETTINGS.write_text(json.dumps(d, indent=1))


def _loosened(vals: dict) -> bool:
    return any(v == "allow" for k, v in vals["policies"].items() if k != INAPP_CLIENT) or bool(str(vals["claudeBin"] or "").strip())


def choice() -> dict:
    from app.services import claude_code
    d = _read()
    policies = d.get("policies") if isinstance(d.get("policies"), dict) else {}
    vals = {"enabled": bool(d.get("enabled", True)), "policies": dict(policies), "claudeBin": claude_code.raw_prefs().get("claudeBin", "")}
    state = seal.check(vals, d.get("seal"))
    broken = state in ("unsealed", "mismatch") and _loosened(vals)
    if broken:
        vals = {"enabled": vals["enabled"], "policies": {k: ("ask" if v == "allow" and k != INAPP_CLIENT else v) for k, v in vals["policies"].items()}, "claudeBin": ""}
    return {**vals, "seal": "broken" if broken else state, "sealBroken": broken}


def commit(mutate=None, claude_bin: str | None = None) -> str:
    eff = choice()
    d = _read()
    d["enabled"], d["policies"] = eff["enabled"], dict(eff["policies"])
    if mutate is not None:
        mutate(d)
    bin_ = eff["claudeBin"] if claude_bin is None else claude_bin
    from app.services import claude_code
    if claude_code.raw_prefs().get("claudeBin", "") != bin_ and claude_code.PREFS.exists():
        claude_code.write_bin(bin_)
    d.pop("seal", None)
    mark = seal.sign({"enabled": bool(d.get("enabled", True)), "policies": d.get("policies") or {}, "claudeBin": bin_})
    if mark:
        d["seal"] = mark
    _write(d)
    return bin_


def enabled() -> bool:
    return choice()["enabled"]


def set_enabled(on: bool) -> None:
    commit(lambda d: d.__setitem__("enabled", on))
    push("settings")


def policy(client: str) -> str:
    p = choice()["policies"].get(client, "allow" if client == INAPP_CLIENT else "ask")
    return p if p in POLICIES else "ask"


def set_policy(client: str, p: str) -> None:
    if p not in POLICIES:
        raise ValueError(f"policy is one of {', '.join(POLICIES)}")
    commit(lambda d: d.setdefault("policies", {}).__setitem__(client, p))
    push("clients")


def push(what: str, **extra) -> None:
    ev = {"type": "agents", "what": what, **extra}
    for q in list(core.listeners):
        try:
            q.put_nowait(ev)
        except asyncio.QueueFull:
            pass


def view(**req) -> None:
    ev = {"type": "view", **{k: v for k, v in req.items() if v is not None}}
    for q in list(core.listeners):
        try:
            q.put_nowait(ev)
        except asyncio.QueueFull:
            pass


def display(info: dict | None, fallback: str) -> str:
    name = str((info or {}).get("name") or "").strip()
    if not name:
        return fallback
    return KNOWN.get(name.lower(), name)


def identify(headers, params: dict, method: str) -> tuple[str, str | None]:
    """The client making this request, and a new session id when it is initializing."""
    params = params if isinstance(params, dict) else {}
    meta = params.get("_meta") if isinstance(params.get("_meta"), dict) else {}
    info = meta.get("io.modelcontextprotocol/clientInfo")
    info = info if isinstance(info, dict) else None
    transport = headers.get("x-formdesign-transport") or "http"
    sid = headers.get("mcp-session-id")
    new_sid = None
    if method == "initialize":
        info = (params.get("clientInfo") if isinstance(params.get("clientInfo"), dict) else None) or info
        new_sid = secrets.token_hex(12)
        sessions[new_sid] = {"info": info or {}, "transport": transport}
    elif sid and sid in sessions:
        info = info or sessions[sid]["info"]
        transport = sessions[sid]["transport"]
    ua = (headers.get("user-agent") or "").split("/")[0].strip()
    name = INAPP_CLIENT if inapp(headers) else display(info, ua or "Unknown client")
    if not inapp(headers) and (name.strip().lower() in (INAPP_CLIENT.lower(), "assistant") or name.strip().lower().startswith("plugin:")):
        name = f"{name} (unverified)"
    c = clients.setdefault(name, {"name": name, "version": None, "transport": transport, "firstSeen": time.time(), "lastSeen": 0.0, "calls": 0})
    c["lastSeen"] = time.time()
    c["transport"] = transport
    if info and info.get("version"):
        c["version"] = str(info["version"])
    return name, new_sid


def inapp(headers) -> bool:
    from app.services import claude_code
    return claude_code.is_inapp(headers.get(INAPP_HEADER))


def gate_reason(name: str, args: dict) -> str | None:
    if not isinstance(args, dict) or name not in GATED_TOOLS:
        return None
    if name == "publish_website" and args.get("action", "publish") != "list":
        return "publishes a website" if args.get("action", "publish") == "publish" else "takes a published website down"
    if name == "review" and args.get("action") in ("share", "stop"):
        return "shares the document for review" if args.get("action") == "share" else "takes the review site down"
    if name in ("save_document", "export_document") and args.get("path"):
        try:
            if Path(str(args["path"])).expanduser().exists():
                return f"overwrites {args['path']}"
        except (OSError, ValueError):
            return None
    if name == "book" and args.get("action") == "export" and args.get("path"):
        try:
            if Path(str(args["path"])).expanduser().exists():
                return f"overwrites {args['path']}"
        except (OSError, ValueError):
            return None
    if name == "run_command":
        from app.services import commands as C
        cid = str(args.get("id") or "")
        if C.risky(cid) or (C.meta().get(cid) or {}).get("risky"):
            return f"runs {cid}, which Undo cannot reverse"
    if name == "run_commands":
        from app.services import commands as C
        for c in args.get("commands") or []:
            cid = c.get("command") if isinstance(c, dict) else None
            if cid in GATED_COMMANDS:
                return GATED_COMMANDS[cid]
            if isinstance(cid, str) and cid != "layout.pages.delete" and (C.risky(cid) or (C.meta().get(cid) or {}).get("risky")):
                return f"runs {cid}, which Undo cannot reverse"
            if cid == "layout.pages.delete":
                pages = c["params"].get("pages") if isinstance(c.get("params"), dict) else None
                if not isinstance(pages, list) and pages is not None:
                    return "deletes pages"
                if pages and len(pages) > 1:
                    return f"deletes {len(pages)} pages"
    return None


def short(args: dict, limit: int = 160) -> str:
    s = json.dumps(args, ensure_ascii=False, separators=(",", ":"))
    return s if len(s) <= limit else s[: limit - 1] + "…"


async def ask(client: str, name: str, args: dict, reason: str) -> bool:
    aid = secrets.token_hex(6)
    fut = asyncio.get_running_loop().create_future()
    approvals[aid] = {"id": aid, "client": client, "tool": name, "args": short(args, 400), "reason": reason, "at": time.time(), "future": fut}
    push("approval", id=aid, client=client, tool=name, reason=reason)
    try:
        return await asyncio.wait_for(asyncio.shield(fut), APPROVAL_WAIT)
    except asyncio.TimeoutError:
        return False
    finally:
        approvals.pop(aid, None)
        push("approvals")


def decide(aid: str, approve: bool) -> bool:
    a = approvals.get(aid)
    if not a or a["future"].done():
        return False
    a["future"].set_result(approve)
    return True


def record(client: str, name: str, args: dict, ok: bool, ms: float, text: str, rev0: int) -> None:
    global _seq
    _seq += 1
    clients.get(client, {})["calls"] = clients.get(client, {}).get("calls", 0) + 1
    activity.appendleft({"id": _seq, "at": time.time(), "client": client, "tool": name, "args": short(args), "ok": ok, "ms": round(ms),
                         "note": text[:200], "changed": core.rev != rev0})
    push("activity", id=_seq)


def snapshot() -> dict:
    c = choice()
    pol = c["policies"]
    names = sorted(set(clients) | set(pol))
    return {
        "enabled": c["enabled"],
        "seal": c["seal"],
        "sealBroken": c["sealBroken"],
        "endpoint": f"http://{config.HOST}:{config.PORT}/mcp",
        "stdio": "./formdesign mcp",
        "clients": [{**clients.get(n, {"name": n, "version": None, "transport": None, "lastSeen": None, "calls": 0}), "policy": policy(n)} for n in names],
        "approvals": [{k: v for k, v in a.items() if k != "future"} for a in approvals.values()],
        "activity": list(activity)[:60],
    }


def install(client: str, dry: bool) -> str:
    if client not in ("claude", "codex", "cursor"):
        raise ValueError("client is claude, codex or cursor")
    script = config.REPO_ROOT / "setup" / "agents.py"
    if config.FROZEN or not script.exists():
        raise ValueError("agent setup runs from a FormDesign source folder: ./formdesign agents install " + client)
    args = [sys.executable, str(script), "install", client] + (["--dry-run"] if dry else [])
    r = subprocess.run(args, cwd=str(config.REPO_ROOT), capture_output=True, text=True, timeout=120)
    if r.returncode != 0:
        raise ValueError((r.stderr or r.stdout).strip() or f"agents install {client} failed")
    return r.stdout.strip()
