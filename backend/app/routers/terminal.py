import asyncio
import hmac
import json
import urllib.parse

from fastapi import APIRouter, HTTPException, Request, WebSocket, WebSocketDisconnect
from pydantic import BaseModel

from app.services.pty import TOKEN, presets, terminals, workdir

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


@router.get("/term")
async def term_info(request: Request):
    _guard(request)
    return {"token": TOKEN, "presets": presets(), "sessions": terminals.list(), "cwd": str(workdir())}


@router.post("/term")
async def term_new(body: NewTerminal, request: Request):
    _guard(request)
    try:
        s = await terminals.create(body.preset, body.cols, body.rows, body.prompt, body.resume)
    except ValueError as e:
        raise HTTPException(400, str(e))
    return {"id": s.id, "preset": s.preset}


@router.delete("/term/{sid}")
async def term_close(sid: str, request: Request):
    _guard(request)
    terminals.close(sid)
    return {"ok": True}


@router.websocket("/term/{sid}/ws")
async def term_ws(ws: WebSocket, sid: str):
    if not _local(ws.headers.get("origin")) or not hmac.compare_digest(ws.query_params.get("token", ""), TOKEN):
        await ws.close(code=4403)
        return
    s = terminals.sessions.get(sid)
    if s is None:
        await ws.close(code=4404)
        return
    await ws.accept()
    q: asyncio.Queue = asyncio.Queue()
    if s.buffer:
        await ws.send_bytes(bytes(s.buffer))
    s.listeners.add(q)

    async def pump() -> None:
        while True:
            data = await q.get()
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
        task.cancel()
