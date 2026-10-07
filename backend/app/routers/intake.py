import asyncio
import json
import mimetypes
from pathlib import Path

from fastapi import APIRouter, Body, File, Form, HTTPException, UploadFile
from fastapi.responses import FileResponse

from app.core import config
from app.intake import pack as pack_mod
from app.intake.store import new_pack_id, packs

router = APIRouter()


def _safe_name(name: str) -> str:
    keep = "".join(c if (c.isalnum() or c in "._- ") else "_" for c in Path(name).name)
    return keep.strip() or "file"


@router.post("/intake")
async def intake_upload(files: list[UploadFile] = File(default=[]), brief: str = Form(default=""), options: str = Form(default="{}")):
    pack_id = new_pack_id()
    dest = packs.dir(pack_id) / "files"
    dest.mkdir(parents=True, exist_ok=True)
    paths = []
    total = 0
    for f in files:
        data = await f.read()
        total += len(data)
        if total > config.MAX_UPLOAD_BYTES:
            raise HTTPException(413, "the upload is larger than 512 MB")
        rel = Path(f.filename or "file").parts
        target = dest.joinpath(*[_safe_name(p) for p in rel]) if len(rel) > 1 else dest / _safe_name(f.filename or "file")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        paths.append(target)
    try:
        opts = json.loads(options or "{}")
    except ValueError:
        opts = {}
    pack = await asyncio.to_thread(pack_mod.ingest, paths, brief, opts, pack_id)
    return pack_mod.summary(pack)


@router.post("/intake/paths")
async def intake_paths(body: dict = Body(...)):
    paths = [Path(p).expanduser() for p in body.get("paths", [])]
    missing = [str(p) for p in paths if not p.exists()]
    if missing:
        raise HTTPException(422, f"not found: {', '.join(missing[:5])}")
    pack = await asyncio.to_thread(pack_mod.ingest, paths, body.get("brief", ""), body.get("options") or {})
    return pack_mod.summary(pack)


@router.get("/intake")
async def list_packs():
    return packs.listing()


@router.get("/intake/{pack_id}")
async def get_pack(pack_id: str, detail: str = "summary"):
    pack = packs.get(pack_id)
    if not pack:
        raise HTTPException(404, f"no content pack {pack_id}")
    return pack if detail == "full" else pack_mod.summary(pack, detail)


@router.patch("/intake/{pack_id}")
async def update_pack(pack_id: str, body: dict = Body(...)):
    pack = packs.get(pack_id)
    if not pack:
        raise HTTPException(404, f"no content pack {pack_id}")
    for key in ("title", "subtitle", "author", "date"):
        if key in body:
            pack[key] = str(body[key])
    if "brief" in body:
        from app.intake import brief as brief_mod
        pack["brief"] = brief_mod.parse(str(body["brief"]), body.get("options") or {})
    for img in pack.get("images", []):
        edits = (body.get("images") or {}).get(img["id"])
        if isinstance(edits, dict):
            for k in ("caption", "alt", "role", "focal"):
                if k in edits:
                    img[k] = edits[k]
    packs.put(pack)
    return pack_mod.summary(pack)


@router.get("/intake/{pack_id}/image/{ref}")
async def pack_image(pack_id: str, ref: str):
    pack = packs.get(pack_id)
    img = next((i for i in (pack or {}).get("images", []) if i["id"] == ref), None)
    if not img or not Path(img["path"]).is_file():
        raise HTTPException(404, "no such image")
    return FileResponse(img["path"], media_type=mimetypes.guess_type(img["path"])[0] or "application/octet-stream")
