import math
import re
import time

from app.autolayout import rank as R
from app.autolayout.content import LONG, Content, parse_slot, sentences
from app.autolayout.fill import image_fill, text_fill
from app.engine.core import Core, CoreError
from app.templates import compile as C
from app.templates import registry, samples

IMAGE_PRIORITY = ["hero", "background", "product", "portrait", "image", "gallery"]


def luminance(hx: str) -> float:
    r, g, b = (int(hx[i:i + 2], 16) / 255 for i in (1, 3, 5))
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def brand_from_brief(pack: dict) -> dict | None:
    brief = pack.get("brief") or {}
    cols = [c for c in brief.get("colors") or [] if isinstance(c, str) and len(c) == 7]
    pal: dict = {}
    accents = [c for c in cols if 0.12 < luminance(c) < 0.9]
    darks = [c for c in cols if luminance(c) <= 0.12]
    lights = [c for c in cols if 0.9 <= luminance(c) < 0.995]
    if lights and re.search(r"\b(paper|background|stock)\b", brief.get("text") or "", re.I):
        pal["paper"] = lights[0]
    if accents:
        pal["accent"] = accents[0]
    if len(accents) > 1:
        pal["accent2"] = accents[1]
    if darks:
        pal["dark"] = darks[0]
    if not pal and not brief.get("brand"):
        return None
    return {"name": brief.get("brand"), "palette": pal, "fonts": {}}


def style_metrics(rt: dict, name: str | None) -> tuple[float, float | None]:
    by = {p["name"]: p for p in rt.get("paraStyles") or []}
    size, leading, seen = None, None, set()
    cur = by.get(name or "")
    while cur and cur["name"] not in seen:
        seen.add(cur["name"])
        ch = cur.get("chars") or {}
        if size is None and "size" in ch:
            size = float(ch["size"])
        if leading is None and isinstance(ch.get("leading"), dict) and ch["leading"].get("kind") == "points":
            leading = float(ch["leading"]["value"])
        cur = by.get(cur.get("basedOn") or "")
    return size or float(rt.get("base", 10)), leading


def slot_aspect(rec: dict) -> float:
    x0, y0, x1, y1 = rec["rect"]
    return max(abs(x1 - x0), 1) / max(abs(y1 - y0), 1)


def slot_area(rec: dict) -> float:
    x0, y0, x1, y1 = rec["rect"]
    return abs(x1 - x0) * abs(y1 - y0)


def image_order(pack: dict) -> list[dict]:
    refs: list[str] = []
    for a in pack.get("articles") or []:
        for b in a["blocks"]:
            if b["type"] == "image" and b["ref"] not in refs:
                refs.append(b["ref"])
    imgs = [i for i in pack.get("images") or [] if i.get("role") != "logo"]
    return sorted(imgs, key=lambda i: (refs.index(i["id"]) if i["id"] in refs else 99, 0 if i.get("role") == "hero" else 1, -i.get("megapixels", 0)))


def assign_images(rt: dict, recs: list[dict], pack: dict, placeholders: bool) -> dict:
    imgs = image_order(pack)
    logos = [i for i in pack.get("images") or [] if i.get("role") == "logo"]
    used: set[str] = set()
    by_name: dict[str, dict] = {}
    out: dict[int, dict] = {}

    def prio(r: dict) -> tuple:
        base, n, _ = parse_slot(r.get("slot") or "")
        return (IMAGE_PRIORITY.index(base) if base in IMAGE_PRIORITY else len(IMAGE_PRIORITY), n or 0, -slot_area(r))

    n_ph = 0
    for r in sorted(recs, key=prio):
        slot = r.get("slot") or ""
        if slot in by_name:
            out[r["id"]] = by_name[slot]
            continue
        base, _, _ = parse_slot(slot)
        choice = None
        if "logo" in slot:
            if logos:
                choice = {"image": logos[0], "fit": "fit"}
            elif placeholders:
                choice = {"placeholder": str(samples.logo_placeholder(rt["palette"])), "fit": "fit"}
        else:
            sa = slot_aspect(r)
            best, best_score = None, -1e9
            for k, img in enumerate(imgs):
                if img["id"] in used:
                    continue
                s = -2.0 * abs(math.log(max(img.get("aspect", 1.0), 0.01) / sa)) - 0.15 * k
                if max(img.get("width", 0), img.get("height", 0)) < 400:
                    s -= 3.0
                if img.get("role") == base:
                    s += 1.5
                if C.is_print(rt):
                    w_in = max(abs(r["rect"][2] - r["rect"][0]), 1) / 72
                    if img.get("width", 0) / w_in < 150:
                        s -= 1.0
                if s > best_score:
                    best, best_score = img, s
            if best is not None:
                used.add(best["id"])
                choice = {"image": best, "fit": r.get("fit", "fill")}
            elif placeholders:
                orient = "square" if 0.85 < sa < 1.18 else ("landscape" if sa > 1 else "portrait")
                choice = {"placeholder": str(samples.placeholder(rt["palette"], orient, n_ph)), "fit": r.get("fit", "fill")}
                n_ph += 1
        if choice:
            out[r["id"]] = choice
            by_name[slot] = choice
    return out


