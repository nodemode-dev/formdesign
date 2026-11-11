import hashlib
import json
import re
from pathlib import Path

from app.core import config

BOOLS = ("overset", "fonts", "links", "emptyFrames", "rgb", "transparency")
NUMBERS = ("minPpi", "maxPpi", "bleed", "safety", "minType", "spotMax", "pageMultiple")

BUILTIN = [
    {"name": "[Basic]", "note": "Overset text, missing fonts and links, empty frames; low resolution and RGB in print documents.", "checks": {}},
    {"name": "Commercial print", "note": "Offset or digital press: 300 ppi images, 9 pt of bleed, text 9 pt inside the trim, type of 6 pt and up, CMYK only, two spot inks at most.",
     "checks": {"rgb": True, "minPpi": 300, "maxPpi": 1200, "bleed": 9, "safety": 9, "minType": 6, "spotMax": 2}},
    {"name": "PDF/X-1a, no transparency", "note": "Commercial print checks plus any transparency, which PDF/X-1a flattens.",
     "checks": {"rgb": True, "minPpi": 300, "maxPpi": 1200, "bleed": 9, "safety": 9, "minType": 6, "spotMax": 2, "transparency": True}},
    {"name": "Saddle-stitched booklet", "note": "Commercial print checks plus a page count in multiples of four.",
     "checks": {"rgb": True, "minPpi": 300, "bleed": 9, "safety": 12, "minType": 6, "pageMultiple": 4}},
    {"name": "Digital and screen", "note": "PDFs, EPUB and the web: RGB is fine, 72 ppi is enough, very large images are flagged.",
     "checks": {"rgb": False, "minPpi": 72, "maxPpi": 600}},
]


class ProfileError(ValueError):
    pass


def slug(name: str) -> str:
    base = re.sub(r"[^a-z0-9]+", "-", (name or "").lower()).strip("-")
    key = (name or "").strip().lower()
    return base or (f"profile-{hashlib.sha1(key.encode()).hexdigest()[:8]}" if key else "profile")


def _file(name: str) -> Path:
    d = config.DATA_DIR / "Preflight"
    path, old = d / f"{slug(name)}.json", d / "profile.json"
    try:
        if not path.exists() and str(json.loads(old.read_text()).get("name") or "").strip().lower() == (name or "").strip().lower():
            return old
    except (OSError, ValueError, AttributeError):
        pass
    return path


def clean(p: dict) -> dict:
    name = str(p.get("name") or "").strip()[:80]
    if not name:
        raise ProfileError("A preflight profile needs a name.")
    checks = {}
    for k, v in (p.get("checks") or {}).items():
        if k in BOOLS and isinstance(v, bool):
            checks[k] = v
        elif k in NUMBERS and (v is None or (isinstance(v, (int, float)) and not isinstance(v, bool) and v >= 0)):
            checks[k] = v
    out = {"name": name, "checks": checks}
    if p.get("note"):
        out["note"] = str(p["note"])[:300]
    return out


def listing() -> list[dict]:
    out = [{**p, "builtin": True} for p in BUILTIN]
    d = config.DATA_DIR / "Preflight"
    if d.is_dir():
        for f in sorted(d.glob("*.json")):
            try:
                out.append(json.loads(f.read_text()))
            except ValueError:
                continue
    return out


def get(name: str) -> dict | None:
    key = (name or "").strip().lower()
    return next((p for p in listing() if p["name"].lower() == key), None)


def save(p: dict) -> dict:
    out = clean(p)
    if any(b["name"].lower() == out["name"].lower() for b in BUILTIN):
        raise ProfileError(f"\"{out['name']}\" is a built-in profile; save under another name.")
    d = config.DATA_DIR / "Preflight"
    d.mkdir(parents=True, exist_ok=True)
    _file(out["name"]).write_text(json.dumps(out, indent=2), encoding="utf-8")
    return out


def delete(name: str) -> None:
    if any(b["name"].lower() == (name or "").lower() for b in BUILTIN):
        raise ProfileError("Built-in profiles cannot be deleted.")
    path = _file(name)
    if not path.is_file():
        raise ProfileError(f"No preflight profile named \"{name}\".")
    path.unlink()
