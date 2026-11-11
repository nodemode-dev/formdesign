import asyncio
import base64
import hashlib
import json
import time
from pathlib import Path

from app.autolayout.fill import image_fill, text_fill
from app.core import config
from app.engine.core import Core, CoreError
from app.templates import compile as C
from app.templates import samples, spec

_cache: dict = {"at": 0.0, "all": {}}
_aux: Core | None = None
_aux_lock = asyncio.Lock()


def templates(refresh: bool = False) -> dict[str, dict]:
    if refresh or not _cache["all"] or time.time() - _cache["at"] > 5:
        _cache["all"] = spec.load_all()
        _cache["at"] = time.time()
    return _cache["all"]


def get(template_id: str) -> dict | None:
    return templates().get(template_id)


def user_path(t: dict):
    cat = "".join(c for c in str(t.get("category", "other")).lower() if c.isalpha()) or "other"
    f = Path(t["_file"]) if t.get("_file") and t.get("_user") else config.TEMPLATES_USER_DIR / cat / f"{t['id']}.json"
    if config.TEMPLATES_USER_DIR.resolve() not in f.resolve().parents:
        raise ValueError(f"{t.get('id')}: a template is saved only inside the template library")
    return f


def cards(category: str | None = None, kind: str | None = None, query: str | None = None, medium: str | None = None) -> list[dict]:
    out = []
    q = (query or "").lower()
    for t in templates().values():
        c = spec.card(t)
        if category and c["category"] != category:
            continue
        if kind and kind not in c["kinds"]:
            continue
        if medium and c["medium"] != medium:
            continue
        if q and q not in json.dumps([c["id"], c["name"], c["tags"], c["description"], c["kinds"]]).lower():
            continue
        out.append(c)
    order = ["magazine", "newsletter", "book", "report", "print", "stationery", "ad", "web", "presentation", "other"]
    return sorted(out, key=lambda c: (order.index(c["category"]) if c["category"] in order else 99, c["name"]))


def slot_ids(builder: C.Builder, results: list) -> list[dict]:
    out = []
    for rec in builder.slots:
        r = results[rec["cmd"]] if rec["cmd"] < len(results) else None
        fid = r.get("id") if isinstance(r, dict) else None
        if fid is not None:
            out.append({**{k: v for k, v in rec.items() if k != "item"}, "id": fid, "item": rec["item"]})
    return out


def sample_fills(rt: dict, slots: list[dict]) -> list[dict]:
    cmds: list[dict] = []
    n_img = 0
    for s in slots:
        it = s["item"]
        if s["kind"] == "image":
            role = (s.get("slot") or "").lower()
            if "logo" in role:
                path = samples.logo_placeholder(rt["palette"])
            else:
                x0, y0, x1, y1 = s["rect"]
                w, h = abs(x1 - x0), abs(y1 - y0)
                orient = "square" if 0.85 < (w / max(h, 1)) < 1.18 else ("landscape" if w > h else "portrait")
                path = samples.placeholder(rt["palette"], orient, n_img)
            n_img += 1
            cmds += image_fill(s["id"], str(path), s.get("fit", "fill"))
        elif s["kind"] == "text" and s.get("flow"):
            chain = [x for x in slots if x.get("flow") == s["flow"] and x["kind"] == "text"]
            if chain[0] is not s:
                continue
            paras = []
            for x in chain:
                smp = x["item"].get("sample")
                for t in ([smp] if isinstance(smp, str) else smp or []):
                    paras.append({"style": x.get("style") or "Body", "text": str(t)})
            if not paras:
                style = s.get("style")
                paras = [{"style": "Body First" if k == 0 and "Body First" in rt["styleNames"] else (style or "Body"), "text": t} for k, t in enumerate(samples.PARAGRAPHS * 3)]
            cmds += text_fill(s["id"], paras, s["flow"])
        elif s["kind"] == "text":
            if it.get("text") or it.get("sample"):
                continue
            name = s.get("slot") or "text"
            sample = samples.sample_text(name, None)
            paras = [{"style": s.get("style"), "text": t} for t in sample] if isinstance(sample, list) else [{"style": s.get("style"), "text": sample}]
            cmds += text_fill(s["id"], paras, name)
    cmds.append({"command": "edit.deselectAll", "params": {}})
    return cmds


