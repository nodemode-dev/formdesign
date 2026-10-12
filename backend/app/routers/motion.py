from fastapi import APIRouter, Body, HTTPException
from fastapi.responses import Response

from app.engine.core import CoreError
from app.services import motion as M

router = APIRouter()


async def _meta():
    meta = await M.active_meta()
    if meta is None:
        raise HTTPException(422, "no document open")
    return meta


def _fail(exc: Exception) -> HTTPException:
    return HTTPException(422, str(exc))


@router.get("/motion")
async def listing():
    meta = await _meta()
    try:
        geo = await M.geometry()
    except CoreError as exc:
        raise _fail(exc) from exc
    m = M.normalise(meta, geo)
    out = {}
    for k, a in m["anims"].items():
        g = geo[int(k)]
        out[k] = {**a, "kf": M.keyframes(a, g), "opt": M.options(a), "page": g["page"], "pageName": g["pageName"], "label": g["label"] or g["kind"], "kind": g["kind"], "bounds": g["bounds"], "spread": g["spread"]}
    return {"presets": [{"id": k, "label": v["label"], "enter": v["enter"]} for k, v in M.PRESETS.items()], "events": list(M.EVENTS), "speeds": list(M.SPEEDS),
            "anims": out, "timing": m["timing"], "items": {str(i): {"label": g["label"] or g["kind"], "kind": g["kind"], "page": g["page"], "pageName": g["pageName"]} for i, g in geo.items()},
            "ffmpeg": bool(M.has_ffmpeg())}


@router.post("/motion/set")
async def set_animation(body: dict = Body(...)):
    meta = await _meta()
    ids = [int(i) for i in body.get("ids") or []]
    if not ids:
        raise HTTPException(422, "ids: select the objects to animate")
    try:
        geo = await M.geometry()
        m = M.store(meta)
        raw = body.get("anim")
        for i in ids:
            if i not in geo:
                raise CoreError(f"no object with id {i}")
            if raw is None:
                m["anims"].pop(str(i), None)
            else:
                m["anims"][str(i)] = M.clean(raw)
        M.normalise(meta, geo)
    except CoreError as exc:
        raise _fail(exc) from exc
    M.notify()
    return {"ok": True}


@router.post("/motion/timing")
async def set_timing(body: dict = Body(...)):
    meta = await _meta()
    try:
        page = str(int(body["page"]))
        groups = [[int(i) for i in g] for g in body.get("groups") or []]
    except (KeyError, TypeError, ValueError) as exc:
        raise HTTPException(422, "page and groups ([[id, ...], ...]) are required") from exc
    M.store(meta)["timing"][page] = [g for g in groups if g]
    M.normalise(meta, await M.geometry())
    M.notify()
    return {"ok": True}


@router.get("/motion/media/{item}")
async def media_get(item: int):
    meta = await _meta()
    return await M.media_for(meta, item)


@router.post("/motion/media")
async def media_set(body: dict = Body(...)):
    meta = await _meta()
    item = str(int(body["id"]))
    cur = M.store(meta)["media"].setdefault(item, {})
    if "poster" in body:
        if body["poster"] not in ("auto", "none", "file"):
            raise HTTPException(422, "poster is auto, none or file")
        cur["poster"] = body["poster"]
    if "time" in body:
        cur["time"] = max(0.0, min(3600.0, float(body["time"])))
    if "file" in body:
        cur["file"] = body["file"]
    M.notify("media")
    return {"ok": True}


@router.get("/motion/snapshot/{item}")
async def snapshot(item: int, scale: float = 2.0):
    try:
        data = await M.snapshot(item, max(0.5, min(4.0, scale)))
    except CoreError as exc:
        raise HTTPException(404, str(exc)) from exc
    return Response(data, media_type="image/png", headers={"Cache-Control": "no-store"})
