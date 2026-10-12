import asyncio

from fastapi import APIRouter, Body, HTTPException
from fastapi.responses import FileResponse

from app.engine.core import CoreError
from app.services import cloudfonts as cf

router = APIRouter(prefix="/cloudfonts")


def _fail(exc: Exception, code: int = 400):
    raise HTTPException(code, str(exc))


@router.get("")
async def listing(q: str = "", category: str = "", tag: str = "", state: str = "", limit: int = 80, offset: int = 0):
    try:
        return await asyncio.to_thread(cf.listing, q, category, tag, state, max(1, min(limit, 200)), max(0, offset))
    except cf.CloudFontError as exc:
        _fail(exc, 502)


@router.post("/refresh")
async def refresh():
    try:
        data = await asyncio.to_thread(cf.index, True)
    except cf.CloudFontError as exc:
        _fail(exc, 502)
    return {"count": len(data["families"]), "fetched": data["fetched"]}


@router.get("/active")
async def active():
    return {"active": list(cf.activated().values()), "folder": str(cf.CLOUD_DIR)}


@router.get("/family/{family}")
async def family(family: str):
    try:
        return await asyncio.to_thread(cf.detail, family)
    except cf.CloudFontError as exc:
        _fail(exc, 404 if "not in" in str(exc) else 502)


@router.get("/preview/{family}")
async def preview(family: str):
    try:
        path = await asyncio.to_thread(cf.preview_file, family)
    except cf.CloudFontError as exc:
        _fail(exc, 404 if "not in" in str(exc) else 502)
    return FileResponse(path, media_type="font/ttf", headers={"Cache-Control": "max-age=86400"})


@router.post("/activate")
async def activate(body: dict = Body(...)):
    name = str(body.get("family") or "")
    if not name:
        raise HTTPException(400, "family is required")
    try:
        return await cf.activate(name, body.get("doc") or None)
    except (cf.CloudFontError, CoreError) as exc:
        _fail(exc, 502)


@router.post("/deactivate")
async def deactivate(body: dict = Body(...)):
    try:
        return cf.deactivate(str(body.get("slug") or ""))
    except cf.CloudFontError as exc:
        _fail(exc)


@router.post("/missing")
async def missing(body: dict = Body(...)):
    try:
        return await cf.missing(str(body.get("doc") or ""))
    except (cf.CloudFontError, CoreError) as exc:
        _fail(exc, 502)
