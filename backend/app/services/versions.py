"""Local version history: a copy of the document is kept under the data folder on every save, and on demand with a
comment (File > Save a Version). Each copy is a whole .fdes (it carries its own page 1 thumbnail), so restoring or opening
a copy needs nothing else."""

import hashlib
import io
import json
import threading
import time
import zipfile
from pathlib import Path

from app.core import config

ROOT = config.DATA_DIR / "Versions"
INNER = "document.fdoc"
KEEP_AUTO = 40
KEEP_ALL = 200
_lock = threading.Lock()


def folder(doc_path: str) -> Path:
    return ROOT / hashlib.sha1(str(Path(doc_path).expanduser().resolve()).encode()).hexdigest()[:16]


def _index(d: Path) -> list[dict]:
    try:
        return [v for v in json.loads((d / "index.json").read_text()) if isinstance(v, dict)]
    except (OSError, ValueError):
        return []


def _write_index(d: Path, items: list[dict]) -> None:
    (d / "index.json").write_text(json.dumps(items, indent=1))


def _digest(data: bytes) -> str:
    try:
        with zipfile.ZipFile(io.BytesIO(data)) as z:
            return hashlib.sha1(z.read(INNER)).hexdigest()
    except (zipfile.BadZipFile, KeyError):
        return hashlib.sha1(data).hexdigest()


def keep(doc_path: str, data: bytes, comment: str = "", kind: str = "save") -> dict | None:
    """Keep `data` as a version of the document at `doc_path`. An unchanged document is not kept twice; a comment on an
    unchanged document names the latest version instead."""
    d = folder(doc_path)
    with _lock:
        d.mkdir(parents=True, exist_ok=True)
        items = _index(d)
        digest = _digest(data)
        if items and items[0].get("digest") == digest:
            if comment:
                items[0].update({"comment": comment, "kind": "named"})
                _write_index(d, items)
                return items[0]
            return None
        now = time.time()
        vid = time.strftime("%Y%m%d-%H%M%S", time.localtime(now)) + f"-{int(now * 1000) % 1000:03d}"
        (d / f"{vid}.fdes").write_bytes(data)
        entry = {"id": vid, "time": int(now * 1000), "kind": "named" if comment else kind, "comment": comment, "bytes": len(data), "digest": digest}
        items.insert(0, entry)
        autos = [v for v in items if v.get("kind") != "named"]
        drop = autos[KEEP_AUTO:] + items[KEEP_ALL:]
        for v in drop:
            if v in items:
                items.remove(v)
            (d / f"{v['id']}.fdes").unlink(missing_ok=True)
        _write_index(d, items)
        return entry


def listing(doc_path: str) -> list[dict]:
    d = folder(doc_path)
    return [{k: v for k, v in e.items() if k != "digest"} for e in _index(d) if (d / f"{e['id']}.fdes").is_file()]


def _path(doc_path: str, vid: str) -> Path:
    if not vid.replace("-", "").isalnum():
        raise ValueError("no such version")
    p = folder(doc_path) / f"{vid}.fdes"
    if not p.is_file():
        raise ValueError("no such version")
    return p


def read(doc_path: str, vid: str) -> bytes:
    return _path(doc_path, vid).read_bytes()


def thumb(doc_path: str, vid: str) -> bytes | None:
    try:
        with zipfile.ZipFile(_path(doc_path, vid)) as z:
            return z.read("thumbs/page-1.png")
    except (KeyError, zipfile.BadZipFile, OSError):
        return None


def entry(doc_path: str, vid: str) -> dict | None:
    return next((e for e in listing(doc_path) if e["id"] == vid), None)