async def run(core: Core, cmds: list[dict], source: str) -> str | None:
    try:
        await core.batch(cmds, source=source)
        return None
    except CoreError as exc:
        return str(exc)[:160]


async def story(core: Core, frame: int, source: str) -> dict:
    try:
        return (await core.call("exec", {"command": "story.get", "params": {"frame": frame}}, source=source)).result or {}
    except CoreError:
        return {}


async def fit_text(core: Core, rt: dict, rec: dict, paras: list[str], source: str) -> dict | None:
    fid, slot, style = rec["id"], rec.get("slot") or "", rec.get("style")
    info = await story(core, fid, source)
    if not info.get("overset"):
        return None
    base, _, sub = parse_slot(slot)
    prose = base in LONG or sub in ("body",)
    if prose:
        units = paras if len(paras) > 1 else sentences(paras[0] if paras else "")
        sep = None if len(paras) > 1 else " "

        def body(k: int) -> list[dict]:
            if sep is None:
                return [{"style": style, "text": p} for p in units[:k]]
            return [{"style": style, "text": sep.join(units[:k])}]

        lo, hi, best = 1, max(len(units) - 1, 1), None
        while lo <= hi and len(units) > 1:
            mid = (lo + hi) // 2
            await run(core, text_fill(fid, body(mid), slot), source)
            if (await story(core, fid, source)).get("overset"):
                hi = mid - 1
            else:
                best, lo = mid, mid + 1
        if best:
            await run(core, text_fill(fid, body(best), slot), source)
            return {"trimmed": f"{best}/{len(units)} {'paragraphs' if sep is None else 'sentences'}"}
        await run(core, text_fill(fid, [{"style": style, "text": p} for p in paras], slot), source)
    size, leading = style_metrics(rt, style)
    for k in (0.92, 0.85, 0.78) if prose else (0.92, 0.85, 0.78, 0.7, 0.62, 0.55):
        info = await story(core, fid, source)
        total = len((info.get("text") or "").encode())
        attrs = {"size": round(size * k, 2)}
        if leading:
            attrs["leading"] = {"kind": "points", "value": round(leading * k, 2)}
        err = await run(core, [{"command": "text.select", "params": {"story": info.get("id"), "anchor": 0, "focus": total}},
                               {"command": "type.char", "params": {"attrs": attrs}},
                               {"command": "edit.deselectAll", "params": {}}], source)
        if err:
            return {"error": err}
        if not (await story(core, fid, source)).get("overset"):
            return {"shrunk": k}
    words = " ".join(paras).split()
    lo, hi, best = 1, len(words) - 1, None
    while lo <= hi:
        mid = (lo + hi) // 2
        await run(core, text_fill(fid, [{"style": style, "text": " ".join(words[:mid]).rstrip(",;:") + "…"}], slot), source)
        if (await story(core, fid, source)).get("overset"):
            hi = mid - 1
        else:
            best, lo = mid, mid + 1
    if best:
        await run(core, text_fill(fid, [{"style": style, "text": " ".join(words[:best]).rstrip(",;:") + "…"}], slot), source)
        return {"trimmed": f"{best}/{len(words)} words"}
    return {"overset": True}


