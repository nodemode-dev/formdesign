import asyncio
import json
import urllib.parse

from fastapi import APIRouter, HTTPException, Request, WebSocket, WebSocketDisconnect
from pydantic import BaseModel

from app.core import window_key
from app.services.pty import presets, terminals, workdir

router = APIRouter()


def _local(origin: str | None) -> bool:
    if not origin:
        return True
    host = urllib.parse.urlparse(origin).hostname or ""
    return host in ("localhost", "127.0.0.1", "::1", "tauri.localhost") or origin.startswith("tauri://")


def _guard(request: Request) -> None:
    if not _local(request.headers.get("origin")):
        raise HTTPException(403, "terminals answer only this computer's FormDesign window")


class NewTerminal(BaseModel):
    preset: str = "shell"
    cols: int = 100
    rows: int = 30
    prompt: str | None = None
    resume: str | None = None
    reuse: bool = False


@router.get("/term")
async def term_info(request: Request):
    window_key.require(request)
    return {"presets": presets(), "sessions": terminals.list(), "cwd": str(workdir())}


@router.post("/term")
async def term_new(body: NewTerminal, request: Request):
    window_key.require(request)
    if body.reuse and body.preset == "claude" and not body.resume:
        s = await terminals.reuse("claude", body.prompt)
        if s is not None:
            return {"id": s.id, "preset": s.preset, "title": s.title, "reused": True}
    try:
        s = await terminals.create(body.preset, body.cols, body.rows, body.prompt, body.resume)
    except ValueError as e:
        raise HTTPException(400, str(e))
    return {"id": s.id, "preset": s.preset, "title": s.title, "reused": False}


@router.post("/term/{sid}/restart")
async def term_restart(sid: str, request: Request):
    window_key.require(request)
    try:
        s = await terminals.restart(sid)
    except KeyError:
        raise HTTPException(404, "no such terminal")
    except ValueError as e:
        raise HTTPException(400, str(e))
    return {"id": s.id, "preset": s.preset, "title": s.title}


@router.delete("/term/{sid}")
async def term_close(sid: str, request: Request):
    window_key.require(request)
    terminals.close(sid)
    return {"ok": True}


@router.websocket("/term/{sid}/ws")
async def term_ws(ws: WebSocket, sid: str):
    if not _local(ws.headers.get("origin")) or not window_key.spend(ws.query_params.get("ticket", "")):
        await ws.close(code=4403)
        return
    s = terminals.sessions.get(sid)
    if s is None:
        await ws.close(code=4404)
        return
    typed = ws.query_params.get("notices") == "1"
    await ws.accept()
    q: asyncio.Queue = asyncio.Queue()
    first, ended = s.replay(typed), s.exited
    s.listeners.add(q)
    if typed:
        s.typed.add(q)
        await ws.send_text(json.dumps({"type": "hello", "alive": ended is None, "kind": s.preset}))
    if first:
        await ws.send_bytes(first)
    if typed and ended is not None:
        await ws.send_text(json.dumps({"type": "exit", "code": ended}))

    async def pump() -> None:
        while True:
            data = await q.get()
            if isinstance(data, dict):
                await ws.send_text(json.dumps(data))
            else:
                await ws.send_bytes(data)

    task = asyncio.create_task(pump())
    try:
        while True:
            msg = await ws.receive()
            if msg.get("type") == "websocket.disconnect":
                break
            if msg.get("bytes") is not None:
                s.write(msg["bytes"])
            elif msg.get("text") is not None:
                try:
                    ctl = json.loads(msg["text"])
                except ValueError:
                    continue
                if ctl.get("resize"):
                    cols, rows = ctl["resize"]
                    s.resize(int(cols), int(rows))
    except WebSocketDisconnect:
        pass
    finally:
        s.listeners.discard(q)
        s.typed.discard(q)
        task.cancel()
