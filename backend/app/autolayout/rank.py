import math

from app.templates import registry, spec

LONGFORM = {"magazine", "report", "book", "newsletter", "web-article", "web-blog", "web-press", "web-docs"}
SHORTFORM = {"ad-social", "ad-story", "ad-banner", "ad-print", "poster", "flyer", "postcard", "card", "invitation"}
UNIT_PT = {"pt": 1.0, "px": 1.0, "in": 72.0, "mm": 72 / 25.4, "cm": 72 / 2.54, "pc": 12.0}


def image_slots(t: dict) -> int:
    return sum(1 for p in t.get("pages") or [] for it in p.get("items", []) if it.get("kind") == "image" and "logo" not in (it.get("slot") or ""))


def rank(pack: dict, limit: int = 5, category: str | None = None, medium: str | None = None) -> list[dict]:
    brief = pack.get("brief") or {}
    kinds = brief.get("kinds") or []
    tones = set(brief.get("tone") or [])
    facts = pack.get("facts") or {}
    words = facts.get("words", 0)
    nimg = sum(1 for i in pack.get("images") or [] if i.get("role") != "logo")
    size = brief.get("size")
    out = []
    for t in registry.templates().values():
        c = spec.card(t)
        if category and c["category"] != category:
            continue
        if medium and c["medium"] != medium:
            continue
        score, why = 0.0, []
        hit = next((i for i, k in enumerate(kinds[:4]) if k in c["kinds"]), None)
        if hit is not None:
            score += 10 - 2.5 * hit
            why.append(kinds[hit])
        elif kinds:
            score -= 4
        else:
            if words > 900 and set(c["kinds"]) & LONGFORM:
                score += 4
                why.append("long text")
            elif words < 150 and set(c["kinds"]) & SHORTFORM:
                score += 3
                why.append("short copy")
        if brief.get("medium") and brief["medium"] == c["medium"] and kinds:
            score += 1.5
        common = tones & set(c["tone"])
        if common:
            score += 1.5 * len(common)
            why += sorted(common)
        if size:
            u = UNIT_PT.get(size.get("unit", "pt"), 1.0)
            cu = UNIT_PT.get(c["unit"], 1.0)
            bw, bh = size["w"] * u, size["h"] * u
            tw, th = c["size"][0] * cu, c["size"][1] * cu
            if bw > 0 and bh > 0 and tw > 0 and th > 0:
                d = abs(math.log((bw / bh) / (tw / th)))
                if d < 0.03:
                    score += 6
                    why.append("size")
                    if abs(bw - tw) < 2 and abs(bh - th) < 2:
                        score += 2
                elif d > 0.25:
                    score -= 3
        if brief.get("pages") and not c["flows"]:
            score -= 0.4 * abs(c["pages"] - int(brief["pages"]))
        islots = image_slots(t)
        missing = max(0, islots - nimg)
        score -= 0.35 * missing
        score += 0.6 * min(nimg, islots, 3)
        text = (brief.get("text") or "").lower()
        words_in_name = [w for w in c["name"].lower().replace(",", " ").split() if len(w) > 3]
        tagged = {g.lower() for g in c["tags"] + words_in_name if len(g) > 3 and g.lower() in text}
        score += min(len(tagged), 3)
        if nimg >= 2 and "no image" in c["tags"]:
            score -= 2
        if c["flows"] and words > 300:
            score += 1.5
        cont = any((f or {}).get("page") and int((f or {}).get("max", 24)) > 0 for f in (t.get("flow") or {}).values())
        if brief.get("pages") and int(brief["pages"]) > c["pages"] and not cont:
            score -= min(3.0, 0.5 * (int(brief["pages"]) - c["pages"]))
        if cont and words > 500:
            score += 0.5
        if any(g in ("sale", "promo", "discount", "offer") for g in c["tags"] + c["kinds"]) and not any(w in text for w in ("sale", "promo", "discount", "offer", "% off", "percent off")):
            score -= 2.5
        if c["flows"] and words < 150:
            score -= 2
        if not c["flows"] and words > 900 and set(c["kinds"]) & LONGFORM:
            score -= 2
        if c["user"]:
            score += 1
        out.append({"id": c["id"], "name": c["name"], "category": c["category"], "score": round(score, 2), "why": why[:4]})
    out.sort(key=lambda x: (-x["score"], x["id"]))
    return out[:limit]
