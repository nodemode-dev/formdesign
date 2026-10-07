import copy
import json
from functools import lru_cache
from pathlib import Path

from app.core import config

BUNDLED_FONTS = {
    "Inter": ["Regular", "Medium", "SemiBold"],
    "Source Sans 3": ["Regular", "Italic", "Bold", "Semibold"],
    "Source Serif 4": ["Regular", "Italic", "Bold", "Bold Italic", "Semibold"],
    "JetBrains Mono": ["Regular"],
}
ROLE_FALLBACK = {"display": ("Source Serif 4", "Bold"), "text": ("Source Serif 4", "Regular"), "accent": ("Inter", "SemiBold"), "mono": ("JetBrains Mono", "Regular")}
PAGE_PRESETS = {"letter": (612, 792), "legal": (612, 1008), "tabloid": (792, 1224), "a3": (841.89, 1190.55), "a4": (595.28, 841.89),
                "a5": (419.53, 595.28), "a6": (297.64, 419.53), "b5": (498.9, 708.66), "digest": (396, 612), "trade": (432, 648)}
UNIT = {"pt": 1.0, "px": 1.0, "mm": 72 / 25.4, "in": 72.0, "cm": 72 / 2.54, "pc": 12.0}
SWATCH_NAMES = {"ink": "Ink", "paper": "Paper Color", "accent": "Accent", "accent2": "Accent 2", "muted": "Muted", "tint": "Tint", "dark": "Dark", "light": "Light"}
ALIGN = {"left": "left", "center": "center", "right": "right", "justify": "leftJustified", "fully": "fullyJustified",
         "centerJustify": "centerJustified", "towardsSpine": "towardsSpine", "awayFromSpine": "awayFromSpine"}


@lru_cache(maxsize=1)
def available_fonts() -> dict[str, list[str]]:
    fonts = {k: list(v) for k, v in BUNDLED_FONTS.items()}
    manifest = config.FONTS_DIR / "fonts.json"
    if manifest.is_file():
        try:
            for fam, styles in json.loads(manifest.read_text()).items():
                fonts.setdefault(fam, [])
                for s in styles:
                    if s not in fonts[fam]:
                        fonts[fam].append(s)
        except ValueError:
            pass
    return fonts


def library_dirs() -> list[Path]:
    return [d for d in (config.TEMPLATES_DIR, config.TEMPLATES_USER_DIR) if d.is_dir()]


@lru_cache(maxsize=1)
def _base() -> tuple[dict, dict]:
    themes = json.loads((config.TEMPLATES_DIR / "_themes.json").read_text())
    kits = json.loads((config.TEMPLATES_DIR / "_kits.json").read_text())
    return themes, kits


def themes() -> dict:
    return _base()[0]


def kits() -> dict:
    return _base()[1]


def load_all() -> dict[str, dict]:
    out: dict[str, dict] = {}
    for d in library_dirs():
        for f in sorted(d.rglob("*.json")):
            if f.name.startswith("_"):
                continue
            try:
                t = json.loads(f.read_text())
            except ValueError:
                continue
            if not isinstance(t, dict) or "id" not in t:
                continue
            t["_file"] = str(f)
            t["_user"] = d == config.TEMPLATES_USER_DIR
            out[t["id"]] = t
    return out


def page_size(page: dict) -> tuple[float, float]:
    unit = UNIT.get(page.get("unit", "pt"), 1.0)
    size = page.get("size", "Letter")
    if isinstance(size, str):
        w, h = PAGE_PRESETS.get(size.lower(), PAGE_PRESETS["letter"])
        if page.get("orientation") == "landscape":
            w, h = h, w
        return w, h
    return float(size[0]) * unit, float(size[1]) * unit


def margins(page: dict) -> dict:
    unit = UNIT.get(page.get("unit", "pt"), 1.0)
    m = page.get("margins", 36)
    if isinstance(m, (int, float)):
        v = float(m) * unit
        return {"top": v, "bottom": v, "inside": v, "outside": v}
    if isinstance(m, dict):
        return {k: float(m.get(k, 36)) * unit for k in ("top", "bottom", "inside", "outside")}
    t, b, i, o = (list(m) + [36, 36, 36, 36])[:4]
    return {"top": t * unit, "bottom": b * unit, "inside": i * unit, "outside": o * unit}


def resolve_font(role_font: list | tuple, font_style: str | None, explicit: str | None = None) -> tuple[str, str]:
    fonts = available_fonts()
    fam, base = role_font[0], role_font[1] if len(role_font) > 1 else "Regular"
    if fam not in fonts:
        return fam, explicit or base
    styles = fonts[fam]
    if explicit:
        return fam, explicit if explicit in styles else base
    if not font_style:
        return fam, base if base in styles else (styles[0] if styles else "Regular")
    fs = font_style.lower()
    if fs == "regular":
        return fam, "Regular" if "Regular" in styles else base
    if fs == "bold":
        if base in ("Bold", "Black", "SemiBold", "Semibold"):
            return fam, base
        return fam, "Bold" if "Bold" in styles else base
    if fs == "italic":
        for cand in (f"{base} Italic" if base != "Regular" else "Italic", "Italic", "It"):
            if cand in styles:
                return fam, cand
        return fam, base
    if fs in ("bolditalic", "bold italic"):
        for cand in ("Bold Italic", "Black Italic", "Italic"):
            if cand in styles:
                return fam, cand
        return fam, base
    return fam, base


