import json

from fastapi import APIRouter, HTTPException, Request
from fastapi.responses import FileResponse

from app.routers.terminal import _guard
from app.services import claude_chat, claude_code

router = APIRouter()


async def _body(request: Request) -> dict:
    try:
        d = json.loads(await request.body() or b"{}")
    except ValueError:
        raise HTTPException(400, "expected a JSON body")
    if not isinstance(d, dict):
        raise HTTPException(400, "expected a JSON object")
    return d


def _run(fn, *args):
    try:
        return fn(*args)
    except claude_code.ClaudeError as exc:
        raise HTTPException(400, str(exc))


@router.get("/claude/status")
async def status(request: Request):
    _guard(request)
    return claude_code.status()


@router.post("/claude/prefs")
async def prefs(request: Request):
    _guard(request)
    _run(claude_code.save_prefs, await _body(request))
    return claude_code.status()


@router.get("/claude/chat")
async def chat_state(request: Request):
    _guard(request)
    return claude_chat.state()


@router.post("/claude/chat")
async def chat_send(request: Request):
    _guard(request)
    b = await _body(request)
    return _run(claude_chat.send, str(b.get("text") or ""), str(b.get("context") or ""), str(b.get("note") or ""))


@router.post("/claude/chat/stop")
async def chat_stop(request: Request):
    _guard(request)
    return claude_chat.cancel()


@router.post("/claude/chat/reset")
async def chat_reset(request: Request):
    _guard(request)
    return claude_chat.reset()


@router.get("/claude/sessions")
async def chat_sessions(request: Request):
    _guard(request)
    return _run(claude_chat.sessions)


@router.post("/claude/adopt")
async def chat_adopt(request: Request):
    _guard(request)
    b = await _body(request)
    return _run(claude_chat.adopt, str(b.get("session") or ""))


@router.get("/claude/image/{name}")
async def chat_image(name: str, request: Request):
    _guard(request)
    p = claude_chat.image_path(name)
    if p is None:
        raise HTTPException(404, "no such image")
    return FileResponse(p)
