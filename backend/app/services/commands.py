import asyncio
import json
import secrets
from pathlib import Path

from app.engine.core import core

META_FILE = Path(__file__).resolve().parent.parent / "command_meta.json"
RISKY_PREFIX = ("file.print", "file.package", "file.export", "book.export", "file.close", "layout.pages.delete")
RISKY_EXACT = {"file.save", "file.saveAs", "file.revert", "layer.delete", "document.close"}
WAIT = 30.0

window: list[dict] = []
_engine: list[dict] = []
_meta: dict | None = None
_pending: dict[str, asyncio.Future] = {}


class CommandError(ValueError):
    pass


def meta() -> dict:
    global _meta
    if _meta is None:
        try:
            d = json.loads(META_FILE.read_text(encoding="utf-8"))
            _meta = d if isinstance(d, dict) else {}
        except (OSError, ValueError):
            _meta = {}
    return _meta


def risky(cid: str) -> bool:
    return cid in RISKY_EXACT or cid.startswith(RISKY_PREFIX)


def _words(entry: dict) -> dict:
    m = meta().get(entry["id"]) or {}
    group = entry.get("group") or m.get("group") or (entry["id"].split(".")[0] if "." in entry["id"] else "window")
    desc = entry.get("description") or m.get("description") or entry.get("help") or ""
    return {**entry, "group": group, "description": desc, "when": entry.get("when") or m.get("when") or "",
            "risky": bool(entry.get("risky") or m.get("risky") or risky(entry["id"]))}


async def engine_commands(fresh: bool = False) -> list[dict]:
    if fresh or not _engine:
        r = await core.call("commands", {"enabledOnly": False}, source="rest")
        cmds = r.result.get("commands", r.result) if isinstance(r.result, dict) else r.result
        _engine[:] = [c for c in (cmds or []) if isinstance(c, dict) and c.get("id")]
    return _engine


def mirror(entries: list) -> int:
    keep = []
    for a in entries[:3000]:
        if not isinstance(a, dict) or not isinstance(a.get("id"), str):
            continue
        keep.append({"id": a["id"], "label": str(a.get("label") or a["id"])[:160], "keys": [str(k) for k in (a.get("keys") or [])][:4],
                     "menu": str(a.get("menu") or "")[:200], "group": str(a.get("group") or "")[:40], "description": str(a.get("description") or "")[:600],
                     "when": str(a.get("when") or "")[:300], "params": a.get("params") if isinstance(a.get("params"), dict) else {},
                     "mutates": bool(a.get("mutates", False)), "risky": bool(a.get("risky", False)), "kind": "window"})
    window[:] = keep
    return len(keep)


async def catalogue(q: str = "") -> list[dict]:
    out = []
    for c in await engine_commands():
        out.append(_words({"id": c["id"], "kind": "command", "label": c.get("label") or c["id"], "menu": " > ".join(c.get("menu") or []),
                           "keys": [c["shortcut"]] if c.get("shortcut") else [], "params": c.get("params") or "{}", "mutates": bool(c.get("undoable", True)),
                           "help": c.get("help") or "", "enabled": c.get("enabled", True)}))
    seen = {o["id"] for o in out}
    out += [_words(w) for w in window if w["id"] not in seen]
    ql = q.lower().strip()
    if ql:
        out = [o for o in out if any(ql in str(o.get(k) or "").lower() for k in ("id", "label", "menu", "group", "description"))]
    return out


async def describe(cid: str) -> dict:
    for o in await catalogue():
        if o["id"] == cid:
            return o
    raise CommandError(f"no command `{cid}`; GET /api/commands?q=word lists them")


def _push(ev: dict) -> int:
    n = 0
    for q in list(core.listeners):
        try:
            q.put_nowait(ev)
            n += 1
        except asyncio.QueueFull:
            pass
    return n


async def run(cid: str, params: dict, source: str = "rest") -> dict:
    entry = await describe(cid)
    if entry["kind"] == "command":
        from app.routers.core_api import ROUTED, routed_exec
        if cid in ROUTED:
            res = await routed_exec({"command": cid, "params": params or {}}, source)
        else:
            res = await core.exec(cid, params or {}, source=source)
        return {"ran": True, "kind": "command", "result": res}
    rid = secrets.token_hex(6)
    fut = asyncio.get_running_loop().create_future()
    _pending[rid] = fut
    try:
        if not _push({"type": "command", "id": cid, "params": params or {}, "rid": rid}):
            raise CommandError(f"`{cid}` is a window command and no FormDesign window is open")
        try:
            return await asyncio.wait_for(fut, WAIT)
        except asyncio.TimeoutError:
            raise CommandError(f"the window did not answer `{cid}` within {int(WAIT)} seconds") from None
    finally:
        _pending.pop(rid, None)


def reply(rid: str, body: dict) -> bool:
    fut = _pending.get(rid)
    if fut is None or fut.done():
        return False
    fut.set_result({"ran": bool(body.get("ran")), "kind": "window", "result": body.get("result"), **({"error": str(body["error"])} if body.get("error") else {})})
    return True


def lacking() -> list[str]:
    return [c["id"] for c in _engine + window if not (meta().get(c["id"], {}).get("description") or c.get("help") or c.get("description"))]
