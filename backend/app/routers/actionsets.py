import json
import re

from fastapi import APIRouter, HTTPException, Request

from app.core import config
from app.routers.terminal import _guard

router = APIRouter()
FILE = config.DATA_DIR / "actions.json"
ID = re.compile(r"^[A-Za-z][\w.]{0,80}$")


def _read() -> dict:
    try:
        d = json.loads(FILE.read_text(encoding="utf-8"))
        return d if isinstance(d, dict) else {}
    except (OSError, ValueError):
        return {}


def _write(d: dict) -> None:
    FILE.parent.mkdir(parents=True, exist_ok=True)
    FILE.write_text(json.dumps(d, indent=1, ensure_ascii=False), encoding="utf-8")


def _steps(raw) -> list[dict]:
    out = []
    for s in raw if isinstance(raw, list) else []:
        if isinstance(s, dict) and isinstance(s.get("command"), str) and ID.match(s["command"]):
            out.append({"command": s["command"], "params": s.get("params") if isinstance(s.get("params"), dict) else {}})
    return out[:500]


@router.get("/actionsets")
async def list_actions(request: Request):
    _guard(request)
    return {"actions": [{"name": k, "steps": v.get("steps", [])} for k, v in sorted(_read().items())]}


@router.post("/actionsets")
async def save_action(request: Request):
    _guard(request)
    try:
        b = json.loads(await request.body() or b"{}")
    except ValueError:
        raise HTTPException(400, "expected a JSON body")
    name = str(b.get("name") or "").strip()[:80]
    steps = _steps(b.get("steps"))
    if not name or not steps:
        raise HTTPException(400, "an action needs a name and at least one recorded step")
    d = _read()
    d[name] = {"steps": steps}
    _write(d)
    return {"ok": True, "name": name, "steps": len(steps)}


@router.delete("/actionsets/{name}")
async def delete_action(name: str, request: Request):
    _guard(request)
    d = _read()
    if d.pop(name, None) is None:
        raise HTTPException(404, f"no action named {name}")
    _write(d)
    return {"ok": True}
