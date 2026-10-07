import asyncio
import base64
import io
import json
import logging
import time
import zipfile
from dataclasses import asdict, dataclass, field
from pathlib import Path

from app import __version__
from app.core import config
from app.engine.core import CoreError, core

log = logging.getLogger("formdes.documents")

LAYOUT_EXTS = {".fdes", ".fdoc", ".idml"}
EXPORTS = {
    "pdf": ("file.exportPdf", ".pdf", "application/pdf"),
    "idml": ("file.exportIdml", ".idml", "application/vnd.adobe.indesign-idml-package"),
    "epub": ("file.exportEpub", ".epub", "application/epub+zip"),
    "epub-fixed": ("file.exportFixedEpub", ".epub", "application/epub+zip"),
    "html": ("file.exportHtml", ".html", "text/html"),
    "txt": ("file.exportText", ".txt", "text/plain"),
    "rtf": ("file.exportText", ".rtf", "application/rtf"),
    "xml": ("file.exportXml", ".xml", "application/xml"),
    "booklet": ("file.printBooklet", ".pdf", "application/pdf"),
}


@dataclass
class DocMeta:
    uid: int
    path: str | None = None
    template: dict | None = None
    brand: dict | None = None
    brief: str = ""
    pack_id: str | None = None
    slots: dict = field(default_factory=dict)
    created: float = field(default_factory=time.time)
    modified: float = field(default_factory=time.time)


