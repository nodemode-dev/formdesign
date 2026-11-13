import asyncio
import json
import shutil
from pathlib import Path

from fastapi import APIRouter, HTTPException

from app.engine.core import CoreError
from app.engine.documents import documents
from app.intake import pack as pack_mod
from app.intake.store import new_pack_id, packs
from app.routers.make import make_one
from app.templates import samples

router = APIRouter()
MATERIAL = Path(__file__).resolve().parent.parent / "demos"
_busy = asyncio.Lock()


def demos() -> dict[str, dict]:
    return json.loads((MATERIAL / "demos.json").read_text(encoding="utf-8"))


def _gather(d: dict, pack_id: str) -> list[Path]:
    dest = packs.dir(pack_id) / "files"
    dest.mkdir(parents=True, exist_ok=True)
    pics = []
    for n, (name, orientation) in enumerate(d.get("pictures") or []):
        target = dest / name
        shutil.copyfile(samples.placeholder(d["brand"]["palette"], orientation, n), target)
        pics.append(target)
    if not d.get("material"):
        return pics
    md = dest / d["material"]
    shutil.copyfile(MATERIAL / d["material"], md)
    return [md]


@router.get("/demos")
async def list_demos():
    return {"demos": [{"id": k, "title": v["title"], "template": v["template"]} for k, v in demos().items()]}


@router.post("/demos/{demo_id}")
async def open_demo(demo_id: str):
    d = demos().get(demo_id)
    if d is None:
        raise HTTPException(404, f"no demo {demo_id}")
    if _busy.locked():
        raise HTTPException(409, "a demo is already opening")
    async with _busy:
        pack_id = new_pack_id()
        paths = await asyncio.to_thread(_gather, d, pack_id)
        pack = await asyncio.to_thread(pack_mod.ingest, paths, d["brief"], {}, pack_id)
        try:
            rep = await make_one(pack, d["template"], d["brand"], d.get("fills"), {"title": d["title"]})
        except CoreError as exc:
            raise HTTPException(422, str(exc)) from exc
    return {"demo": demo_id, "report": rep, "documents": await documents.overview()}
