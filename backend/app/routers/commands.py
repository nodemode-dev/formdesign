import json

from fastapi import APIRouter, HTTPException, Request

from app.engine.core import CoreError
from app.routers.terminal import _guard
from app.services import commands as C

router = APIRouter()


async def _body(request: Request) -> dict:
    try:
        d = json.loads(await request.body() or b"{}")
    except ValueError:
        raise HTTPException(400, "expected a JSON body")
    if not isinstance(d, dict):
        raise HTTPException(400, "expected a JSON object")
    return d


@router.post("/commands/catalogue")
async def catalogue_mirror(request: Request):
    _guard(request)
    b = await _body(request)
    return {"ok": True, "count": C.mirror(b.get("commands") or [])}


@router.post("/commands/reply/{rid}")
async def command_reply(rid: str, request: Request):
    _guard(request)
    return {"ok": C.reply(rid, await _body(request))}


@router.get("/commands")
async def list_commands(request: Request, q: str = ""):
    _guard(request)
    try:
        cmds = await C.catalogue(q)
    except CoreError as exc:
        raise HTTPException(503, str(exc))
    return {"count": len(cmds), "commands": cmds}


@router.get("/commands/{cid}")
async def describe_command(cid: str, request: Request):
    _guard(request)
    try:
        return await C.describe(cid)
    except C.CommandError as exc:
        raise HTTPException(404, str(exc))


@router.post("/commands/{cid}")
async def run_command(cid: str, request: Request):
    _guard(request)
    b = await _body(request)
    params = b.get("params") if isinstance(b.get("params"), dict) else {k: v for k, v in b.items() if k != "yes"}
    try:
        entry = await C.describe(cid)
        if entry["risky"] and not b.get("yes"):
            raise HTTPException(409, f"`{cid}` cannot be undone ({entry['description'] or entry['label']}); send yes: true to run it")
        return await C.run(cid, params, "rest")
    except C.CommandError as exc:
        raise HTTPException(400, str(exc))
    except CoreError as exc:
        raise HTTPException(422, str(exc))
