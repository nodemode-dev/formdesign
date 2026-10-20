import asyncio

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel

from app.engine.core import core
from app.routers.terminal import _guard
from app.services import agents as A

router = APIRouter()
_window_actions: list[dict] = []


class Policy(BaseModel):
    client: str
    policy: str


class Decision(BaseModel):
    approve: bool


class Install(BaseModel):
    client: str
    dry: bool = False


class Enabled(BaseModel):
    enabled: bool


class WindowActions(BaseModel):
    actions: list[dict]


@router.get("/agents")
async def agents(request: Request):
    _guard(request)
    return A.snapshot()


@router.post("/agents/enabled")
async def agents_enabled(body: Enabled, request: Request):
    _guard(request)
    A.set_enabled(body.enabled)
    return {"enabled": body.enabled}


@router.post("/agents/policy")
async def agents_policy(body: Policy, request: Request):
    _guard(request)
    try:
        A.set_policy(body.client, body.policy)
    except ValueError as e:
        raise HTTPException(400, str(e))
    return {"ok": True}


@router.post("/agents/approvals/{aid}")
async def agents_decide(aid: str, body: Decision, request: Request):
    _guard(request)
    if not A.decide(aid, body.approve):
        raise HTTPException(404, "that request is no longer waiting")
    return {"ok": True}


@router.post("/agents/turn")
async def agents_turn_end(request: Request):
    """Ends the agents' open turns: their next changes start a new undo step. Claude Code's UserPromptSubmit hook in
    this repository calls it, so one prompt's work is one Undo. It can only split undo steps, never merge or drop them."""
    _guard(request)
    return {"ended": A.end_turns()}


@router.post("/agents/install")
async def agents_install(body: Install, request: Request):
    _guard(request)
    try:
        text = await asyncio.to_thread(A.install, body.client, body.dry)
    except ValueError as e:
        raise HTTPException(400, str(e))
    return {"ok": True, "text": text}


@router.post("/actions/window")
async def actions_window(body: WindowActions, request: Request):
    _guard(request)
    keep = []
    for a in body.actions[:2000]:
        if isinstance(a.get("id"), str):
            keep.append({"id": a["id"], "label": str(a.get("label") or a["id"])[:120], "keys": [str(k) for k in (a.get("keys") or [])][:4], "menu": str(a.get("menu") or "")[:160]})
    _window_actions[:] = keep
    return {"ok": True, "count": len(keep)}


@router.get("/actions")
async def actions(request: Request, q: str = ""):
    """Everything a person or an agent can do, from one list: engine commands, tools and the window's own actions."""
    _guard(request)
    from app.routers.mcp import TOOLS
    cmds = (await core.call("commands", {})).result
    cmds = cmds.get("commands", cmds) if isinstance(cmds, dict) else cmds
    tools = (await core.call("tools", {})).result
    tools = tools.get("tools", tools) if isinstance(tools, dict) else tools
    flat = [t for g in tools for t in (g if isinstance(g, list) else [g])] if isinstance(tools, list) else []
    out = [{"kind": "command", "id": c["id"], "label": c.get("label"), "menu": " > ".join(c.get("menu") or []), "keys": [c["shortcut"]] if c.get("shortcut") else [],
            "params": c.get("params"), "reach": ["menu", "search", "cli: formdesign cmd", "mcp: run_commands"]} for c in cmds]
    out += [{"kind": "tool", "id": t.get("id"), "label": t.get("label"), "menu": "Tools", "keys": [t["shortcut"]] if t.get("shortcut") else [],
             "reach": ["rail", "search", "mcp: show tool"]} for t in flat if isinstance(t, dict)]
    out += [{"kind": "window", **w, "reach": ["menu", "search"]} for w in _window_actions if not any(o["id"] == w["id"] for o in out)]
    out += [{"kind": "mcp", "id": t["name"], "label": t["name"], "menu": "", "keys": [], "reach": ["mcp", "cli: formdesign tool"]} for t in TOOLS]
    ql = q.lower().strip()
    if ql:
        out = [o for o in out if ql in str(o["id"]).lower() or ql in str(o.get("label") or "").lower() or ql in str(o.get("menu") or "").lower()]
    counts = {k: sum(1 for o in out if o["kind"] == k) for k in ("command", "tool", "window", "mcp")}
    return {"counts": counts, "actions": out}
