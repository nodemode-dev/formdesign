import asyncio
import json
from pathlib import Path

from fastapi import APIRouter, Body, File, Form, HTTPException, UploadFile
from fastapi.responses import Response

from app.autolayout import make as M
from app.autolayout import rank as R
from app.autolayout import slots as S
from app.engine.core import CoreError, core
from app.engine.documents import documents
from app.intake import pack as pack_mod
from app.intake.store import new_pack_id, packs
from app.routers.intake import _safe_name
from app.templates import brands, registry

router = APIRouter()


async def make_one(pack: dict, template: str | None, brand, fills: dict | None, options: dict) -> dict:
    b = brands.resolve(brand)
    rep = await M.make(core, pack, template, brand=b, fills=fills, options=options, source="rest")
    t = registry.get(rep["template"]) or {}
    await documents.adopt(template={"id": rep["template"], "name": t.get("name")}, brand=b, brief=(pack.get("brief") or {}).get("text", ""),
                          pack_id=pack["id"], slots=rep.pop("slots", {}))
    return rep


@router.post("/make")
async def make(body: dict = Body(...)):
    pack = packs.get(body.get("pack"))
    if pack is None:
        raise HTTPException(404, f"no content pack {body.get('pack')}: run intake first")
    options = body.get("options") or {}
    n = max(1, min(int(body.get("variations", 1)), 4))
    try:
        if n == 1:
            return await make_one(pack, body.get("template"), body.get("brand"), body.get("fills"), options)
        picks = [body["template"]] if body.get("template") else []
        picks += [r["id"] for r in R.rank(pack, limit=n + 1, category=options.get("category")) if r["id"] not in picks]
        out = [await make_one(pack, tid, body.get("brand"), body.get("fills"), options) for tid in picks[:n]]
        return {"variations": out, "documents": await documents.overview()}
    except CoreError as exc:
        raise HTTPException(422, str(exc)) from exc


@router.post("/make/quick")
async def make_quick(files: list[UploadFile] = File(default=[]), brief: str = Form(default=""), template: str = Form(default=""),
                     brand: str = Form(default=""), options: str = Form(default="{}")):
    pack_id = new_pack_id()
    dest = packs.dir(pack_id) / "files"
    dest.mkdir(parents=True, exist_ok=True)
    paths: list[Path] = []
    for f in files:
        target = dest / _safe_name(f.filename or "file")
        target.write_bytes(await f.read())
        paths.append(target)
    try:
        opts = json.loads(options or "{}")
    except ValueError:
        opts = {}
    pack = await asyncio.to_thread(pack_mod.ingest, paths, brief, opts, pack_id=pack_id)
    try:
        rep = await make_one(pack, template or None, brand or None, None, opts)
    except CoreError as exc:
        raise HTTPException(422, str(exc)) from exc
    return {"pack": pack_mod.summary(pack), "report": rep}


@router.post("/make/preview")
async def preview(body: dict = Body(...)):
    pack = packs.get(body.get("pack"))
    if pack is None:
        raise HTTPException(404, f"no content pack {body.get('pack')}")
    aux = await registry.aux_core()
    async with registry._aux_lock:
        try:
            await M.make(aux, pack, body.get("template"), brand=brands.resolve(body.get("brand")), fills=body.get("fills"),
                         options=body.get("options") or {}, source="preview")
            r = await aux.call("page.png", {"page": int(body.get("page", 0)), "maxSide": int(body.get("size", 640))}, source="preview")
        except CoreError as exc:
            raise HTTPException(422, str(exc)) from exc
        finally:
            try:
                await aux.call("docs.close", {}, source="preview")
            except CoreError:
                pass
    return Response(r.bin or b"", media_type="image/png")


@router.get("/slots")
async def get_slots(full: bool = False):
    try:
        s = await S.list_slots(core, "rest")
    except CoreError as exc:
        raise HTTPException(422, str(exc)) from exc
    return {"slots": s if full else S.compact(s)}


@router.post("/slots")
async def set_slots(body: dict = Body(...)):
    values = body.get("values") or {k: v for k, v in body.items() if k != "values"}
    m = await documents.meta_for_active()
    pack = packs.get(m.pack_id) if m and m.pack_id else None
    try:
        return await S.set_slots(core, values, pack, "rest")
    except CoreError as exc:
        raise HTTPException(422, str(exc)) from exc
