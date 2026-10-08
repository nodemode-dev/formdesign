import base64
import json
import os
import re
from pathlib import Path

from app.core import config
from app.engine.core import core

SUFFIX = ".fdlib"


class LibraryError(ValueError):
    pass


def folder() -> Path:
    raw = os.environ.get("FORMDES_LIBRARIES_DIR", "").strip()
    return Path(raw).expanduser() if raw else config.DATA_DIR / "Libraries"


def slug(name: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", (name or "").lower()).strip("-") or "library"


def _path(name: str) -> Path:
    p = folder() / f"{slug(name)}{SUFFIX}"
    if p.parent.resolve() != folder().resolve():
        raise LibraryError("bad library name")
    return p


def _load(name: str) -> dict:
    p = _path(name)
    if not p.is_file():
        raise LibraryError(f"No library named \"{name}\".")
    try:
        data = json.loads(p.read_text())
    except ValueError as exc:
        raise LibraryError(f"{p.name} is not a library: {exc}") from exc
    data.setdefault("items", [])
    data.setdefault("name", name)
    return data


def _save(name: str, data: dict) -> None:
    folder().mkdir(parents=True, exist_ok=True)
    _path(name).write_text(json.dumps(data, ensure_ascii=False))


def listing() -> dict:
    out = []
    d = folder()
    if d.is_dir():
        for f in sorted(d.glob(f"*{SUFFIX}")):
            try:
                data = json.loads(f.read_text())
            except ValueError:
                continue
            out.append({"name": data.get("name") or f.stem, "file": f.name, "items": len(data.get("items", []))})
    return {"folder": str(d), "libraries": out}


def create(name: str) -> dict:
    name = (name or "").strip()[:80]
    if not name:
        raise LibraryError("A library needs a name.")
    if _path(name).exists():
        raise LibraryError(f"A library named \"{name}\" already exists.")
    _save(name, {"name": name, "items": []})
    return {"name": name, "items": 0}


def delete(name: str) -> None:
    p = _path(name)
    if not p.is_file():
        raise LibraryError(f"No library named \"{name}\".")
    p.unlink()


def items(name: str) -> list[dict]:
    data = _load(name)
    return [{"index": i, "name": it.get("name", ""), "description": it.get("description", ""), "thumb": it.get("thumb", "")} for i, it in enumerate(data["items"])]


async def thumb(snippet: str) -> str:
    try:
        r = await core.call("snippetThumb", {"base64": snippet, "size": 192})
    except Exception:
        return ""
    return "data:image/png;base64," + base64.b64encode(r.bin or b"").decode() if r.bin else ""


async def add(name: str, title: str | None = None, description: str = "", ids: list[int] | None = None) -> dict:
    data = _load(name)
    if ids:
        await core.exec("selection.set", {"ids": ids}, source="engine")
    snip = await core.exec("snippet.export", {}, source="engine")
    b64 = (snip or {}).get("base64") if isinstance(snip, dict) else None
    if not b64:
        raise LibraryError("Select one or more objects to add them to the library.")
    item = {"name": (title or "").strip()[:120] or f"Item {len(data['items']) + 1}", "description": (description or "")[:400], "snippet": b64, "thumb": await thumb(b64)}
    data["items"].append(item)
    _save(name, data)
    return {"index": len(data["items"]) - 1, "name": item["name"]}


def update(name: str, index: int, title: str | None = None, description: str | None = None) -> dict:
    data = _load(name)
    if not 0 <= index < len(data["items"]):
        raise LibraryError(f"No item {index} in \"{name}\".")
    it = data["items"][index]
    if title is not None and title.strip():
        it["name"] = title.strip()[:120]
    if description is not None:
        it["description"] = description[:400]
    _save(name, data)
    return {"index": index, "name": it["name"], "description": it.get("description", "")}


def remove(name: str, index: int) -> None:
    data = _load(name)
    if not 0 <= index < len(data["items"]):
        raise LibraryError(f"No item {index} in \"{name}\".")
    data["items"].pop(index)
    _save(name, data)


async def place(name: str, index: int, x: float | None = None, y: float | None = None, spread: int | None = None) -> dict:
    data = _load(name)
    if not 0 <= index < len(data["items"]):
        raise LibraryError(f"No item {index} in \"{name}\".")
    params: dict = {"base64": data["items"][index]["snippet"]}
    if x is not None and y is not None:
        params.update({"x": x, "y": y})
    if spread is not None:
        params["spread"] = spread
    return await core.exec("snippet.place", params, source="engine")
