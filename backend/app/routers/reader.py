import asyncio
import hashlib
import shutil
from pathlib import Path

from fastapi import APIRouter, Body, File, HTTPException, UploadFile
from fastapi.responses import Response

from app.core import config
from app.intake import pack as pack_mod
from app.services import pdfread

router = APIRouter()
READER_DIR = config.INTAKE_DIR / "reader"


def _path(doc_id: str) -> Path:
    p = READER_DIR / f"{''.join(c for c in doc_id if c.isalnum())}.pdf"
    if not p.is_file():
        raise HTTPException(404, f"no open PDF {doc_id}")
    return p


def _register(src: Path | None = None, data: bytes | None = None, name: str = "document.pdf") -> dict:
    READER_DIR.mkdir(parents=True, exist_ok=True)
    blob = data if data is not None else src.read_bytes()
    if not blob.startswith(b"%PDF"):
        raise HTTPException(422, f"{name} is not a PDF")
    doc_id = hashlib.sha1(blob).hexdigest()[:16]
    target = READER_DIR / f"{doc_id}.pdf"
    if not target.is_file():
        if src is not None:
            shutil.copyfile(src, target)
        else:
            target.write_bytes(blob)
    (READER_DIR / f"{doc_id}.name").write_text(name)
    return {"id": doc_id, "name": name, **pdfread.info(str(target))}


@router.post("/reader/open")
async def open_pdf(body: dict = Body(...)):
    p = Path(str(body.get("path", ""))).expanduser()
    if not p.is_file():
        raise HTTPException(422, f"no file at {p}")
    return await asyncio.to_thread(_register, p, None, p.name)


@router.post("/reader/upload")
async def upload_pdf(file: UploadFile = File(...)):
    data = await file.read()
    return await asyncio.to_thread(_register, None, data, file.filename or "document.pdf")


@router.get("/reader/{doc_id}")
async def info(doc_id: str):
    p = _path(doc_id)
    name_file = READER_DIR / f"{doc_id}.name"
    return {"id": doc_id, "name": name_file.read_text() if name_file.is_file() else p.name, **(await asyncio.to_thread(pdfread.info, str(p)))}


@router.get("/reader/{doc_id}/page/{page}.png")
async def page(doc_id: str, page: int, maxSide: float = 1400, scale: float | None = None):
    p = _path(doc_id)
    try:
        data = await asyncio.to_thread(pdfread.render, str(p), page, scale, maxSide)
    except IndexError as exc:
        raise HTTPException(404, "no such page") from exc
    return Response(data, media_type="image/png", headers={"Cache-Control": "max-age=3600"})


@router.get("/reader/{doc_id}/text/{page}")
async def text(doc_id: str, page: int):
    return await asyncio.to_thread(pdfread.text_layer, str(_path(doc_id)), page)


@router.get("/reader/{doc_id}/plain/{page}")
async def plain(doc_id: str, page: int):
    return {"page": page, "text": await asyncio.to_thread(pdfread.page_text, str(_path(doc_id)), page)}


@router.get("/reader/{doc_id}/search")
async def search(doc_id: str, q: str):
    return await asyncio.to_thread(pdfread.search, str(_path(doc_id)), q)


@router.get("/reader/{doc_id}/structure")
async def structure(doc_id: str):
    return await asyncio.to_thread(pdfread.structure, str(_path(doc_id)))


@router.post("/reader/{doc_id}/extract")
async def extract(doc_id: str, body: dict = Body(default={})):
    p = _path(doc_id)
    name_file = READER_DIR / f"{doc_id}.name"
    name = name_file.read_text() if name_file.is_file() else p.name
    work = config.INTAKE_DIR / "reader" / f"{doc_id}-src"
    work.mkdir(parents=True, exist_ok=True)
    named = work / Path(name).name
    if not named.is_file():
        shutil.copyfile(p, named)
    pack = await asyncio.to_thread(pack_mod.ingest, [named], body.get("brief", ""), body.get("options") or {})
    return pack_mod.summary(pack)
