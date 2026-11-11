import json
import re

from fastapi import APIRouter, Body, HTTPException, Query
from fastapi.responses import Response

from app.autolayout import rank as R
from app.engine.core import CoreError, core
from app.engine.documents import documents
from app.intake.store import packs
from app.templates import brands, registry, spec

router = APIRouter()
ID = re.compile(r"^[a-z0-9][a-z0-9-]{1,62}$")


@router.get("/templates")
async def list_templates(category: str | None = None, kind: str | None = None, q: str | None = None, medium: str | None = None):
    cards = registry.cards(category, kind, q, medium)
    cats: dict[str, int] = {}
    for c in registry.cards():
        cats[c["category"]] = cats.get(c["category"], 0) + 1
    return {"templates": cards, "categories": cats, "total": len(registry.templates())}


@router.get("/templates/rank")
async def rank_for_pack(pack: str, limit: int = 6, category: str | None = None):
    p = packs.get(pack)
    if p is None:
        raise HTTPException(404, f"no content pack {pack}")
    return {"ranked": R.rank(p, limit=limit, category=category)}


@router.get("/templates/{template_id}")
async def get_template(template_id: str):
    t = registry.get(template_id)
    if t is None:
        raise HTTPException(404, f"no template {template_id}")
    return {"card": spec.card(t), "spec": {k: v for k, v in t.items() if not k.startswith("_")}}


@router.get("/templates/{template_id}/thumb.png")
async def thumb(template_id: str, page: int = 0, size: int = Query(480, ge=64, le=2048), brand: str | None = None):
    if registry.get(template_id) is None:
        raise HTTPException(404, f"no template {template_id}")
    try:
        png = await registry.thumbnail(template_id, brands.get(brand) if brand else None, page, size)
    except CoreError as exc:
        raise HTTPException(422, str(exc)) from exc
    return Response(png, media_type="image/png", headers={"Cache-Control": "max-age=3600"})


@router.post("/templates/{template_id}/new")
async def new_from_template(template_id: str, body: dict = Body(default={})):
    brand = brands.resolve(body.get("brand"))
    try:
        built = await registry.build(core, template_id, brand=brand, sample=bool(body.get("sample", True)), title=body.get("title"), source="rest")
    except CoreError as exc:
        raise HTTPException(422, str(exc)) from exc
    await documents.adopt(template={"id": template_id, "name": built["rt"]["name"]}, brand=brand,
                          slots={s.get("slot") or f"flow:{s.get('flow')}": s["id"] for s in built["slots"]})
    return await documents.overview()


@router.post("/templates/validate")
async def validate(body: dict = Body(...)):
    t = body.get("spec") or body
    errs = spec.problems(t)
    if errs:
        return {"ok": False, "problems": errs}
    return await registry.check_spec(t, render=bool(body.get("render", True)), size=int(body.get("size", 640)))


@router.post("/templates")
async def save_template(body: dict = Body(...)):
    t = body.get("spec") or body
    if not ID.match(str(t.get("id", ""))):
        raise HTTPException(422, "id must be kebab-case letters, digits and hyphens")
    existing = registry.get(t["id"])
    if existing is not None and not existing.get("_user") and not body.get("overrideBuiltIn"):
        raise HTTPException(409, f"{t['id']} is a built-in template; choose another id")
    errs = spec.problems(t)
    if errs:
        raise HTTPException(422, "; ".join(errs[:6]))
    res = await registry.check_spec(t, render=False)
    if not res.get("ok"):
        raise HTTPException(422, res.get("error", "the template does not compile"))
    try:
        f = registry.user_path(t)
    except ValueError as exc:
        raise HTTPException(422, str(exc)) from exc
    f.parent.mkdir(parents=True, exist_ok=True)
    f.write_text(json.dumps({k: v for k, v in t.items() if not k.startswith("_")}, indent=2, ensure_ascii=False))
    registry.templates(refresh=True)
    return {"saved": str(f), "check": res}


@router.delete("/templates/{template_id}")
async def delete_template(template_id: str):
    t = registry.get(template_id)
    if t is None:
        raise HTTPException(404, f"no template {template_id}")
    if not t.get("_user"):
        raise HTTPException(409, "built-in templates cannot be deleted")
    f = registry.user_path(t)
    if f.is_file():
        f.unlink()
    registry.templates(refresh=True)
    return {"deleted": template_id}


@router.get("/brands")
async def list_brands():
    return {"brands": brands.listing()}


@router.post("/brands")
async def save_brand(body: dict = Body(...)):
    return brands.save(body)


@router.delete("/brands/{name}")
async def delete_brand(name: str):
    if not brands.delete(name):
        raise HTTPException(404, f"no brand {name}")
    return {"deleted": name}
