import asyncio
import json
import logging

from fastapi import APIRouter, Body, HTTPException, Query, WebSocket, WebSocketDisconnect
from fastapi.responses import Response

from app.engine.core import CoreError, core
from app.engine.documents import documents

router = APIRouter()
log = logging.getLogger("formdes.ws")

JSON_METHODS = {"exec", "batch", "commands", "inspect", "state", "layout", "overlays", "pointer", "key", "text", "cursor",
                "tool", "tools", "docs.activate", "docs.close", "fonts.families", "fonts.styles", "fonts.face", "story.paras", "objectStyles", "findChange", "spellMarks", "spellSuggest", "pathType", "hello", "ping", "slots",
                "frame.thread", "frame.unthread"}
BINARY_METHODS = {"tiles", "page.png", "serialize"}
ROUTED = {"file.save", "file.saveAs", "file.new", "file.open"}


async def routed_exec(params: dict, source: str):
    command = params.get("command")
    p = params.get("params") or {}
    if command in ("file.save", "file.saveAs"):
        return await documents.save(p.get("path"), source=source)
    if command == "file.open":
        return await documents.open_path(str(p.get("path", "")), source=source)
    return await documents.new(p, source=source)


@router.post("/core/{method}")
async def call(method: str, params: dict = Body(default={})):
    if method not in JSON_METHODS:
        raise HTTPException(404, f"unknown core method {method}")
    try:
        if method == "exec" and params.get("command") in ROUTED:
            return {"result": await routed_exec(params, "rest"), "rev": core.rev, "selRev": core.sel_rev}
        r = await core.call(method, params, source="rest")
    except CoreError as exc:
        raise HTTPException(422, str(exc)) from exc
    return {"result": r.result, **r.meta}


@router.get("/core/page.png")
async def page_png(page: int = 0, maxSide: float | None = Query(default=None), scale: float = 1.0, bleed: bool = False, format: str = "png"):
    params = {"page": page, "scale": scale, "bleed": bleed, "format": format}
    if maxSide:
        params["maxSide"] = maxSide
    try:
        r = await core.call("page.png", params, source="rest")
    except CoreError as exc:
        raise HTTPException(422, str(exc)) from exc
    return Response(r.bin or b"", media_type=r.result.get("mime", "image/png"), headers={"Cache-Control": "no-store"})


@router.get("/core/font")
async def font_file(family: str, style: str = "Regular"):
    try:
        r = await core.call("fonts.data", {"family": family, "style": style}, source="rest")
    except CoreError as exc:
        raise HTTPException(404, str(exc)) from exc
    return Response(r.bin or b"", media_type="font/ttf", headers={"Cache-Control": "private, max-age=3600"})


def _frame(header: dict, payload: bytes) -> bytes:
    h = json.dumps(header, separators=(",", ":")).encode()
    return len(h).to_bytes(4, "big") + h + payload


@router.websocket("/ws")
async def ws(socket: WebSocket):
    await socket.accept()
    queue = core.subscribe()
    send_lock = asyncio.Lock()

    async def send_json(obj: dict) -> None:
        async with send_lock:
            await socket.send_text(json.dumps(obj, separators=(",", ":")))

    async def send_bytes(data: bytes) -> None:
        async with send_lock:
            await socket.send_bytes(data)

    async def pump() -> None:
        while True:
            ev = await queue.get()
            await send_json(ev)

    async def handle(msg: dict) -> None:
        rid = msg.get("id")
        method = msg.get("method", "")
        params = msg.get("params") or {}
        if method not in JSON_METHODS and method not in BINARY_METHODS:
            await send_json({"id": rid, "ok": False, "error": f"unknown method {method}"})
            return
        try:
            if method == "exec" and params.get("command") in ROUTED:
                result = await routed_exec(params, "window")
                await send_json({"id": rid, "ok": True, "result": result, "rev": core.rev, "selRev": core.sel_rev})
                return
            r = await core.call(method, params, source="window")
        except CoreError as exc:
            await send_json({"id": rid, "ok": False, "error": str(exc)})
            return
        if r.bin is not None:
            await send_bytes(_frame({"id": rid, "ok": True, "result": r.result, **r.meta}, r.bin))
        else:
            await send_json({"id": rid, "ok": True, "result": r.result, **r.meta})

    pumper = asyncio.create_task(pump())
    tasks: set[asyncio.Task] = set()
    try:
        await send_json({"type": "hello", "rev": core.rev, "selRev": core.sel_rev, "core": core.version, "running": core.alive()})
        while True:
            raw = await socket.receive_text()
            try:
                msg = json.loads(raw)
            except ValueError:
                continue
            t = asyncio.create_task(handle(msg))
            tasks.add(t)
            t.add_done_callback(tasks.discard)
    except WebSocketDisconnect:
        pass
    except RuntimeError as exc:
        log.info("window socket closed: %s", exc)
    finally:
        pumper.cancel()
        for t in tasks:
            t.cancel()
        core.unsubscribe(queue)