def merged_fonts(theme_fonts: dict, override: dict | None) -> dict:
    fonts = available_fonts()
    out = {}
    for role in ("display", "text", "accent", "mono"):
        cand = (override or {}).get(role) or theme_fonts.get(role) or ROLE_FALLBACK[role]
        cand = list(cand) if isinstance(cand, (list, tuple)) else [cand, "Regular"]
        if cand[0] not in fonts:
            cand = list(ROLE_FALLBACK[role])
        if len(cand) < 2:
            cand.append("Regular")
        if cand[1] not in fonts.get(cand[0], []):
            styles = fonts.get(cand[0], [])
            cand[1] = "Regular" if "Regular" in styles else (styles[0] if styles else "Regular")
        out[role] = cand
    return out


def _style_params(name: str, s: dict, base: float, fonts: dict, palette_names: dict, baseline: float | None, eff: dict | None = None) -> dict:
    para: dict = {}
    chars: dict = {}
    eff = eff or s
    size = None
    if "size" in s:
        size = round(float(s["size"]) * base, 2)
        chars["size"] = size
    if "font" in s or "fontStyle" in s or "style" in s:
        role = s.get("font", eff.get("font", "text"))
        fam, sty = resolve_font(fonts.get(role, ROLE_FALLBACK.get(role, ROLE_FALLBACK["text"])), s.get("fontStyle", eff.get("fontStyle")), s.get("style"))
        chars["fontFamily"] = fam
        chars["fontStyle"] = sty
    if "leading" in s:
        ref = size or float(eff.get("size", 1)) * base
        chars["leading"] = {"kind": "points", "value": round(float(s["leading"]) * ref, 2)}
    elif size and "leading" in eff:
        chars["leading"] = {"kind": "points", "value": round(float(eff["leading"]) * size, 2)}
    if "tracking" in s:
        chars["tracking"] = float(s["tracking"])
    if "color" in s:
        chars["fill"] = palette_names.get(s["color"], s["color"])
    if "caps" in s:
        chars["capitalization"] = "allCaps" if s["caps"] else "normal"
    if s.get("smallCaps"):
        chars["capitalization"] = "allCaps"
        chars.setdefault("tracking", 40.0)
    if "underline" in s:
        chars["underline"] = bool(s["underline"])
    if "align" in s:
        para["align"] = ALIGN.get(s["align"], s["align"])
    ref = float(eff.get("size", 1)) * base
    for k in ("firstLineIndent", "leftIndent", "rightIndent"):
        if k in s:
            para[k] = round(float(s[k]) * ref, 2)
    if "tabs" in s:
        para["tabs"] = [{"position": round(float(t) * ref, 2)} if not isinstance(t, dict) else
                        {"position": round(float(t.get("position", 0)) * ref, 2), "align": t.get("align", "left"), **({"leader": t["leader"]} if t.get("leader") else {})}
                        for t in s["tabs"]]
    elif eff.get("list") and float(eff.get("leftIndent", 0)) > 0 and ("list" in s or "leftIndent" in s):
        para["tabs"] = [{"position": round(float(eff["leftIndent"]) * ref, 2)}]
    if "hyphenate" not in eff and not s.get("basedOn") and (eff.get("font") == "display" or float(eff.get("size", 1)) >= 1.5):
        para["hyphenate"] = False
    for k in ("spaceBefore", "spaceAfter"):
        if k in s:
            para[k] = round(float(s[k]) * base, 2)
    if "dropCap" in s:
        para["dropCapLines"] = int(s["dropCap"])
        para["dropCapChars"] = int(s.get("dropCapChars", 1))
    if "keepWithNext" in s:
        para["keepWithNext"] = int(s["keepWithNext"])
    if "hyphenate" in s:
        para["hyphenate"] = bool(s["hyphenate"])
    if s.get("list") == "bullets":
        para["listType"] = "bullets"
    elif s.get("list") == "numbers":
        para["listType"] = "numbers"
    if s.get("balance"):
        para["balanceRagged"] = True
    if s.get("grid") and baseline:
        para["gridAlign"] = "allLines"
    for which in ("ruleAbove", "ruleBelow"):
        if which in s:
            r = s[which]
            para[which] = {"on": True, "weight": float(r.get("weight", 1)), "color": palette_names.get(r.get("color", "ink"), r.get("color", "Ink")),
                           "offset": float(r.get("offset", 0)), "columnWidth": bool(r.get("columnWidth", True))}
    if "shading" in s:
        sh = s["shading"]
        para["shadingOn"] = True
        para["shadingColor"] = palette_names.get(sh.get("color", "tint"), sh.get("color", "Tint"))
        para["shadingTint"] = float(sh.get("tint", 1.0))
        o = float(sh.get("offset", 4))
        para["shadingOffsets"] = [o, o, o, o]
    out = {"name": name, "para": para, "chars": chars}
    if s.get("basedOn"):
        out["basedOn"] = s["basedOn"]
    if s.get("next"):
        out["nextStyle"] = s["next"]
    return out


