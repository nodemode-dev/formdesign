import json
import re
from pathlib import Path

from app.core import config
from app.engine.core import CoreError, core

ID = re.compile(r"^[A-Za-z0-9_.-]{1,80}$")
COMBO = re.compile(r"^[^\s]{1,40}$")
KINDS = {"workspaces": "Workspaces", "shortcuts": "Shortcuts"}


class PrefsError(ValueError):
    pass


def folder(kind: str) -> Path:
    return config.DATA_DIR / KINDS[kind]


def slug(name: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", (name or "").lower()).strip("-") or "untitled"


def _ids(v, limit: int = 64) -> list[str]:
    return [x for x in (v if isinstance(v, list) else []) if isinstance(x, str) and ID.match(x)][:limit]


def _layout(raw: dict) -> dict:
    raw = raw if isinstance(raw, dict) else {}
    d = raw.get("dock") if isinstance(raw.get("dock"), dict) else {}
    heights = {k: float(v) for k, v in (d.get("heights") or {}).items() if ID.match(str(k)) and isinstance(v, (int, float)) and 0 < v < 5000} if isinstance(d.get("heights"), dict) else {}
    custom = [{"id": c["id"], "label": str(c.get("label") or "")[:80]} for c in (d.get("custom") or []) if isinstance(c, dict) and re.match(r"^g[0-9a-z]{1,16}$", str(c.get("id", "")))][:16]
    width = d.get("width") if isinstance(d.get("width"), (int, float)) and 120 <= d.get("width") <= 1200 else None
    groups = {}
    for k, v in (raw.get("groups") or {}).items() if isinstance(raw.get("groups"), dict) else []:
        if ID.match(str(k)) and isinstance(v, dict):
            g = {}
            if isinstance(v.get("tab"), str) and ID.match(v["tab"]):
                g["tab"] = v["tab"]
            if isinstance(v.get("min"), bool):
                g["min"] = v["min"]
            groups[k] = g
    homes = {k: v for k, v in (raw.get("homes") or {}).items() if ID.match(str(k)) and isinstance(v, str) and ID.match(v)} if isinstance(raw.get("homes"), dict) else {}
    return {"dock": {"side": _ids(d.get("side")), "dock": _ids(d.get("dock")), "hidden": _ids(d.get("hidden")), "heights": heights, "custom": custom, "width": width}, "groups": groups, "homes": homes}


def _keys(raw: dict) -> dict:
    out = {}
    for k, v in (raw if isinstance(raw, dict) else {}).items():
        if ID.match(str(k)) and isinstance(v, list):
            out[k] = [c for c in v if isinstance(c, str) and COMBO.match(c)][:4]
    return dict(list(out.items())[:2000])


def clean(kind: str, body: dict) -> dict:
    name = str((body or {}).get("name") or "").strip()[:80]
    if not name:
        raise PrefsError("A name is needed.")
    if kind == "workspaces":
        return {"name": name, "layout": _layout(body.get("layout"))}
    return {"name": name, "keys": _keys(body.get("keys"))}


def listing(kind: str) -> list[dict]:
    d = folder(kind)
    out = []
    if d.is_dir():
        for f in sorted(d.glob("*.json")):
            try:
                out.append(clean(kind, json.loads(f.read_text())))
            except (ValueError, PrefsError):
                continue
    return out


def save(kind: str, body: dict, reserved: tuple[str, ...] = ()) -> dict:
    out = clean(kind, body)
    if out["name"].lower() in {r.lower() for r in reserved}:
        raise PrefsError(f"\"{out['name']}\" is built in; save under another name.")
    d = folder(kind)
    d.mkdir(parents=True, exist_ok=True)
    (d / f"{slug(out['name'])}.json").write_text(json.dumps(out, indent=1, ensure_ascii=False))
    return out


def profiles_dir() -> Path:
    return config.DATA_DIR / "Profiles"


def profile_files() -> list[Path]:
    d = profiles_dir()
    return sorted(f for f in d.iterdir() if f.suffix.lower() in (".icc", ".icm")) if d.is_dir() else []


async def add_profile(src: str) -> dict:
    p = Path(src or "")
    if not p.is_file() or p.suffix.lower() not in (".icc", ".icm"):
        raise PrefsError("Choose an ICC profile (.icc or .icm).")
    try:
        info = (await core.exec("color.loadProfile", {"path": str(p)}, source="engine")) or {}
    except CoreError as exc:
        raise PrefsError(str(exc).split(": ", 1)[-1]) from exc
    d = profiles_dir()
    d.mkdir(parents=True, exist_ok=True)
    dest = d / f"{slug(info.get('name') or p.stem)}{p.suffix.lower()}"
    if p.resolve() != dest.resolve():
        dest.write_bytes(p.read_bytes())
    return info


async def load_profiles() -> int:
    n = 0
    for f in profile_files():
        try:
            await core.exec("color.loadProfile", {"path": str(f)}, source="engine")
            n += 1
        except CoreError:
            continue
    return n


def delete(kind: str, name: str) -> None:
    path = folder(kind) / f"{slug(name)}.json"
    if not path.is_file():
        raise PrefsError(f"Nothing saved as \"{name}\".")
    path.unlink()
