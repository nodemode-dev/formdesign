import io
import json
import re
import tempfile
import zipfile
from pathlib import Path

from app.core import config
from app.engine.core import CoreError, core
from app.engine.documents import INNER, documents

BOOK_EXT = ".fdbook"
BOOKS_DIR = config.PROJECTS_DIR / "Books"


class BookError(Exception):
    pass


def _safe_name(name: str) -> str:
    n = re.sub(r"[^\w .()-]+", "", name).strip()
    return n[:80] or "Book"


def all_books() -> list[dict]:
    out = []
    for p in sorted(config.PROJECTS_DIR.rglob(f"*{BOOK_EXT}")):
        try:
            b = load(str(p))
            out.append({"path": str(p), "name": p.stem, "documents": len(b["documents"])})
        except BookError:
            continue
    return out


def project_documents() -> list[dict]:
    out = []
    for p in sorted(config.PROJECTS_DIR.rglob(f"*{config.FDES_EXT}"), key=lambda x: x.stat().st_mtime, reverse=True):
        out.append({"path": str(p), "name": p.stem, "modified": p.stat().st_mtime})
    return out[:200]


def load(path: str) -> dict:
    p = Path(path)
    try:
        b = json.loads(p.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise BookError(f"cannot read the book {p.name}: {exc}") from exc
    if b.get("format") != "formdes-book" or not isinstance(b.get("documents"), list):
        raise BookError(f"{p.name} is not a formdes book")
    b["styleSource"] = int(b.get("styleSource") or 0)
    return b


def save(path: str, b: dict) -> None:
    b = {"format": "formdes-book", "version": 1, "documents": b["documents"], "styleSource": b.get("styleSource", 0)}
    Path(path).write_text(json.dumps(b, indent=2), encoding="utf-8")


def create(name: str) -> str:
    BOOKS_DIR.mkdir(parents=True, exist_ok=True)
    p = BOOKS_DIR / f"{_safe_name(name)}{BOOK_EXT}"
    i = 2
    while p.exists():
        p = BOOKS_DIR / f"{_safe_name(name)} {i}{BOOK_EXT}"
        i += 1
    save(str(p), {"documents": [], "styleSource": 0})
    return str(p)


def add(path: str, doc: str, at: int | None = None) -> None:
    b = load(path)
    d = str(Path(doc).expanduser())
    if not Path(d).is_file():
        raise BookError(f"no such document: {d}")
    if d in b["documents"]:
        raise BookError(f"{Path(d).name} is already in the book")
    b["documents"].insert(len(b["documents"]) if at is None else max(0, at), d)
    save(path, b)


def remove(path: str, index: int) -> None:
    b = load(path)
    if not 0 <= index < len(b["documents"]):
        raise BookError("no such chapter")
    b["documents"].pop(index)
    if b["styleSource"] >= len(b["documents"]):
        b["styleSource"] = 0
    save(path, b)


def move(path: str, index: int, to: int) -> None:
    b = load(path)
    docs = b["documents"]
    if not 0 <= index < len(docs):
        raise BookError("no such chapter")
    src = docs[b["styleSource"]] if docs else None
    d = docs.pop(index)
    docs.insert(max(0, min(len(docs), to)), d)
    b["styleSource"] = docs.index(src) if src in docs else 0
    save(path, b)


def set_source(path: str, index: int) -> None:
    b = load(path)
    if not 0 <= index < len(b["documents"]):
        raise BookError("no such chapter")
    b["styleSource"] = index
    save(path, b)


def _inner(p: Path) -> bytes:
    data = p.read_bytes()
    if p.suffix.lower() == config.FDES_EXT:
        with zipfile.ZipFile(io.BytesIO(data)) as z:
            return z.read(INNER)
    return data


def _write_back(p: Path, inner: bytes) -> bool:
    if p.suffix.lower() != config.FDES_EXT:
        if p.read_bytes() == inner:
            return False
        p.write_bytes(inner)
        return True
    data = p.read_bytes()
    with zipfile.ZipFile(io.BytesIO(data)) as z:
        if z.read(INNER) == inner:
            return False
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w") as out:
            for info in z.infolist():
                body = inner if info.filename == INNER else z.read(info.filename)
                out.writestr(info, body, compress_type=info.compress_type)
    p.write_bytes(buf.getvalue())
    return True


async def _exec(command: str, params: dict):
    return (await core.call("exec", {"command": command, "params": params}, source="book")).result


async def _open_paths() -> dict[str, dict]:
    ov = await documents.overview()
    return {str(Path(d["path"]).resolve()): d for d in ov.get("docs", []) if d.get("path")}


async def run(path: str, steps: list[tuple[str, dict]], write_back: bool) -> dict:
    b = load(path)
    docs = [Path(d) for d in b["documents"]]
    missing = [d.name for d in docs if not d.is_file()]
    if missing:
        raise BookError("missing chapters: " + ", ".join(missing))
    opened = await _open_paths()
    if write_back:
        dirty = [d.name for d in docs if opened.get(str(d.resolve()), {}).get("dirty")]
        if dirty:
            raise BookError("save these chapters first: " + ", ".join(dirty))
    with tempfile.TemporaryDirectory(prefix="formdes-book-") as td:
        temps = []
        for i, d in enumerate(docs):
            t = Path(td) / f"{i:03d}.fdoc"
            t.write_bytes(_inner(d))
            temps.append(t)
        results = []
        try:
            await _exec("book.new", {"path": str(Path(td) / "book.fdbook")})
            for t in temps:
                await _exec("book.add", {"path": str(t)})
            if temps:
                await _exec("book.styleSource", {"index": min(b["styleSource"], len(temps) - 1)})
            for cmd, params in steps:
                results.append(await _exec(cmd, params))
            listing = await _exec("book.list", {})
        except CoreError as exc:
            raise BookError(str(exc)) from exc
        finally:
            try:
                await _exec("book.close", {})
            except CoreError:
                pass
        changed = []
        if write_back:
            for d, t in zip(docs, temps):
                if _write_back(d, t.read_bytes()):
                    changed.append(d.name)
    chapters = []
    for i, d in enumerate(docs):
        row = (listing or {}).get("documents", [])[i] if listing and i < len(listing.get("documents", [])) else {}
        chapters.append({"path": str(d), "name": d.stem, "pages": row.get("pages"), "firstPage": row.get("firstPage"),
                         "open": str(d.resolve()) in opened})
    stale = [c["name"] for c in chapters if c["open"] and Path(c["path"]).name in changed]
    return {"path": path, "name": Path(path).stem, "styleSource": b["styleSource"], "chapters": chapters, "results": results,
            "changed": changed, "reopen": stale}


async def info(path: str) -> dict:
    return await run(path, [], write_back=False)


async def export_pdf(path: str) -> tuple[dict, Path]:
    out = config.EXPORTS_DIR / f"{Path(path).stem}.pdf"
    out.parent.mkdir(parents=True, exist_ok=True)
    r = await run(path, [("book.exportPdf", {"path": str(out)})], write_back=False)
    return r, out