async def tighten(core: Core, rt: dict, style: str | None, first: int, page_count: int, added: int, source: str) -> int:
    info = await story(core, first, source)
    per_page = (info.get("length") or 0) / max(added + 1, 1)
    last_page = page_count + added - 1
    if added < 1 or per_page <= 0:
        return added
    if await run(core, [{"command": "layout.pages.delete", "params": {"pages": [last_page]}}], source):
        return added
    over = (await story(core, first, source)).get("overset") or 0
    if over == 0:
        return added - 1
    if over > 0.2 * per_page:
        return -1
    name = style if style in (rt.get("styleNames") or []) else "Body"
    size, leading = style_metrics(rt, name)
    for track, k in ((-8, 1.0), (-12, 0.985), (-15, 0.97)):
        chars = {"tracking": float(track)}
        if leading:
            chars["leading"] = {"kind": "points", "value": round(leading * k, 2)}
        if await run(core, [{"command": "style.paragraph.edit", "params": {"name": name, "chars": chars}}], source):
            break
        if not (await story(core, first, source)).get("overset"):
            return added - 1
    await run(core, [{"command": "style.paragraph.edit", "params": {"name": name, "chars": {"tracking": 0.0, **({"leading": {"kind": "points", "value": leading}} if leading else {})}}}], source)
    return -1


async def continue_flow(core: Core, rt: dict, flow: str, chain: list[int], page_count: int, source: str) -> tuple[int, bool]:
    spec_ = rt["flow"].get(flow) or {}
    limit = int(spec_.get("max", 24 if spec_.get("page") else 0))
    added = 0
    last = chain[-1]
    tightened = False
    while True:
        info = await story(core, chain[0], source)
        if not info.get("overset"):
            if added and not tightened:
                tightened = True
                left = await tighten(core, rt, (rt["flow"].get(flow) or {}).get("style"), chain[0], page_count, added, source)
                if left >= 0:
                    return left, False
                added -= 1
                last = (await story(core, chain[0], source)).get("frames", [last])[-1]
                continue
            return added, False
        if added >= limit:
            return added, True
        n = page_count + added
        spread, offx, side = C.page_slots(rt, n + 1)[n]
        b = C.continuation(rt, flow, n, spread, offx, side)
        try:
            res = await core.batch(b.cmds, source=source)
        except CoreError:
            return added, True
        results = res.get("results", [])
        ids = [results[i].get("id") for i in b.flows.get(flow, []) if i < len(results) and isinstance(results[i], dict)]
        if not ids:
            return added + 1, True
        for nxt in ids:
            await core.call("frame.thread", {"from": last, "to": nxt}, source=source)
            last = nxt
        added += 1


