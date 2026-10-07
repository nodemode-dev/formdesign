import json
import re
import time

from app.core import config

ROLES = ("ink", "paper", "accent", "accent2", "muted", "tint", "dark", "light")
FONT_ROLES = ("display", "text", "accent", "mono")
HEX = re.compile(r"^#[0-9a-fA-F]{6}$")


def slug(name: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", (name or "").lower()).strip("-") or "brand"


def clean(b: dict) -> dict:
    pal = {k: v.lower() for k, v in (b.get("palette") or {}).items() if k in ROLES and isinstance(v, str) and HEX.match(v)}
    fonts = {}
    for k, v in (b.get("fonts") or {}).items():
        if k in FONT_ROLES and isinstance(v, (list, tuple)) and v:
            fonts[k] = [str(v[0]), str(v[1]) if len(v) > 1 else "Regular"]
        elif k in FONT_ROLES and isinstance(v, str) and v:
            fonts[k] = [v, "Regular"]
    out = {"name": str(b.get("name") or "Brand").strip()[:60], "palette": pal, "fonts": fonts}
    if b.get("logo"):
        out["logo"] = str(b["logo"])
    if b.get("voice"):
        out["voice"] = str(b["voice"])[:400]
    return out


def listing() -> list[dict]:
    out = []
    if config.BRANDS_DIR.is_dir():
        for f in sorted(config.BRANDS_DIR.glob("*.json")):
            try:
                out.append(json.loads(f.read_text()))
            except ValueError:
                continue
    return out


def get(name: str | None) -> dict | None:
    if not name:
        return None
    f = config.BRANDS_DIR / f"{slug(name)}.json"
    if f.is_file():
        try:
            return json.loads(f.read_text())
        except ValueError:
            return None
    return None


def save(b: dict) -> dict:
    data = clean(b)
    data["modified"] = time.time()
    config.BRANDS_DIR.mkdir(parents=True, exist_ok=True)
    (config.BRANDS_DIR / f"{slug(data['name'])}.json").write_text(json.dumps(data, indent=1))
    return data


def delete(name: str) -> bool:
    f = config.BRANDS_DIR / f"{slug(name)}.json"
    if f.is_file():
        f.unlink()
        return True
    return False


def resolve(brand) -> dict | None:
    if brand is None or brand == "":
        return None
    if isinstance(brand, str):
        return get(brand)
    if isinstance(brand, dict):
        return clean(brand) if brand.get("palette") or brand.get("fonts") else (get(brand.get("name")) or clean(brand))
    return None
