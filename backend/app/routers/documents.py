import hashlib
from pathlib import Path

from fastapi import APIRouter, Body, File, HTTPException, UploadFile
from fastapi.responses import Response

from app.core import config
from app.engine.core import CoreError, core
from app.engine.documents import EXPORTS, documents

router = APIRouter()


def _fail(exc: Exception) -> HTTPException:
    return HTTPException(422, str(exc))


@router.get("/documents")
async def list_documents():
    try:
        return await documents.overview()
    except CoreError as exc:
        raise _fail(exc) from exc


@router.get("/documents/recent")
async def recent():
    return documents.recent()


@router.post("/files")
async def upload_files(files: list[UploadFile] = File(...)):
    out = []
    total = 0
    for f in files:
        data = await f.read()
        total += len(data)
        if total > config.MAX_UPLOAD_BYTES:
            raise HTTPException(413, "the upload is larger than 512 MB")
        digest = hashlib.sha1(data).hexdigest()[:16]
        name = "".join(c if (c.isalnum() or c in "._- ") else "_" for c in Path(f.filename or "file").name).strip() or "file"
        target = config.INTAKE_DIR / "uploads" / digest / name
        target.parent.mkdir(parents=True, exist_ok=True)
        if not target.exists():
            target.write_bytes(data)
        out.append({"name": name, "path": str(target), "bytes": len(data)})
    return {"files": out}


@router.post("/documents/new")
async def new(params: dict = Body(default={})):
    try:
        return await documents.new(params, source="window")
    except CoreError as exc:
        raise _fail(exc) from exc


@router.post("/documents/open")
async def open_path(body: dict = Body(...)):
    try:
        return await documents.open_path(str(body.get("path", "")), source="window")
    except CoreError as exc:
        raise _fail(exc) from exc


@router.post("/documents/upload")
async def upload(file: UploadFile = File(...)):
    data = await file.read()
    if len(data) > config.MAX_UPLOAD_BYTES:
        raise HTTPException(413, "the file is larger than 512 MB")
    try:
        return await documents.open_bytes(file.filename or "Untitled.fdoc", data, source="window")
    except CoreError as exc:
        raise _fail(exc) from exc


@router.post("/documents/save")
async def save(body: dict = Body(default={})):
    try:
        return await documents.save(body.get("path"), source="window")
    except CoreError as exc:
        raise _fail(exc) from exc


@router.post("/documents/export")
async def export(body: dict = Body(...)):
    fmt = str(body.get("format", "pdf")).lower()
    try:
        info, data, mime = await documents.export(fmt, body.get("path"), body.get("options") or {}, source="window")
    except CoreError as exc:
        raise _fail(exc) from exc
    if data is None:
        return info
    ext = EXPORTS.get(fmt, (None, f".{fmt}"))[1] if fmt in EXPORTS else (".jpg" if fmt in ("jpg", "jpeg") else " site.zip" if fmt in ("web", "website", "site") else f".{fmt}")
    if mime == "application/zip" and fmt == "html":
        ext = ".zip"
    state = await documents.overview()
    title =Path(state["docs"][state["active"]]["title"]).stem if state.get("active") is not None else "formdes"
    return Response(data, media_type=mime, headers={"Content-Disposition": f'attachment; filename="{title}{ext}"', "Cache-Control": "no-store"})


@router.post("/documents/activate")
async def activate(body: dict = Body(...)):
    try:
        await core.call("docs.activate", {"index": int(body.get("index", 0))}, source="window")
        return await documents.overview()
    except CoreError as exc:
        raise _fail(exc) from exc


@router.post("/documents/close")
async def close(body: dict = Body(default={})):
    try:
        params = {"index": int(body["index"])} if "index" in body else {}
        await core.call("docs.close", params, source="window")
        return await documents.overview()
    except CoreError as exc:
        raise _fail(exc) from exc
