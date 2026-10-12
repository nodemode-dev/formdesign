import asyncio

from fastapi import APIRouter, Body, File, HTTPException, UploadFile
from fastapi.responses import FileResponse, PlainTextResponse

from app.services import plugins as P

router = APIRouter(prefix="/plugins")


def _fail(exc: Exception, code: int = 400):
    raise HTTPException(code, str(exc))


@router.get("")
async def listing():
    return P.listing()


@router.get("/repo/{path:path}", include_in_schema=False)
async def repo(path: str):
    target = (P.REPO_PLUGINS / path).resolve()
    if P.REPO_PLUGINS.resolve() not in target.parents or not target.is_file():
        raise HTTPException(404, "not found")
    return FileResponse(target)


@router.get("/catalogue")
async def catalogue():
    try:
        return await asyncio.to_thread(P.catalogue)
    except P.PluginError as exc:
        _fail(exc, 502)


@router.post("/catalogue/url")
async def catalogue_url(body: dict = Body(...)):
    try:
        return {"url": P.set_catalogue(str(body.get("url") or ""))}
    except P.PluginError as exc:
        _fail(exc)


@router.post("/stage")
async def stage(body: dict = Body(...)):
    try:
        if body.get("catalogue"):
            return await asyncio.to_thread(P.stage_catalogue, str(body["catalogue"]))
        if body.get("path"):
            return await asyncio.to_thread(P.stage_path, str(body["path"]))
    except P.PluginError as exc:
        _fail(exc)
    raise HTTPException(400, "give a catalogue id or a path")


@router.post("/stage/upload")
async def stage_upload(file: UploadFile = File(...)):
    try:
        return await asyncio.to_thread(P.stage_bytes, await file.read(P.MAX_ZIP + 1), "file")
    except P.PluginError as exc:
        _fail(exc)


@router.post("/install")
async def install(body: dict = Body(...)):
    try:
        return P.install(str(body.get("stage") or ""), body.get("consent") is True)
    except P.PluginError as exc:
        _fail(exc)


@router.post("/{pid}/enabled")
async def enabled(pid: str, body: dict = Body(...)):
    try:
        return P.set_enabled(pid, bool(body.get("enabled")))
    except P.PluginError as exc:
        _fail(exc, 404)


@router.post("/{pid}/uninstall")
async def uninstall(pid: str):
    try:
        P.uninstall(pid)
    except P.PluginError as exc:
        _fail(exc, 404)
    return {"ok": True}


@router.get("/{pid}/entry")
async def entry(pid: str):
    try:
        return PlainTextResponse(P.entry_source(pid), media_type="text/javascript")
    except (P.PluginError, OSError) as exc:
        _fail(exc, 404)


QUERY_COMMANDS = {"story.get", "story.paras", "font.list"}
QUERY_METHODS = {"state", "inspect"}


async def _query(pid: str, args: dict) -> dict:
    """Read-only questions to the core (a story's text, the selection) that no MCP tool answers. They honour Agent Connect's switch and Deny."""
    import json
    import time

    from app.engine.core import CoreError, core
    from app.routers.mcp import _fail as tool_fail, _text, agents_enabled
    from app.services import agents as A
    client = f"plugin:{pid}"
    if not agents_enabled():
        return tool_fail("Agent access to FormDesign is turned off in Window > Agent Connect.")
    if A.policy(client) == "deny":
        A.record(client, "query", args, False, 0, "refused: this client is set to Deny in Agent Connect", core.rev)
        return tool_fail(f"{client} is set to Deny in FormDesign's Agent Connect panel.")
    t0, rev0 = time.perf_counter(), core.rev
    try:
        if args.get("command") in QUERY_COMMANDS:
            out = await core.exec(args["command"], args.get("params") or {}, source="mcp")
        elif args.get("method") in QUERY_METHODS:
            out = (await core.call(args["method"], {}, source="mcp")).result
        else:
            raise HTTPException(403, "query takes a read-only command (" + ", ".join(sorted(QUERY_COMMANDS)) + ") or a method (" + ", ".join(sorted(QUERY_METHODS)) + ")")
    except CoreError as exc:
        return tool_fail(str(exc))
    res = _text(out)
    A.record(client, "query", args, True, (time.perf_counter() - t0) * 1000, json.dumps(args.get("command") or args.get("method")), rev0)
    return res


@router.post("/{pid}/call")
async def call(pid: str, body: dict = Body(...)):
    """The broker's second check, then the tool runs as the MCP client plugin:<pid>, under Agent Connect's policy."""
    from app.routers.mcp import NAMES, call_tool
    tool, args = str(body.get("tool") or ""), body.get("args") or {}
    if not isinstance(args, dict):
        raise HTTPException(400, "args is an object")
    try:
        P.allowed_tool(pid, tool)
    except P.PluginError as exc:
        _fail(exc, 403)
    if tool == "query":
        return await _query(pid, args)
    if tool not in NAMES:
        raise HTTPException(404, f"no tool named {tool}")
    return await call_tool(tool, args, f"plugin:{pid}")


@router.post("/{pid}/fetch")
async def fetch(pid: str, body: dict = Body(...)):
    try:
        P.allowed_fetch(pid, str(body.get("url") or ""))
    except P.PluginError as exc:
        _fail(exc, 403)
    try:
        return {"text": await asyncio.to_thread(P.fetch, pid, str(body["url"]))}
    except P.PluginError as exc:
        _fail(exc, 502)
