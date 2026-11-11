import asyncio
import base64
import html as htmllib
import io
import re
import time
import zipfile
from pathlib import Path
from xml.sax.saxutils import escape, quoteattr

from app.engine.core import CoreError, core
from app.engine.documents import documents
from app.services import motion as M
from app.services.website import slugify

CORE_TYPES = {"video/mp4", "audio/mpeg", "audio/mp4", "audio/ogg", "audio/aac", "audio/x-m4a"}
TOP = re.compile(r"<(p|h[1-6]|ul|ol|table|blockquote|figure|div|pre|section)\b[^>]*>.*?</\1>", re.S)
STYLE = ".fd-media{margin:1em 0}.fd-media video,.fd-media audio{display:block;max-width:100%;width:100%}.fd-media video{height:auto;background:#000}"


def _plain(p: dict) -> str:
    return "".join(r.get("t", "") for r in p.get("runs", []))


def _norm(s: str) -> str:
    return re.sub(r"\s+", " ", htmllib.unescape(re.sub(r"<[^>]+>", "", s))).strip()


class Item:
    def __init__(self, block: dict, index: int) -> None:
        self.block = block
        self.index = index
        self.id = int(block["item"])
        self.kind = block["kind"]
        self.name = block.get("name") or ("media" + (".mp4" if self.kind == "video" else ".mp3"))
        self.mime = block.get("mime") or ("video/mp4" if self.kind == "video" else "audio/mpeg")
        self.data = b""
        self.opts: dict = {}
        self.file = ""
        self.poster_file = ""
        self.poster: bytes | None = None
        self.alt = block.get("alt") or ""


async def gather(meta) -> tuple[list[dict], list[Item]]:
    web = (await core.call("web")).result
    blocks = web.get("blocks", [])
    out: list[Item] = []
    used: set[str] = set()
    for i, b in enumerate(blocks):
        if b.get("kind") not in ("video", "audio") or b.get("asset") is None:
            continue
        it = Item(b, i)
        r = await core.call("web.asset", {"asset": b["asset"]})
        it.data = r.bin or b""
        it.mime = r.result.get("mime") or it.mime
        ext = Path(r.result.get("name") or it.name).suffix.lower() or (".mp4" if it.kind == "video" else ".mp3")
        stem = slugify(Path(r.result.get("name") or it.name).stem, fallback="media")[:40]
        name = f"{stem}-{b['asset']}{ext}"
        while name in used:
            name = f"{stem}-{b['asset']}-{len(used)}{ext}"
        used.add(name)
        it.file = name
        it.opts = await M.media_for(meta, it.id)
        if it.kind == "video":
            got = await M.poster_bytes(it.opts, it.data, ext)
            if got:
                it.poster = got[0]
                it.poster_file = f"{stem}-{b['asset']}-poster{got[1]}"
        out.append(it)
    return blocks, out


def media_tag(it: Item, base: str, extra: str = "", style: str = "", xml: bool = False) -> str:
    o = it.opts

    def flag(n: str) -> str:
        return f' {n}="{n}"' if xml else f" {n}"
    attrs = ""
    if o.get("controls", True):
        attrs += flag("controls")
    if o.get("autoplay"):
        attrs += flag("autoplay")
        if it.kind == "video":
            attrs += flag("muted")
    if o.get("loop"):
        attrs += flag("loop")
    if it.kind == "video":
        attrs += flag("playsinline")
        if it.poster:
            attrs += f' poster={quoteattr(base + it.poster_file)}'
    label = escape(it.alt or Path(it.name).stem)
    st = f" style={quoteattr(style)}" if style else ""
    return (f'<{it.kind}{attrs}{extra}{st} preload="metadata" src={quoteattr(base + it.file)}>'
            f'<source src={quoteattr(base + it.file)} type={quoteattr(it.mime)}/>{label}</{it.kind}>')