class Documents:
    def __init__(self) -> None:
        self.meta: dict[int, DocMeta] = {}
        self.recent_file = config.DATA_DIR / "recent.json"
        self._autosave: asyncio.Task | None = None

    async def active_uid(self) -> int | None:
        st = await core.state()
        i = st.get("active")
        if i is None:
            return None
        return st["docs"][i]["uid"]

    async def meta_for_active(self) -> DocMeta | None:
        uid = await self.active_uid()
        if uid is None:
            return None
        return self.meta.setdefault(uid, DocMeta(uid))

    async def overview(self) -> dict:
        st = await core.state()
        docs = []
        for d in st.get("docs", []):
            m = self.meta.get(d["uid"])
            title = Path(m.path).name if m and m.path else d.get("title")
            docs.append({**d, "title": title, "path": (m.path if m and m.path else d.get("path")),
                         "template": m.template.get("id") if m and m.template else None,
                         "brand": m.brand.get("name") if m and m.brand else None})
        return {"active": st.get("active"), "docs": docs, "rev": st.get("rev")}

    async def new(self, params: dict, *, source: str = "engine") -> dict:
        await core.call("exec", {"command": "file.new", "params": params}, source=source)
        uid = await self.active_uid()
        if uid is not None:
            self.meta[uid] = DocMeta(uid)
        return await self.overview()

    async def adopt(self, *, template: dict | None = None, brand: dict | None = None, brief: str = "", pack_id: str | None = None,
                    slots: dict | None = None) -> DocMeta | None:
        uid = await self.active_uid()
        if uid is None:
            return None
        m = DocMeta(uid, template=template, brand=brand, brief=brief, pack_id=pack_id, slots=slots or {})
        self.meta[uid] = m
        return m

    async def open_path(self, path: str, *, source: str = "engine") -> dict:
        p = Path(path).expanduser()
        if not p.is_file():
            raise CoreError(f"no file at {p}")
        return await self.open_bytes(p.name, p.read_bytes(), path=str(p), source=source)

    async def open_bytes(self, name: str, data: bytes, *, path: str | None = None, source: str = "engine") -> dict:
        ext = Path(name).suffix.lower()
        meta_json: dict = {}
        if ext == ".fdes" or (data[:2] == b"PK" and _is_fdes(data)):
            with zipfile.ZipFile(io.BytesIO(data)) as z:
                inner = z.read("document.fdoc")
                if "formdes.json" in z.namelist():
                    meta_json = json.loads(z.read("formdes.json"))
            await core.call("exec", {"command": "file.openBytes", "params": {"name": Path(name).stem + ".fdoc", "base64": base64.b64encode(inner).decode()}}, source=source)
        elif ext == ".idml":
            await core.call("exec", {"command": "file.openIdml", "params": {"base64": base64.b64encode(data).decode(), "name": name}}, source=source)
        elif ext == ".fdoc" or data[:2] == b"PK" or data[:1] == b"{":
            await core.call("exec", {"command": "file.openBytes", "params": {"name": name, "base64": base64.b64encode(data).decode()}}, source=source)
        else:
            raise CoreError(f"{name} is not a layout document (.fdes, .fdoc or .idml); place it or make a layout from it instead")
        uid = await self.active_uid()
        if uid is not None:
            m = DocMeta(uid, path=path if ext == ".fdes" else None)
            for k in ("template", "brand", "brief", "slots", "created"):
                if k in meta_json:
                    setattr(m, k, meta_json[k])
            m.pack_id = meta_json.get("pack")
            self.meta[uid] = m
            if path:
                self.remember(path)
        return await self.overview()

    async def serialize_fdes(self, pack: dict | None = None) -> bytes:
        r = await core.call("serialize")
        m = await self.meta_for_active()
        if m is None:
            raise CoreError("no document open")
        m.modified = time.time()
        meta = {"format": "formdes", "version": 1, "app": __version__, "title": r.result.get("title"),
                "template": m.template, "brand": m.brand, "brief": m.brief, "pack": m.pack_id, "slots": m.slots,
                "created": m.created, "modified": m.modified}
        thumb = None
        try:
            thumb = (await core.call("page.png", {"page": 0, "maxSide": 512})).bin
        except CoreError:
            pass
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w") as z:
            z.writestr(zipfile.ZipInfo("mimetype"), config.FDES_MIME, compress_type=zipfile.ZIP_STORED)
            z.writestr("formdes.json", json.dumps(meta, indent=2), compress_type=zipfile.ZIP_DEFLATED)
            z.writestr("document.fdoc", r.bin or b"", compress_type=zipfile.ZIP_STORED)
            if pack is not None:
                z.writestr("content/pack.json", json.dumps(pack), compress_type=zipfile.ZIP_DEFLATED)
            if thumb:
                z.writestr("thumbs/page-1.png", thumb, compress_type=zipfile.ZIP_STORED)
        return buf.getvalue()

    async def save(self, path: str | None = None, *, source: str = "engine") -> dict:
        m = await self.meta_for_active()
        if m is None:
            raise CoreError("no document open")
        target = path or m.path
        if not target:
            st = await core.state()
            title = st["docs"][st["active"]]["title"] if st.get("active") is not None else "Untitled"
            target = str(config.PROJECTS_DIR / f"{Path(title).stem}{config.FDES_EXT}")
        p = Path(target).expanduser()
        ext = p.suffix.lower()
        p.parent.mkdir(parents=True, exist_ok=True)
        if ext == ".fdoc":
            await core.call("exec", {"command": "file.saveAs", "params": {"path": str(p)}}, source=source)
        elif ext == ".idml":
            await core.call("exec", {"command": "file.exportIdml", "params": {"path": str(p)}}, source=source)
        else:
            if ext != config.FDES_EXT:
                p = p.with_suffix(config.FDES_EXT)
            from app.intake.store import packs
            pack = packs.get(m.pack_id) if m.pack_id else None
            p.write_bytes(await self.serialize_fdes(pack))
            m.path = str(p)
            await core.call("docs.saved", {"path": str(p)}, source=source)
        self.remember(str(p))
        return {"path": str(p), "bytes": p.stat().st_size}

    async def export(self, fmt: str, path: str | None = None, options: dict | None = None, *, source: str = "engine") -> tuple[dict, bytes | None, str]:
        options = dict(options or {})
        if fmt in ("png", "jpg", "jpeg"):
            page = int(options.get("page", 0))
            params = {"page": page, "scale": float(options.get("scale", 2.0)), "bleed": bool(options.get("bleed", False)), "format": "jpeg" if fmt != "png" else "png"}
            r = await core.call("page.png", params, source=source)
            mime = r.result.get("mime", "image/png")
            if path:
                Path(path).expanduser().write_bytes(r.bin or b"")
                return {"path": path, "bytes": len(r.bin or b"")}, None, mime
            return r.result, r.bin, mime
        if fmt == "fdes":
            data = await self.serialize_fdes()
            if path:
                Path(path).expanduser().write_bytes(data)
                return {"path": path, "bytes": len(data)}, None, config.FDES_MIME
            return {"bytes": len(data)}, data, config.FDES_MIME
        if fmt not in EXPORTS:
            raise CoreError(f"unknown export format {fmt}: one of {', '.join(sorted([*EXPORTS, 'png', 'jpg', 'fdes', 'web']))}")
        command, ext, mime = EXPORTS[fmt]
        if fmt in ("txt", "rtf"):
            options.setdefault("format", fmt)
        if path:
            options["path"] = str(Path(path).expanduser())
        result = await core.exec(command, options, source=source)
        if path:
            return result, None, mime
        data = base64.b64decode(result.get("base64", "")) if isinstance(result, dict) and result.get("base64") else None
        if data is None and isinstance(result, dict) and isinstance(result.get("text"), str):
            data = result["text"].encode()
        return {k: v for k, v in (result or {}).items() if k != "base64"}, data, mime

    def remember(self, path: str) -> None:
        items = self.recent()
        items = [i for i in items if i.get("path") != path]
        p = Path(path)
        items.insert(0, {"name": p.name, "path": path, "modified": int(time.time() * 1000)})
        try:
            self.recent_file.parent.mkdir(parents=True, exist_ok=True)
            self.recent_file.write_text(json.dumps(items[:20], indent=1))
        except OSError as exc:
            log.warning("recent list not written: %s", exc)

    def recent(self) -> list[dict]:
        try:
            return [i for i in json.loads(self.recent_file.read_text()) if isinstance(i, dict)]
        except (OSError, ValueError):
            return []

    def start_autosave(self) -> None:
        if self._autosave is None:
            self._autosave = asyncio.create_task(self._autosave_loop())

    async def _autosave_loop(self) -> None:
        recovery = config.CACHE_DIR / "recovery"
        while True:
            await asyncio.sleep(60)
            try:
                st = await core.state()
                if any(d.get("dirty") for d in st.get("docs", [])):
                    recovery.mkdir(parents=True, exist_ok=True)
                    await core.call("exec", {"command": "file.recovery.save", "params": {"dir": str(recovery)}}, source="autosave")
            except CoreError as exc:
                log.warning("recovery save failed: %s", exc)


def _is_fdes(data: bytes) -> bool:
    try:
        with zipfile.ZipFile(io.BytesIO(data)) as z:
            return "formdes.json" in z.namelist() and "document.fdoc" in z.namelist()
    except zipfile.BadZipFile:
        return False


documents = Documents()


def meta_dict(m: DocMeta) -> dict:
    return asdict(m)