async def make(core: Core, pack: dict, template_id: str | None = None, *, brand: dict | None = None, fills: dict | None = None,
               options: dict | None = None, source: str = "engine") -> dict:
    t0 = time.time()
    options = options or {}
    ranked = None
    if not template_id:
        ranked = R.rank(pack, limit=3, category=options.get("category"))
        if not ranked:
            raise CoreError("no templates to choose from")
        template_id = ranked[0]["id"]
    if registry.get(template_id) is None:
        raise CoreError(f"no template {template_id}: list them with list_templates")
    brand = brand or brand_from_brief(pack)
    title = options.get("title") or pack.get("title") or None
    built = await registry.build(core, template_id, brand=brand, sample=False, title=title, source=source)
    rt, recs = built["rt"], built["slots"]
    has_flow = any(r.get("flow") for r in recs)
    content = Content(pack, has_flow=has_flow, overrides=fills)
    report: dict = {"template": template_id, "filled": [], "kept": [], "placeholders": [], "emptied": [], "fit": {}, "warnings": []}
    src = pack
    if brand and brand.get("logo") and not any(i.get("role") == "logo" for i in pack.get("images") or []):
        src = {**pack, "images": [*(pack.get("images") or []), {"id": "brand-logo", "path": brand["logo"], "role": "logo", "name": "logo"}]}
    images = assign_images(rt, [r for r in recs if r["kind"] == "image"], src, options.get("placeholders", True))
    captions: dict[str, str | None] = {}
    alts = {b["ref"]: b.get("caption") or b.get("alt") for a in pack.get("articles") or [] for b in a["blocks"] if b["type"] == "image"}
    for r in recs:
        if r["kind"] != "image":
            continue
        ch = images.get(r["id"])
        slot = r.get("slot") or ""
        if not ch:
            report["emptied"].append(slot)
            continue
        if "image" in ch:
            img = ch["image"]
            err = await run(core, image_fill(r["id"], img["path"], ch["fit"], img.get("focal")), source)
            if err:
                report["warnings"].append(f"{slot}: {err}")
                continue
            report["filled"].append(f"{slot}={img['id']}")
            base, n, _ = parse_slot(slot)
            cap = img.get("caption") or alts.get(img["id"]) or None
            if base == "hero" or n == 1:
                captions.setdefault("caption", cap)
            if n:
                captions.setdefault(f"caption{n}", cap)
        else:
            await run(core, image_fill(r["id"], ch["placeholder"], ch["fit"]), source)
            report["placeholders"].append(slot)
    done: dict[str, list[str] | None] = {}
    fitted = []
    for r in recs:
        if r["kind"] != "text" or r.get("flow"):
            continue
        slot = r.get("slot") or ""
        if slot not in done:
            if fills and slot in fills:
                v = fills[slot]
                done[slot] = v if isinstance(v, list) else [str(v)]
            elif slot.startswith("caption") and slot in captions:
                done[slot] = [captions[slot]] if captions[slot] else []
            else:
                done[slot] = content.value(slot)
        paras = done[slot]
        if paras is None:
            report["kept"].append(slot)
            continue
        if not paras:
            await run(core, text_fill(r["id"], [{"style": r.get("style"), "text": ""}], slot), source)
            report["emptied"].append(slot)
            continue
        err = await run(core, text_fill(r["id"], [{"style": r.get("style"), "text": p} for p in paras], slot), source)
        if err:
            report["warnings"].append(f"{slot}: {err}")
            continue
        report["filled"].append(slot)
        fitted.append((r, paras))
    chains: dict[str, list[dict]] = {}
    for r in recs:
        if r["kind"] == "text" and r.get("flow"):
            chains.setdefault(r["flow"], []).append(r)
    page_count = len(rt["pages"])
    for flow, chain in chains.items():
        paras = content.flow(rt, chain[0].get("style"), rt.get("medium", "print")) if flow == "body" or len(chains) == 1 else []
        if fills and flow in fills:
            v = fills[flow]
            paras = [{"style": chain[0].get("style") or "Body", "text": str(x)} for x in (v if isinstance(v, list) else str(v).split("\n\n"))]
        if not paras:
            report["kept"].append(f"flow:{flow}")
            continue
        err = await run(core, text_fill(chain[0]["id"], paras, flow), source)
        if err:
            report["warnings"].append(f"flow {flow}: {err}")
            continue
        report["filled"].append(f"flow:{flow}({len(paras)} paragraphs)")
        added, over = await continue_flow(core, rt, flow, [c["id"] for c in chain], page_count, source)
        page_count += added
        if added:
            report["addedPages"] = report.get("addedPages", 0) + added
        if over:
            report["warnings"].append(f"flow {flow} is longer than the template holds ({added} pages added): cut copy, add pages, or pick a template with continuation pages")
    for r, paras in fitted:
        res = await fit_text(core, rt, r, paras, source)
        if res:
            report["fit"][r.get("slot") or str(r["id"])] = res
    await run(core, [{"command": "edit.deselectAll", "params": {}}], source)
    try:
        pf = await core.exec("preflight.run", {}, source=source)
        issues = pf if isinstance(pf, list) else (pf or {}).get("issues", [])
        report["preflight"] = {"errors": sum(1 for i in issues if i.get("severity") == "error"),
                               "warnings": sum(1 for i in issues if i.get("severity") != "error"),
                               "top": [i.get("message", "")[:90] for i in issues[:3]]}
    except CoreError:
        pass
    used_imgs = {f.split("=", 1)[1] for f in report["filled"] if "=" in f}
    unused_imgs = [i["id"] for i in pack.get("images") or [] if i["id"] not in used_imgs and i.get("role") != "logo"]
    unused_arts = [a["id"] for a in (pack.get("articles") or [])[1:] if not any(a["title"] and a["title"] in str(v) for v in done.values())]
    report["unused"] = {k: v for k, v in (("images", unused_imgs), ("articles", unused_arts)) if v}
    report["pages"] = page_count
    report["brand"] = (brand or {}).get("name") or ("from brief colours" if brand else None)
    if ranked:
        report["alternatives"] = [{"id": x["id"], "score": x["score"]} for x in ranked[1:]]
    for k in ("filled", "kept", "placeholders", "emptied", "warnings"):
        report[k] = list(dict.fromkeys(report[k]))
    nxt = []
    if report["kept"]:
        nxt.append(f"replace sample text in: {', '.join(report['kept'][:8])}")
    if report["placeholders"]:
        nxt.append(f"place real images in: {', '.join(report['placeholders'][:6])}")
    if report["emptied"]:
        nxt.append(f"write: {', '.join(report['emptied'][:6])}")
    report["next"] = nxt
    report["seconds"] = round(time.time() - t0, 2)
    report["slots"] = {r.get("slot") or f"flow:{r.get('flow')}": r["id"] for r in recs}
    return {k: v for k, v in report.items() if v not in ([], {}, None)}
