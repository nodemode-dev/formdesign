import json

from fastapi import APIRouter, HTTPException, Request
from fastapi.responses import StreamingResponse

from app.engine.core import CoreError
from app.routers.terminal import _guard
from app.services import ai

router = APIRouter()


def _fail(exc: Exception) -> HTTPException:
    return HTTPException(400 if isinstance(exc, (ai.AIError, CoreError)) else 500, str(exc))


async def _body(request: Request) -> dict:
    try:
        d = json.loads(await request.body() or b"{}")
    except ValueError:
        raise HTTPException(400, "expected a JSON body")
    if not isinstance(d, dict):
        raise HTTPException(400, "expected a JSON object")
    return d


@router.get("/assistant/settings")
async def get_settings(request: Request):
    _guard(request)
    return ai.public()


@router.post("/assistant/settings")
async def set_settings(request: Request):
    _guard(request)
    try:
        return ai.update(await _body(request))
    except ai.AIError as exc:
        raise _fail(exc)


@router.post("/assistant/test")
async def test(request: Request):
    _guard(request)
    try:
        return {"ok": True, "reply": (await ai.test())[:200]}
    except (ai.AIError, CoreError) as exc:
        raise _fail(exc)


@router.get("/assistant/history")
async def history(request: Request):
    _guard(request)
    try:
        return await ai.history()
    except (ai.AIError, CoreError):
        return {"doc": None, "running": False, "items": []}


@router.post("/assistant/reset")
async def reset(request: Request):
    _guard(request)
    try:
        await ai.reset()
    except (ai.AIError, CoreError):
        pass
    return {"ok": True}


@router.post("/assistant/stop")
async def stop(request: Request):
    _guard(request)
    try:
        return {"stopped": await ai.stop()}
    except (ai.AIError, CoreError):
        return {"stopped": False}


@router.post("/assistant/chat")
async def chat(request: Request):
    """One turn of the assistant as server-sent events: text, tool, tool_result, approval, then done, stopped or error."""
    _guard(request)
    b = await _body(request)
    message = str(b.get("message") or "").strip()[:8000]
    if not message:
        raise HTTPException(400, "write a message")
    if not ai.configured():
        raise HTTPException(400, "The assistant is off or not set up. Turn it on in Preferences > AI.")
    page = b.get("page") if isinstance(b.get("page"), int) else None

    async def stream():
        try:
            async for ev in ai.chat(message, page):
                yield f"data: {json.dumps(ev, ensure_ascii=False)}\n\n"
        except (ai.AIError, CoreError) as exc:
            yield f"data: {json.dumps({'type': 'error', 'error': str(exc)})}\n\n"

    return StreamingResponse(stream(), media_type="text/event-stream", headers={"Cache-Control": "no-store", "X-Accel-Buffering": "no"})


@router.post("/ai/alt-text/propose")
async def alt_propose(request: Request):
    _guard(request)
    try:
        return await ai.alt_propose(str((await _body(request)).get("mode") or "auto"))
    except (ai.AIError, CoreError) as exc:
        raise _fail(exc)


@router.post("/ai/alt-text/apply")
async def alt_apply(request: Request):
    _guard(request)
    items = (await _body(request)).get("items")
    if not isinstance(items, list):
        raise HTTPException(400, "expected {items: [{id, text}]}")
    try:
        return await ai.alt_apply([i for i in items if isinstance(i, dict) and isinstance(i.get("id"), int)])
    except (ai.AIError, CoreError) as exc:
        raise _fail(exc)


@router.post("/ai/auto-style")
async def auto_style(request: Request):
    """Classify the paragraphs of the selected (or every unstyled) story and apply the document's styles. apply false only returns the plan."""
    _guard(request)
    b = await _body(request)
    try:
        plan = await ai.autostyle_plan(bool(b.get("ai", True)), bool(b.get("createMissing", True)))
        if b.get("apply", True):
            done = await ai.autostyle_apply(plan)
            return {**done, "stories": len(plan["stories"]), "missing": plan["missing"], "styles": plan["styles"]}
        return plan
    except (ai.AIError, CoreError) as exc:
        raise _fail(exc)


@router.post("/ai/image/generate")
async def image_generate(request: Request):
    _guard(request)
    b = await _body(request)
    try:
        return await ai.image_generate(str(b.get("prompt") or ""), str(b.get("style") or ""), b.get("frame") if isinstance(b.get("frame"), int) else None)
    except (ai.AIError, CoreError) as exc:
        raise _fail(exc)


@router.post("/ai/image/expand")
async def image_expand(request: Request):
    _guard(request)
    b = await _body(request)
    try:
        return await ai.image_expand(b.get("frame") if isinstance(b.get("frame"), int) else None, str(b.get("prompt") or ""))
    except (ai.AIError, CoreError) as exc:
        raise _fail(exc)