def weave(body: str, blocks: list[dict], inserts: dict[int, str], wraps: dict[int, str]) -> str:
    """Put media after the paragraph that precedes it on the page, and wrap animated stories, matching paragraph text."""
    els = [(m.start(), m.end(), _norm(m.group(0))) for m in TOP.finditer(body)]
    spans: dict[int, tuple[int, int]] = {}
    pointer = 0

    def find(text: str, start: int) -> int | None:
        for k in range(start, len(els)):
            if els[k][2] == text:
                return k
        for k in range(0, start):
            if els[k][2] == text:
                return k
        return None

    for bi, b in enumerate(blocks):
        if b.get("kind") != "story":
            continue
        paras = [_norm(_plain(p)) for p in b.get("paras", []) if _plain(p).strip()]
        if not paras:
            continue
        first = find(paras[0], pointer)
        if first is None:
            continue
        last = find(paras[-1], first)
        if last is None or last < first:
            last = first
        spans[bi] = (first, last)
        pointer = last + 1
    edits: list[tuple[int, int, str]] = []
    bm = re.search(r"<body\b[^>]*>", body)
    start_pos = bm.end() if bm else 0
    end_pos = body.rfind("</body>") if "</body>" in body else len(body)
    for bi, html in inserts.items():
        prev = [k for k in spans if k < bi]
        if prev:
            edits.append((els[spans[max(prev)][1]][1], 1, "\n" + html))
        else:
            edits.append((els[0][0] if els else start_pos, 1, html + "\n"))
    for bi, fid in wraps.items():
        if bi in spans:
            a, z = spans[bi]
            edits.append((els[a][0], 2, f'<div class="fd-mo" data-fd="{fid}">'))
            edits.append((els[z][1], 0, "</div>"))
    out, last = [], 0
    for pos, _, text in sorted(edits, key=lambda e: (e[0], e[1])):
        out.append(body[last:pos])
        out.append(text)
        last = pos
    out.append(body[last:])
    return "".join(out) if edits else body


def _inject_head(doc: str, css: str, scripts: str = "") -> str:
    doc = doc.replace("</head>", f"<style>{css}</style></head>", 1) if "</head>" in doc else doc
    if scripts:
        doc = doc.replace("</body>", scripts + "</body>", 1)
    return doc


def _xhtml_script(cfg: dict) -> str:
    data = M.safe_json(cfg)
    return (f'<script type="application/json" id="fd-motion-data">{data}</script>'
            f'<script>//<![CDATA[\n{M.RUNTIME}\n//]]></script>')


def _wrap_ids(blocks: list[dict], anims: dict, geo: dict) -> dict[int, str]:
    out: dict[int, str] = {}
    for bi, b in enumerate(blocks):
        if b.get("kind") != "story":
            continue
        for fid in b.get("frames") or [b.get("item")]:
            if str(fid) in anims:
                out[bi] = str(fid)
                break
    return out


async def html_export(meta, native: dict, path: str | None, options: dict) -> tuple[dict, bytes | None, str]:
    text = native.get("text") or ""
    blocks, items = await gather(meta)
    cfg = await M.export_config(meta, "flow")
    if not items and not cfg:
        raise _Unchanged()
    stem = Path(path).stem if path else "index"
    zipped = path is None or str(path).lower().endswith(".zip")
    base = "media/" if zipped else f"{stem}_media/"
    inserts = {}
    for it in items:
        tag = media_tag(it, base, f' data-fd="{it.id}"' if cfg and str(it.id) in cfg["anims"] else "")
        inserts[it.index] = f'<figure class="fd-media">{tag}</figure>'
    wraps = _wrap_ids(blocks, cfg["anims"], {}) if cfg else {}
    body = weave(text, blocks, inserts, wraps)
    scripts = ""
    if cfg:
        scripts = M.page_script(cfg)
    doc = _inject_head(body, STYLE, scripts)
    files: dict[str, bytes] = {}
    for it in items:
        files[it.file] = it.data
        if it.poster:
            files[it.poster_file] = it.poster
    info = {"media": [it.file for it in items], "animations": len(cfg["anims"]) if cfg else 0}
    if zipped:
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w") as z:
            z.writestr("index.html", doc, compress_type=zipfile.ZIP_DEFLATED)
            for n, d in files.items():
                z.writestr("media/" + n, d, compress_type=zipfile.ZIP_STORED)
        data = buf.getvalue()
        if path:
            Path(path).expanduser().write_bytes(data)
            return {**info, "path": path, "bytes": len(data)}, None, "application/zip"
        return {**info, "bytes": len(data)}, data, "application/zip"
    p = Path(path).expanduser()
    p.write_text(doc, encoding="utf-8")
    folder = p.parent / f"{stem}_media"
    if files:
        folder.mkdir(parents=True, exist_ok=True)
        for n, d in files.items():
            (folder / n).write_bytes(d)
    return {**info, "path": str(p), "media_folder": str(folder) if files else None, "bytes": len(doc.encode())}, None, "text/html"