def resolve(t: dict, brand: dict | None = None) -> dict:
    themes_all, kits_all = _base()
    theme = copy.deepcopy(themes_all.get(t.get("theme", "editorial"), themes_all["editorial"]))
    kit = copy.deepcopy(kits_all.get(t.get("kit", "longform"), kits_all["longform"]))
    palette = {**theme["palette"], **(t.get("palette") or {})}
    font_over = dict(t.get("fonts") or {})
    if brand:
        palette.update({k: v for k, v in (brand.get("palette") or {}).items() if v})
        font_over.update({k: v for k, v in (brand.get("fonts") or {}).items() if v})
    fonts = merged_fonts(theme["fonts"], font_over)
    page = dict(t.get("page") or {})
    unit = UNIT.get(page.get("unit", "pt"), 1.0)
    w, h = page_size(page)
    base = float(kit.get("base", 10)) * float(t.get("scale", 1.0))
    baseline = float(page["baseline"]) * unit if page.get("baseline") else None
    pal_names = {k: SWATCH_NAMES.get(k, k.title()) for k in palette}
    styles_over = (t.get("styles") or {})
    para_specs = {**kit["paragraph"]}
    for name, over in (styles_over.get("paragraph") or {}).items():
        para_specs[name] = {**para_specs.get(name, {}), **over}
    char_specs = {**kit.get("character", {})}
    for name, over in (styles_over.get("character") or {}).items():
        char_specs[name] = {**char_specs.get(name, {}), **over}
    ordered = []
    done: set[str] = set()

    def visit(n: str, depth: int = 0) -> None:
        if n in done or n not in para_specs or depth > 12:
            return
        parent = para_specs[n].get("basedOn")
        if parent:
            visit(parent, depth + 1)
        done.add(n)
        ordered.append(n)

    for n in para_specs:
        visit(n)
    def effective(n: str, depth: int = 0) -> dict:
        sp = para_specs.get(n) or {}
        parent = sp.get("basedOn")
        return {**(effective(parent, depth + 1) if parent and depth < 12 else {}), **sp}

    para_styles = [_style_params(n, para_specs[n], base, fonts, pal_names, baseline, effective(n)) for n in ordered]
    char_styles = []
    for n, s in char_specs.items():
        p = _style_params(n, s, base, fonts, pal_names, None)
        char_styles.append({"name": n, "chars": p["chars"], **({"basedOn": s["basedOn"]} if s.get("basedOn") else {})})
    return {
        "id": t["id"], "name": t.get("name", t["id"]), "category": t.get("category", "other"), "medium": t.get("medium", "print"),
        "width": w, "height": h, "unit": unit, "facing": bool(page.get("facing", False)), "startLeft": bool(page.get("startLeft", False)), "margins": margins(page),
        "columns": int(page.get("columns", 1)), "gutter": float(page.get("gutter", 12)) * unit, "bleed": float(page.get("bleed", 0)) * unit,
        "baseline": baseline, "palette": palette, "swatches": pal_names, "fonts": fonts, "base": base,
        "paraStyles": para_styles, "charStyles": char_styles, "styleNames": [p["name"] for p in para_styles],
        "parents": t.get("parents") or {}, "pages": t.get("pages") or [{"parent": "A", "items": []}],
        "flow": t.get("flow") or {}, "slots": t.get("slots") or {}, "web": t.get("web") or {}, "theme": t.get("theme"), "kit": t.get("kit"),
        "brand": (brand or {}).get("name"),
    }


def card(t: dict) -> dict:
    page = t.get("page") or {}
    w, h = page_size(page)
    slots = sorted({it["slot"] for p in (t.get("pages") or []) for it in p.get("items", []) if it.get("slot")})
    return {"id": t["id"], "name": t.get("name", t["id"]), "category": t.get("category", "other"), "kinds": t.get("kinds", []),
            "tags": t.get("tags", []), "tone": t.get("tone", []), "medium": t.get("medium", "print"), "description": t.get("description", ""),
            "size": [round(w, 2), round(h, 2)], "unit": page.get("unit", "pt"), "pages": len(t.get("pages") or []),
            "facing": bool(page.get("facing")), "theme": t.get("theme"), "slots": slots, "user": bool(t.get("_user")),
            "flows": sorted((t.get("flow") or {}).keys())}
