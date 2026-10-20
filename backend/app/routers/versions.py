import asyncio
import logging
import os
import subprocess
import sys
import time
from pathlib import Path

from fastapi import APIRouter, Body, HTTPException
from fastapi.responses import Response

from app.engine.core import CoreError, core
from app.engine.documents import documents
from app.services import versions

router = APIRouter()
log = logging.getLogger("formdesign.versions")
WATCH_SECONDS = 3600
_watching: dict[tuple[int, int], asyncio.Task] = {}


def _fail(exc: Exception, code: int = 422) -> HTTPException:
    return HTTPException(code, str(exc))


async def _active() -> tuple[str, str]:
    m = await documents.meta_for_active()
    if m is None:
        raise HTTPException(422, "No document is open.")
    if not m.path or not m.path.endswith(".fdes"):
        raise HTTPException(422, "This document has not been saved yet. Use File > Save, then its versions are kept.")
    return m.path, Path(m.path).stem


@router.get("/documents/versions")
async def list_versions():
    m = await documents.meta_for_active()
    if m is None or not m.path or not m.path.endswith(".fdes"):
        return {"path": None, "title": None, "versions": []}
    return {"path": m.path, "title": Path(m.path).stem, "versions": versions.listing(m.path)}


@router.post("/documents/versions/save")
async def save_version(body: dict = Body(default={})):
    comment = str(body.get("comment") or "").strip()[:300]
    try:
        await documents.save(None, source="window", comment=comment or "Saved version")
    except CoreError as exc:
        raise _fail(exc) from exc
    path, title = await _active()
    return {"path": path, "title": title, "versions": versions.listing(path)}


@router.get("/documents/versions/{vid}/thumb")
async def version_thumb(vid: str):
    path, _ = await _active()
    try:
        png = versions.thumb(path, vid)
    except ValueError as exc:
        raise _fail(exc, 404) from exc
    if png is None:
        raise HTTPException(404, "This version has no preview.")
    return Response(png, media_type="image/png", headers={"Cache-Control": "private, max-age=3600"})


@router.post("/documents/versions/open")
async def open_copy(body: dict = Body(...)):
    path, title = await _active()
    vid = str(body.get("id") or "")
    try:
        data = versions.read(path, vid)
    except ValueError as exc:
        raise _fail(exc, 404) from exc
    e = versions.entry(path, vid) or {}
    stamp = time.strftime("%Y-%m-%d %H.%M", time.localtime((e.get("time") or 0) / 1000))
    try:
        return await documents.open_bytes(f"{title} (version {stamp}).fdes", data, source="window")
    except CoreError as exc:
        raise _fail(exc) from exc


@router.post("/documents/versions/restore")
async def restore(body: dict = Body(...)):
    path, title = await _active()
    vid = str(body.get("id") or "")
    try:
        data = versions.read(path, vid)
    except ValueError as exc:
        raise _fail(exc, 404) from exc
    try:
        # What is open now is kept first, so a restore can itself be undone from the list.
        versions.keep(path, await documents.serialize_fdes(), "Before restoring a version", "save")
        Path(path).write_bytes(data)
        st = await core.state()
        await core.call("docs.close", {"index": st["active"]}, source="window")
        return await documents.open_path(path, source="window")
    except (CoreError, OSError) as exc:
        raise _fail(exc) from exc


def _open_with_system(path: str) -> None:
    if sys.platform.startswith("win"):
        os.startfile(path)  # type: ignore[attr-defined]  # noqa: S606
        return
    tool = "open" if sys.platform == "darwin" else "xdg-open"
    subprocess.Popen([tool, path], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)  # noqa: S603


async def _watch(uid: int, asset: int, path: str) -> None:
    """Wait for the file to be saved by the other app, then update the link. The core's own modified-link check does
    the comparing; this only asks it to refresh once the file has changed and settled."""
    p = Path(path)
    try:
        last = p.stat().st_mtime_ns
    except OSError:
        return
    deadline = time.time() + WATCH_SECONDS
    pending: int | None = None
    while time.time() < deadline:
        await asyncio.sleep(1.0)
        try:
            now = p.stat().st_mtime_ns
        except OSError:
            continue
        if now != last:
            pending, last = now, now
            continue
        if pending is not None:
            pending = None
            try:
                if await documents.active_uid() != uid:
                    continue
                await core.exec("links.update", {"asset": asset}, source="engine")
            except CoreError as exc:
                log.warning("link not updated: %s", exc)


@router.post("/links/edit-original")
async def edit_original(body: dict = Body(...)):
    asset = body.get("asset")
    if not isinstance(asset, int):
        raise HTTPException(422, "Say which link to edit (asset).")
    try:
        links = await core.exec("links.list", {}, source="window")
    except CoreError as exc:
        raise _fail(exc) from exc
    link = next((l for l in links or [] if l.get("asset") == asset), None)
    if link is None:
        raise HTTPException(404, "No such link.")
    path = link.get("path")
    if not path or not Path(path).is_file():
        raise HTTPException(422, "The original file is missing. Relink it first.")
    uid = await documents.active_uid()
    try:
        await asyncio.to_thread(_open_with_system, path)
    except OSError as exc:
        raise HTTPException(422, f"Could not open the file: {exc}") from exc
    if uid is not None:
        key = (uid, asset)
        old = _watching.get(key)
        if old and not old.done():
            old.cancel()
        _watching[key] = asyncio.create_task(_watch(uid, asset, path))
    return {"path": path, "name": link.get("name"), "watching": uid is not None}