async def build(core: Core, template_id: str, *, brand: dict | None = None, sample: bool = True, title: str | None = None, source: str = "engine") -> dict:
    t = get(template_id)
    if t is None:
        raise CoreError(f"no template {template_id}: list them with list_templates")
    return await build_spec(core, t, brand=brand, sample=sample, title=title, source=source)


async def build_spec(core: Core, t: dict, *, brand: dict | None = None, sample: bool = True, title: str | None = None, source: str = "engine") -> dict:
    template_id = t["id"]
    rt = spec.resolve(t, brand)
    b = C.structure(rt, title)
    res = await core.batch(b.cmds, source=source)
    results = res.get("results", [])
    for pair in b.threads(results):
        await core.call("frame.thread", pair, source=source)
    slots = slot_ids(b, results)
    if sample:
        fills = sample_fills(rt, slots)
        if fills:
            await core.batch(fills, source=source)
    return {"template": template_id, "rt": rt, "slots": slots, "commands": len(b.cmds)}


async def aux_core() -> Core:
    global _aux
    async with _aux_lock:
        if _aux is None or not _aux.alive():
            c = Core()
            await c.start()
            _aux = c
        return _aux


def thumb_path(template_id: str, brand: dict | None, page: int, size: int) -> object:
    t = get(template_id) or {}
    key = hashlib.sha1(json.dumps([t, brand, page, size, spec.available_fonts()], sort_keys=True, default=str).encode()).hexdigest()[:16]
    return config.CACHE_DIR / "thumbs" / f"{template_id}-{page}-{size}-{key}.png"


async def thumbnail(template_id: str, brand: dict | None = None, page: int = 0, size: int = 480) -> bytes:
    out = thumb_path(template_id, brand, page, size)
    if out.is_file():
        return out.read_bytes()
    core = await aux_core()
    async with _aux_lock:
        await build(core, template_id, brand=brand, sample=True, source="thumbs")
        try:
            r = await core.call("page.png", {"page": page, "maxSide": size}, source="thumbs")
        finally:
            await core.call("docs.close", {}, source="thumbs")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(r.bin or b"")
    return r.bin or b""


async def check_spec(t: dict, *, render: bool = True, size: int = 640, pages: int = 4) -> dict:
    core = await aux_core()
    rep: dict = {"id": t.get("id"), "ok": True}
    async with _aux_lock:
        try:
            built = await build_spec(core, t, sample=True, source="check")
            ins = await core.inspect()
            rep["pages"] = ins.get("pageCount")
            rep["slots"] = sorted({s["slot"] for s in built["slots"] if s.get("slot")})
            rep["flows"] = sorted({s["flow"] for s in built["slots"] if s.get("flow")})
            flow_stories = set()
            for s in built["slots"]:
                if s.get("flow"):
                    info = (await core.call("exec", {"command": "story.get", "params": {"frame": s["id"]}}, source="check")).result or {}
                    flow_stories.add(info.get("id"))
            rep["overset"] = [s.get("id") for s in ins.get("stories") or [] if s.get("overset") and s.get("id") not in flow_stories]
            pf = await core.exec("preflight.run", {}, source="check")
            issues = pf if isinstance(pf, list) else (pf or {}).get("issues", [])
            rep["preflight"] = [f"{i.get('severity')}: {i.get('message')}" for i in issues[:10]]
            if render:
                rep["renders"] = []
                for p in range(min(rep["pages"] or 1, pages)):
                    r = await core.call("page.png", {"page": p, "maxSide": size}, source="check")
                    rep["renders"].append(base64.b64encode(r.bin or b"").decode())
        except CoreError as exc:
            rep["ok"] = False
            rep["error"] = str(exc)
        finally:
            try:
                await core.call("docs.close", {}, source="check")
            except CoreError:
                pass
    return rep
