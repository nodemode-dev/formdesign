import os
import subprocess
import sys
import webbrowser
from pathlib import Path

from fastapi import APIRouter, Body, HTTPException, Request
from fastapi.responses import FileResponse, Response

from app.core import config
from app.engine.core import CoreError
from app.services.website import PUBLISHED_DIR, published, slugify

router = APIRouter()
sites = APIRouter()


def _url(request: Request, slug: str) -> str:
    return f"{str(request.base_url).rstrip('/')}/sites/{slug}/"


@router.get("/website/published")
async def listing(request: Request):
    return {"sites": [{**i, "url": _url(request, i["slug"]), "path": str(published.dir(i["slug"]))} for i in published.entries()],
            "active": await published.active_slug()}


@router.post("/website/publish")
async def publish(request: Request, body: dict = Body(default={})):
    try:
        rec = await published.publish(body.get("options") or {}, body.get("slug"))
    except CoreError as exc:
        raise HTTPException(400, str(exc)) from exc
    return {**rec, "url": _url(request, rec["slug"])}


@router.post("/website/unpublish")
async def unpublish(body: dict = Body(...)):
    if not published.unpublish(str(body.get("slug", ""))):
        raise HTTPException(404, "no such published site")
    return {"ok": True}


@router.get("/website/zip/{slug}")
async def download(slug: str):
    try:
        data = published.zip(slug)
    except CoreError as exc:
        raise HTTPException(404, str(exc)) from exc
    return Response(data, media_type="application/zip", headers={"Content-Disposition": f'attachment; filename="{slugify(slug)}.zip"', "Cache-Control": "no-store"})


@router.post("/system/open")
async def open_target(body: dict = Body(...)):
    url = str(body.get("url") or "")
    if url:
        if not url.startswith(("http://", "https://")):
            raise HTTPException(400, "only web addresses can be opened")
        webbrowser.open(url)
        return {"ok": True}
    target = Path(str(body.get("path") or "")).expanduser().resolve()
    root = config.DATA_DIR.resolve()
    if not target.is_dir() or (target != root and root not in target.parents):
        raise HTTPException(400, "only folders inside the formdes data folder can be opened")
    if sys.platform == "darwin":
        subprocess.Popen(["open", str(target)])
    elif os.name == "nt":
        os.startfile(str(target))
    else:
        subprocess.Popen(["xdg-open", str(target)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return {"ok": True}


@sites.get("/sites/{slug}/{path:path}", include_in_schema=False)
async def serve(slug: str, path: str = ""):
    root = published.dir(slug).resolve()
    if not root.is_dir() or root.parent != PUBLISHED_DIR.resolve():
        raise HTTPException(404, "no such published site")
    target = (root / (path or "index.html")).resolve()
    if target.is_dir():
        target = target / "index.html"
    if root not in target.parents or not target.is_file():
        missing = root / "404.html"
        if missing.is_file():
            return FileResponse(missing, status_code=404, headers={"Cache-Control": "no-cache"})
        raise HTTPException(404, "not found")
    return FileResponse(target, headers={"Cache-Control": "no-cache"})


@sites.get("/sites/{slug}", include_in_schema=False)
async def serve_root(slug: str):
    return Response(status_code=307, headers={"Location": f"/sites/{slug}/"})