class _Unchanged(Exception):
    pass


def _media_type(it: Item, warnings: list[str]) -> str:
    if it.mime.split(";")[0] not in CORE_TYPES:
        warnings.append(f"{it.file}: {it.mime} is not an EPUB core media type; some reading systems will not play it (use MP4/H.264 video or MP3/AAC audio)")
    return it.mime.split(";")[0]


def _manifest_add(opf: str, entries: list[str]) -> str:
    return opf.replace("</manifest>", "".join(entries) + "</manifest>", 1)


def _scripted(opf: str, href: str) -> str:
    pat = re.compile(r'(<item\b[^>]*href="' + re.escape(href) + r'"[^>]*?)(/>)')

    def add(m: re.Match) -> str:
        item = m.group(1)
        if re.search(r'\bproperties="[^"]*\bscripted\b', item):
            return m.group(0)
        if 'properties="' in item:
            return re.sub(r'properties="([^"]*)"', lambda p: f'properties="{p.group(1)} scripted"', item, count=1) + m.group(2)
        return item + ' properties="scripted"' + m.group(2)

    return pat.sub(add, opf, count=1)


async def epub_export(meta, native: dict, fmt: str, path: str | None, options: dict) -> tuple[dict, bytes | None, str]:
    raw = base64.b64decode(native.get("base64", ""))
    blocks, items = await gather(meta)
    cfg = await M.export_config(meta, "page") if fmt == "epub-fixed" else None
    if not items and not cfg:
        raise _Unchanged()
    src = zipfile.ZipFile(io.BytesIO(raw))
    names = src.namelist()
    texts = {n: src.read(n) for n in names}
    warnings: list[str] = list(native.get("warnings") or [])
    opf_name = next(n for n in names if n.endswith(".opf"))
    opf = texts[opf_name].decode()
    root = opf_name.rsplit("/", 1)[0] + "/" if "/" in opf_name else ""
    added: dict[str, bytes] = {}
    manifest: list[str] = []
    for n, it in enumerate(items):
        mt = _media_type(it, warnings)
        added[f"{root}media/{it.file}"] = it.data
        manifest.append(f'<item id="media{n}" href="media/{escape(it.file)}" media-type="{mt}"/>')
        if it.poster:
            added[f"{root}media/{it.poster_file}"] = it.poster
            ptype = "image/png" if it.poster_file.endswith(".png") else "image/jpeg"
            manifest.append(f'<item id="poster{n}" href="media/{escape(it.poster_file)}" media-type="{ptype}"/>')
    scripted: list[str] = []
    if fmt == "epub-fixed":
        geo = await M.geometry()
        from PIL import Image
        by_page: dict[int, list[Item]] = {}
        for it in items:
            if it.id in geo:
                by_page.setdefault(geo[it.id]["page"], []).append(it)
        anim_pages: dict[int, list[int]] = {}
        for k in (cfg or {}).get("anims", {}):
            anim_pages.setdefault(geo[int(k)]["page"], []).append(int(k))
        counter = 0
        for page in sorted(set(by_page) | set(anim_pages)):
            fname = f"{root}page{page + 1}.xhtml"
            if fname not in texts:
                continue
            doc = texts[fname].decode()
            extra = ""
            css = ".fd-m{position:absolute;z-index:4}.fd-a{position:absolute;z-index:3}.t{pointer-events:none}"
            ids = sorted(anim_pages.get(page, []))
            plate_name = f"{root}images/page{page + 1}.png"
            if ids and plate_name in texts:
                dims = Image.open(io.BytesIO(texts[plate_name])).size
                g0 = geo[ids[0]]
                scale = dims[0] / max(1.0, g0["pageW"])
                plate, shots = await M.page_layers(page, ids, scale)
                added[plate_name] = plate
                texts.pop(plate_name, None)
                for i in ids:
                    if i in shots and not any(it.id == i for it in by_page.get(page, [])):
                        g = geo[i]
                        fn = f"images/page{page + 1}-a{i}.png"
                        added[root + fn] = shots[i]
                        manifest.append(f'<item id="anim{counter}" href="{fn}" media-type="image/png"/>')
                        counter += 1
                        extra += f'<img class="fd-a" data-fd="{i}" src="{fn}" alt="" style="left:{g["x"]:.2f}px;top:{g["y"]:.2f}px;width:{g["w"]:.2f}px;height:{g["h"]:.2f}px"/>'
            for it in by_page.get(page, []):
                g = geo[it.id]
                tag = media_tag(it, "media/", f' class="fd-m"' + (f' data-fd="{it.id}"' if cfg and str(it.id) in cfg["anims"] else ""),
                                f"left:{g['x']:.2f}px;top:{g['y']:.2f}px;width:{g['w']:.2f}px;height:{g['h']:.2f}px" + (";object-fit:contain;background:#000" if it.kind == "video" else ""), True)
                extra += tag
            scripts = ""
            if cfg and ids:
                pc = await M.export_config(meta, "page", page)
                if pc:
                    scripts = _xhtml_script(pc)
                    scripted.append(f"page{page + 1}.xhtml")
            doc = doc.replace("</body>", extra + scripts + "</body>", 1)
            doc = doc.replace("</head>", f"<style>{css}</style></head>", 1)
            texts[fname] = doc.encode()
    else:
        text_name = next((n for n in names if n.endswith("text.xhtml")), None)
        if text_name:
            inserts = {it.index: f'<figure class="fd-media">{media_tag(it, "media/", "", "", True)}</figure>' for it in items}
            doc = weave(texts[text_name].decode(), blocks, inserts, {})
            doc = doc.replace("</head>", f"<style>{STYLE}</style></head>", 1)
            texts[text_name] = doc.encode()
    opf = _manifest_add(opf, manifest).replace("<dc:creator></dc:creator>", "")
    opf = re.sub(r'(<meta property="dcterms:modified">)[^<]*(</meta>)', lambda m: m.group(1) + time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()) + m.group(2), opf)
    for s in scripted:
        opf = _scripted(opf, s)
    texts[opf_name] = opf.encode()
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        for n in names:
            if n == "mimetype" or n not in texts:
                continue
            z.writestr(n, texts[n], compress_type=zipfile.ZIP_DEFLATED if n.endswith((".xhtml", ".opf", ".css", ".xml")) else zipfile.ZIP_STORED)
        for n, d in added.items():
            z.writestr(n, d, compress_type=zipfile.ZIP_STORED)
    data = buf.getvalue()
    info = {"bytes": len(data), "media": [it.file for it in items], "animations": len((cfg or {}).get("anims", {})), "warnings": warnings}
    if "pages" in native:
        info["pages"] = native["pages"]
    if path:
        Path(path).expanduser().write_bytes(data)
        return {**info, "path": path}, None, "application/epub+zip"
    return info, data, "application/epub+zip"


async def post(fmt: str, native: dict, path: str | None, options: dict):
    """Returns (info, bytes, mime) when the document has media or animations, otherwise None."""
    meta = await documents.meta_for_active()
    try:
        if fmt == "html":
            return await html_export(meta, native, path, options)
        return await epub_export(meta, native, fmt, path, options)
    except _Unchanged:
        return None
