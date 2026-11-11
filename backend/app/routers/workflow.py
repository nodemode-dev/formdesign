import base64
from pathlib import Path

from fastapi import APIRouter, Body, File, Form, HTTPException, UploadFile
from fastapi.responses import Response

from app.engine.core import CoreError
from app.engine.documents import documents
from app.routers.documents import attachment
from app.services import workflow as W

router = APIRouter()


def _fail(exc: Exception) -> HTTPException:
    return HTTPException(422, str(exc))


@router.get("/workflow")
async def overview():
    try:
        return await W.overview()
    except CoreError as exc:
        if "no document" in str(exc):
            return {"user": dict(W.user), "docId": None, "assignments": [], "stories": [], "package": None}
        raise _fail(exc) from exc


@router.post("/workflow/user")
async def set_user(body: dict = Body(...)):
    return W.set_user(str(body.get("name") or ""), str(body.get("role") or "designer"))


@router.post("/workflow/assignment")
async def save_assignment(body: dict = Body(...)):
    try:
        return await W.save_assignment(body)
    except CoreError as exc:
        raise _fail(exc) from exc


@router.post("/workflow/assignment/delete")
async def delete_assignment(body: dict = Body(...)):
    try:
        W.delete_assignment(await W.active(), str(body.get("id")))
    except CoreError as exc:
        raise _fail(exc) from exc
    return {"ok": True}


@router.post("/workflow/checkout")
async def checkout(body: dict = Body(...)):
    try:
        who = body.get("by") if W.user["role"] == "designer" else None
        return {"checkedOut": await W.checkout([int(s) for s in body.get("stories") or []], who)}
    except CoreError as exc:
        raise _fail(exc) from exc


@router.post("/workflow/checkin")
async def checkin(body: dict = Body(...)):
    try:
        return {"checkedIn": await W.checkin([int(s) for s in body.get("stories") or []], bool(body.get("force")))}
    except CoreError as exc:
        raise _fail(exc) from exc


@router.post("/workflow/assignment/export")
async def export_assignment(body: dict = Body(...)):
    try:
        data, manifest = await W.build_package(str(body.get("id")))
    except CoreError as exc:
        raise _fail(exc) from exc
    name = "".join(c if c.isalnum() or c in "-_ " else "_" for c in manifest["assignment"]["name"]).strip() or "assignment"
    if body.get("path"):
        p = Path(str(body["path"])).expanduser()
        if p.suffix.lower() != W.EXT:
            p = p.with_name(p.name + W.EXT)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)
        return {"path": str(p), "bytes": len(data), "stories": len(manifest["stories"]), "pages": len(manifest["pages"])}
    return Response(data, media_type="application/zip", headers={"Content-Disposition": attachment(f"{name}{W.EXT}"), "Cache-Control": "no-store"})


@router.post("/workflow/assignment/return")
async def return_assignment(body: dict = Body(default={})):
    try:
        data, manifest = await W.return_package()
    except CoreError as exc:
        raise _fail(exc) from exc
    if body.get("path"):
        p = Path(str(body["path"])).expanduser()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)
        return {"path": str(p), "bytes": len(data)}
    name = "".join(c if c.isalnum() or c in "-_ " else "_" for c in manifest["assignment"]["name"]).strip() or "assignment"
    return Response(data, media_type="application/zip", headers={"Content-Disposition": attachment(f"{name}-edited{W.EXT}"), "Cache-Control": "no-store"})


async def _bytes(file: UploadFile | None, path: str | None) -> bytes:
    if file is not None:
        return await file.read()
    if path:
        p = Path(path).expanduser()
        if not p.is_file():
            raise HTTPException(404, f"no file at {p}")
        return p.read_bytes()
    raise HTTPException(422, "send the package as `file` or its `path`")


@router.post("/workflow/assignment/open")
async def open_assignment(file: UploadFile | None = File(default=None), path: str | None = Form(default=None)):
    try:
        return await W.open_package(await _bytes(file, path))
    except CoreError as exc:
        raise _fail(exc) from exc


@router.post("/workflow/assignment/update")
async def update_content(file: UploadFile | None = File(default=None), path: str | None = Form(default=None), force: bool = Form(default=False), keep_locked: bool = Form(default=False)):
    try:
        return await W.update_content(await _bytes(file, path), force, keep_locked)
    except CoreError as exc:
        raise _fail(exc) from exc
