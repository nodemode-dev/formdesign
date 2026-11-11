import asyncio
import ctypes
import hashlib
import math
import re
import shutil
import statistics
import tempfile
import zipfile
from pathlib import Path
from xml.etree import ElementTree as ET

import pypdfium2 as pdfium
import pypdfium2.raw as raw

from app.core import config
from app.engine.core import CoreError, core
from app.engine.documents import documents

WORK = config.CACHE_DIR / "layout-import"
SRC = "layoutimport"
CM = 72.0 / 2.54

QXP_EXTS = {".qxd", ".qxp"}
PM_EXTS = {".pmd", ".p65", ".pm6", ".pm5", ".pm4", ".pm3", ".pt5", ".pt6"}
PUB_EXTS = {".pub"}
SOFFICE_EXTS = QXP_EXTS | PM_EXTS | PUB_EXTS
QXP_VERSIONS = ("LibreOffice's libqxp reader reads .qxd and .qxp files saved in format versions 3.1 to 4.1; files from "
                "later versions have no open reader, so export a PDF from the application that made the file and open that as a layout")


def qxp_signature(path: Path) -> bool:
    """A .qxd or .qxp file carries MM or II (byte order), then XPR, from byte 2 (the system file(1) magic: MMXPR3, IIXPRa)."""
    try:
        with path.open("rb") as fh:
            head = fh.read(8)
    except OSError:
        return False
    return len(head) >= 7 and head[2:4] in (b"MM", b"II") and head[4:7] == b"XPR"
DXF_EXTS = {".dxf"}
DWG_EXTS = {".dwg"}
OPEN_EXTS = SOFFICE_EXTS | DXF_EXTS | {".pdf"}
INDD_EXTS = {".indd", ".indt", ".indb", ".inx"}

SANS = ["Liberation Sans", "Inter", "FreeSans", "DejaVu Sans"]
SERIF = ["Liberation Serif", "Source Serif 4", "FreeSerif", "DejaVu Serif"]
MONO = ["Liberation Mono", "FreeMono", "DejaVu Sans Mono"]
ALIASES = [
    (("arial", "helvetica", "swiss", "nimbussans", "arimo", "verdana", "tahoma", "segoe", "myriad", "frutiger", "univers", "opensans", "roboto", "lato"), SANS),
    (("lmmono", "mono", "typewriter", "lucidacons"), ["Latin Modern Mono"] + MONO),
    (("lmsans", "lmsf"), ["Latin Modern Sans"] + SANS),
    (("calibri", "carlito"), ["Carlito"] + SANS),
    (("cambria", "caladea"), ["Caladea"] + SERIF),
    (("times", "timesnewroman", "nimbusroman", "tinos", "georgia", "minion", "palatino", "bookantiqua", "garamond", "baskerville", "caslon", "bodoni", "century", "cmr", "lmroman"), SERIF),
    (("courier", "couriernew", "consolas", "monaco", "menlo", "lucidaconsole", "cousine"), MONO),
]

_families: list[str] = []
_styles: dict[str, list[str]] = {}


def norm(name: str) -> str:
    return re.sub(r"[^a-z0-9]", "", name.lower())


async def families() -> list[str]:
    global _families
    if not _families:
        r = await core.call("fonts.families", source=SRC)
        _families = list(r.result or [])
    return _families


async def styles_of(family: str) -> list[str]:
    if family not in _styles:
        r = await core.call("fonts.styles", {"family": family}, source=SRC)
        _styles[family] = list(r.result or [])
    return _styles[family]


async def map_font(name: str, bold: bool, italic: bool, flags: int = 0) -> tuple[str, str, bool]:
    """Source font name to an installed (family, style, exact) triple."""
    fams = await families()
    base = re.sub(r"^[A-Z]{6}\+", "", name or "")
    key = norm(base)
    words = key
    if re.search(r"bold|black|heavy|semibold|demi", key):
        bold = True
    if re.search(r"italic|oblique|ital$", key):
        italic = True
    exact = False
    family = None
    for f in sorted(fams, key=lambda f: -len(norm(f))):
        nf = norm(f)
        if nf and (words.startswith(nf) or nf == words):
            family, exact = f, True
            break
    if family is None:
        for keys, cands in ALIASES:
            if any(words.startswith(k) or k in words for k in keys):
                family = next((c for c in cands if c in fams), None)
                break
    if family is None:
        if flags & 1:
            pool = MONO
        elif flags & 2 or re.search(r"serif(?!.*sans)|roman|mincho|song", key):
            pool = SERIF
        else:
            pool = SANS
        family = next((c for c in pool if c in fams), fams[0] if fams else "Inter")
    sts = await styles_of(family)
    want = "Bold Italic" if bold and italic else "Bold" if bold else "Italic" if italic else "Regular"
    style = want if want in sts else None
    if style is None:
        pick = [s for s in sts if (("Bold" in s) == bold and ("Italic" in s or "Oblique" in s) == italic)]
        pick.sort(key=lambda s: (len(s), s))
        style = pick[0] if pick else (sts[0] if sts else "Regular")
    return family, style, exact


_ascent: dict[tuple[str, str], float] = {}


def ascent_of(family: str, style: str) -> float:
    """hhea ascent of the installed font in em, which is where the core puts the first baseline below a frame's top."""
    key = (family, style)
    if key in _ascent:
        return _ascent[key]
    val = 0.93
    try:
        from fontTools.ttLib import TTFont
        import subprocess
        want = norm(family + style)
        found = None
        roots = [Path(__file__).resolve().parents[1] / "fonts", config.FONTS_USER_DIR]
        for root in roots:
            if root.is_dir():
                for f in root.iterdir():
                    if f.suffix.lower() in (".ttf", ".otf") and norm(f.stem) == want:
                        found = f
                        break
            if found:
                break
        if found is None:
            out = subprocess.run(["fc-match", "-f", "%{file}", f"{family}:style={style}"], capture_output=True, text=True, timeout=5).stdout.strip()
            if out and Path(out).is_file():
                found = Path(out)
        if found is not None:
            t = TTFont(str(found), lazy=True, fontNumber=0)
            val = t["hhea"].ascent / t["head"].unitsPerEm
    except Exception:
        pass
    _ascent[key] = val
    return val


def hexcolor(r: int, g: int, b: int) -> str:
    return "#%02x%02x%02x" % (r & 255, g & 255, b & 255)


def ubytes(text: str, upto: int) -> int:
    return len(text[:upto].encode("utf-8"))


def clean_text(t: str) -> str:
    return "".join(ch for ch in t.replace(" ", " ") if ch == "\t" or ord(ch) >= 32 and ord(ch) not in (0x2028, 0x2029, 0xFFFE, 0xFFFF)).replace("�", "")


class Sheet:
    """Everything one imported document needs, collected before any core call: pages, shapes, text frames, images, backdrops."""

    def __init__(self, title: str) -> None:
        self.title = title
        self.pages: list[dict] = []
        self.report: dict = {"converted": {}, "kept": {}, "notes": [], "fonts": {}}

    def page(self, w: float, h: float) -> dict:
        p = {"w": w, "h": h, "items": [], "backdrop": None}
        self.pages.append(p)
        return p

    def count(self, kind: str, n: int = 1) -> None:
        self.report["converted"][kind] = self.report["converted"].get(kind, 0) + n

    def keep(self, kind: str, n: int = 1) -> None:
        self.report["kept"][kind] = self.report["kept"].get(kind, 0) + n

    def note(self, text: str) -> None:
        if text not in self.report["notes"]:
            self.report["notes"].append(text)


def new_work(tag: str) -> Path:
    WORK.mkdir(parents=True, exist_ok=True)
    return Path(tempfile.mkdtemp(prefix=tag + "-", dir=str(WORK)))


# ---------------------------------------------------------------- PDF reading

def _wide(obj, tp) -> str:
    n = raw.FPDFTextObj_GetText(obj.raw, tp.raw, None, 0)
    if n <= 2:
        return ""
    buf = (ctypes.c_ushort * (n // 2 + 1))()
    raw.FPDFTextObj_GetText(obj.raw, tp.raw, ctypes.cast(buf, ctypes.POINTER(ctypes.c_ushort)), n)
    return bytes(buf).decode("utf-16le", "ignore").split("\x00")[0]


def _color(fn, obj) -> tuple[int, int, int, int]:
    r, g, b, a = (ctypes.c_uint() for _ in range(4))
    if not fn(obj.raw, ctypes.byref(r), ctypes.byref(g), ctypes.byref(b), ctypes.byref(a)):
        return 0, 0, 0, 255
    return r.value, g.value, b.value, a.value


def _font_info(obj) -> tuple[str, int, float, int]:
    f = obj.get_font()
    buf = ctypes.create_string_buffer(256)
    n = raw.FPDFFont_GetBaseFontName(f.raw, buf, 256)
    name = buf.value.decode("latin-1") if n else ""
    if not name:
        n = raw.FPDFFont_GetFamilyName(f.raw, buf, 256)
        name = buf.value.decode("latin-1") if n else ""
    weight = raw.FPDFFont_GetWeight(f.raw)
    ang = ctypes.c_int()
    raw.FPDFFont_GetItalicAngle(f.raw, ctypes.byref(ang))
    flags = raw.FPDFFont_GetFlags(f.raw)
    return name, int(weight), float(ang.value), int(flags)


def _pt(path_obj, seg) -> tuple[float, float, int, bool]:
    x, y = ctypes.c_float(), ctypes.c_float()
    raw.FPDFPathSegment_GetPoint(seg, ctypes.byref(x), ctypes.byref(y))
    return x.value, y.value, raw.FPDFPathSegment_GetType(seg), bool(raw.FPDFPathSegment_GetClose(seg))


def _path_points(obj):
    n = raw.FPDFPath_CountSegments(obj.raw)
    m = obj.get_matrix()
    pts = []
    for i in range(n):
        seg = raw.FPDFPath_GetPathSegment(obj.raw, i)
        x, y, t, close = _pt(obj, seg)
        pts.append((m.a * x + m.c * y + m.e, m.b * x + m.d * y + m.f, t, close))
    return pts, m


def read_pdf_page(pdf, i: int, sheet: Sheet, imgdir: Path, page: dict, backdrop_mode: str) -> list[int]:
    """Collect one PDF page into `page`; returns the top-level object indexes that became editable items."""
    pg = pdf[i]
    W, H = pg.get_size()
    tp = pg.get_textpage()
    taken: list[int] = []
    runs: list[dict] = []
    objs = list(pg.get_objects(max_depth=1))
    shot: dict = {}
    for idx, obj in enumerate(objs):
        t = obj.type
        try:
            if t == raw.FPDF_PAGEOBJ_TEXT:
                text = clean_text(_wide(obj, tp))
                if not text.strip():
                    taken.append(idx)
                    continue
                m = obj.get_matrix()
                if abs(m.b) > 0.02 * max(abs(m.a), 1e-6) or m.a * m.d <= 0:
                    sheet.keep("rotated or mirrored text runs")
                    continue
                fs = obj.get_font_size()
                size = fs * math.hypot(m.c, m.d)
                if size < 1:
                    size = fs * abs(m.a)
                size = round(size, 2)
                l, b, r, tt = obj.get_bounds()
                name, weight, ang, flags = _font_info(obj)
                cr, cg, cb, ca = _color(raw.FPDFPageObj_GetFillColor, obj)
                mode = raw.FPDFTextObj_GetTextRenderMode(obj.raw)
                if mode == 3:
                    taken.append(idx)
                    continue
                if mode in (1, 5):
                    cr, cg, cb, ca = _color(raw.FPDFPageObj_GetStrokeColor, obj)
                runs.append({"text": text, "x0": min(m.e, l), "x1": r, "base": H - m.f, "size": size, "font": name, "bold": weight >= 600,
                             "italic": abs(ang) > 1, "flags": flags, "color": hexcolor(cr, cg, cb), "idx": idx})
                taken.append(idx)
            elif t == raw.FPDF_PAGEOBJ_IMAGE:
                l, b, r, tt = obj.get_bounds()
                if r - l < 1 or tt - b < 1:
                    taken.append(idx)
                    continue
                try:
                    im = obj.get_bitmap(render=False).to_pil()
                except Exception:
                    im = obj.get_bitmap(render=True).to_pil()
                if im.mode not in ("RGB", "RGBA", "L"):
                    im = im.convert("RGB")
                out = imgdir / f"p{i + 1}-i{idx}.png"
                if im.mode != "RGBA" and max(im.size) > 700:
                    out = out.with_suffix(".jpg")
                    im.convert("RGB").save(out, "JPEG", quality=92)
                else:
                    im.save(out, "PNG")
                page["items"].append({"kind": "image", "order": idx, "rect": [l, H - tt, r, H - b], "path": str(out), "px": im.size})
                sheet.count("pictures placed")
                taken.append(idx)
            elif t == raw.FPDF_PAGEOBJ_PATH:
                pts, m = _path_points(obj)
                fill, stroke = ctypes.c_int(), ctypes.c_int()
                raw.FPDFPath_GetDrawMode(obj.raw, ctypes.byref(fill), ctypes.byref(stroke))
                if not fill.value and not stroke.value:
                    taken.append(idx)
                    continue
                if abs(m.b) > 1e-3 or abs(m.c) > 1e-3:
                    sheet.keep("rotated or skewed vector paths")
                    continue
                kinds = [p[2] for p in pts]
                if any(k == raw.FPDF_SEGMENT_BEZIERTO for k in kinds):
                    sheet.keep("curved vector paths")
                    continue
                sw = ctypes.c_float(1)
                raw.FPDFPageObj_GetStrokeWidth(obj.raw, ctypes.byref(sw))
                width = max(0.1, sw.value * math.hypot(m.a, m.b))
                fc = _color(raw.FPDFPageObj_GetFillColor, obj) if fill.value else None
                sc = _color(raw.FPDFPageObj_GetStrokeColor, obj) if stroke.value else None
                uniq = []
                for p in pts:
                    q = (round(p[0], 2), round(p[1], 2))
                    if not uniq or uniq[-1] != q:
                        uniq.append(q)
                if len(uniq) > 2 and uniq[0] == uniq[-1]:
                    uniq.pop()
                if len(uniq) == 2 and sc:
                    (x0, y0), (x1, y1) = uniq
                    page["items"].append({"kind": "line", "order": idx, "a": [x0, H - y0], "b": [x1, H - y1], "weight": width, "stroke": hexcolor(*sc[:3])})
                    sheet.count("lines")
                    taken.append(idx)
                elif len(uniq) == 4 and all(abs(uniq[k][0] - uniq[(k + 1) % 4][0]) < 0.02 or abs(uniq[k][1] - uniq[(k + 1) % 4][1]) < 0.02 for k in range(4)):
                    xs = [p[0] for p in uniq]
                    ys = [p[1] for p in uniq]
                    rect = [min(xs), H - max(ys), max(xs), H - min(ys)]
                    white = fc and fc[:3] == (255, 255, 255)
                    if white and not sc and (rect[2] - rect[0]) >= W * 0.97 and (rect[3] - rect[1]) >= H * 0.97:
                        taken.append(idx)
                        continue
                    if fc and rect[2] - rect[0] > 6 and rect[3] - rect[1] > 6:
                        if shot.get("im") is None:
                            shot["im"] = pg.render(scale=0.5).to_pil().convert("RGB")
                        im = shot["im"]
                        bad = 0
                        for fx, fy in ((0.5, 0.5), (0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)):
                            px = int((rect[0] + (rect[2] - rect[0]) * fx) * 0.5)
                            py = int((rect[1] + (rect[3] - rect[1]) * fy) * 0.5)
                            if 0 <= px < im.width and 0 <= py < im.height:
                                got = im.getpixel((px, py))
                                if max(abs(got[k] - fc[k]) for k in range(3)) > 40:
                                    bad += 1
                        if bad >= 3:
                            sheet.keep("gradient, pattern or transparent fills")
                            continue
                    page["items"].append({"kind": "rect", "order": idx, "rect": rect, "fill": hexcolor(*fc[:3]) if fc else None,
                                          "stroke": hexcolor(*sc[:3]) if sc else None, "weight": width})
                    sheet.count("rectangles")
                    taken.append(idx)
                else:
                    sheet.keep("polygons and complex paths")
            elif t == raw.FPDF_PAGEOBJ_FORM:
                sheet.keep("grouped objects (form XObjects)")
            else:
                sheet.keep("shadings and other objects")
        except Exception as exc:
            sheet.keep("objects that could not be read")
            sheet.note(f"page {i + 1}: {type(exc).__name__}: {exc}")
    tp.close()
    pg.close()
    for f in {r["font"] for r in runs}:
        sheet.report["fonts"].setdefault(f, 0)
    for blk in blocks_from_runs(runs, W, H):
        page["items"].append(blk)
    nt = sum(1 for it in page["items"] if it["kind"] == "text")
    if nt:
        sheet.count("text frames", nt)
    return taken


def blocks_from_runs(runs: list[dict], W: float, H: float) -> list[dict]:
    runs = sorted(runs, key=lambda r: (round(r["base"], 1), r["x0"]))
    lines: list[dict] = []
    for r in runs:
        placed = False
        for ln in reversed(lines[-60:]):
            tol = 0.3 * max(r["size"], ln["size"])
            gap = r["x0"] - ln["x1"]
            if abs(ln["base"] - r["base"]) <= tol and -3 <= gap <= 2.2 * max(r["size"], 4):
                if gap > 0.12 * r["size"] and not ln["runs"][-1]["text"].endswith(" ") and not r["text"].startswith(" "):
                    ln["runs"][-1] = {**ln["runs"][-1], "text": ln["runs"][-1]["text"] + " "}
                ln["runs"].append(r)
                ln["x1"] = max(ln["x1"], r["x1"])
                ln["size"] = max(ln["size"], r["size"])
                placed = True
                break
        if not placed:
            lines.append({"x0": r["x0"], "x1": r["x1"], "base": r["base"], "size": r["size"], "runs": [r], "idx": r["idx"]})
    lines.sort(key=lambda l: (l["base"], l["x0"]))
    blocks: list[dict] = []
    for ln in lines:
        best, bgap = None, 1e9
        for bl in blocks[-40:]:
            last = bl["lines"][-1]
            gap = ln["base"] - last["base"]
            sz = max(ln["size"], last["size"])
            if gap < 0.55 * sz or gap > 1.75 * sz:
                continue
            if len(bl["lines"]) > 1 and gap > 1.45 * (bl["pitch"] or gap) + 0.4 * sz and gap > 1.6 * sz:
                continue
            if not 0.8 <= ln["size"] / last["size"] <= 1.25:
                continue
            ov = min(ln["x1"], bl["x1"]) - max(ln["x0"], bl["x0"])
            if ov < 0.3 * min(ln["x1"] - ln["x0"], bl["x1"] - bl["x0"]) and abs(ln["x0"] - bl["x0"]) > 0.6 * sz:
                continue
            if ln["x0"] < bl["x0"] - 3 * sz:
                continue
            if gap < bgap:
                best, bgap = bl, gap
        if best is None:
            blocks.append({"lines": [ln], "x0": ln["x0"], "x1": ln["x1"], "pitch": None})
        else:
            best["lines"].append(ln)
            best["x0"] = min(best["x0"], ln["x0"])
            best["x1"] = max(best["x1"], ln["x1"])
            diffs = [b["base"] - a["base"] for a, b in zip(best["lines"], best["lines"][1:])]
            best["pitch"] = statistics.median(diffs)
    return [_frame_of(b, W) for b in blocks]


def _line_text(ln: dict) -> str:
    return "".join(r["text"] for r in ln["runs"])


def _frame_of(bl: dict, W: float) -> dict:
    lines = bl["lines"]
    size = max(l["size"] for l in lines)
    pitch = bl["pitch"] or size * 1.2
    x0, x1 = bl["x0"], bl["x1"]
    n = len(lines)
    align = "left"
    if n > 1:
        ls = max(l["x0"] for l in lines) - min(l["x0"] for l in lines)
        body = lines[:-1]
        rs = max(l["x1"] for l in lines) - min(l["x1"] for l in body)
        near = sum(1 for l in body if x1 - l["x1"] < 0.6 * size)
        ctrs = [(l["x0"] + l["x1"]) / 2 for l in lines]
        if near >= 0.7 * len(body) and ls < 1.2 * size:
            align = "justify"
        elif ls < 0.8 * size:
            align = "left"
        elif max(l["x1"] for l in lines) - min(l["x1"] for l in lines) < 0.8 * size:
            align = "right"
        elif max(ctrs) - min(ctrs) < 0.8 * size:
            align = "center"
    paras: list[dict] = []
    cur: list[dict] = []
    prev = None
    fixed = all((r["flags"] & 1) or re.search(r"mono|cour|typewriter|cursor", norm(r["font"])) for ln in lines for r in ln["runs"])
    for ln in lines:
        new = False
        if prev is not None:
            gap = ln["base"] - prev["base"]
            if gap > 1.3 * pitch + 0.2 * size:
                new = True
            elif align in ("left", "justify") and ln["x0"] - x0 > 0.9 * size and prev["x0"] - x0 < 0.5 * size:
                new = True
            elif fixed:
                new = True
            elif align == "left" and x1 - prev["x1"] > 0.22 * (x1 - x0) + 2 * size:
                new = True
            elif align in ("left", "justify") and x1 - prev["x1"] > 3 * size and re.search(r"[.!?:”\"]\s*$", _line_text(prev).strip() + " "):
                new = True
        if new and cur:
            paras.append({"lines": cur, "before": max(0.0, (ln["base"] - prev["base"]) - pitch) if prev else 0.0})
            cur = []
        cur.append(ln)
        prev = ln
    if cur:
        paras.append({"lines": cur, "before": 0.0})
    for k in range(1, len(paras)):
        first = paras[k]["lines"][0]
        last = paras[k - 1]["lines"][-1]
        paras[k]["before"] = max(0.0, (first["base"] - last["base"]) - pitch)
    out_paras = []
    for p in paras:
        runs: list[dict] = []
        for j, ln in enumerate(p["lines"]):
            for r in ln["runs"]:
                add = dict(r)
                if j > 0 and runs and runs[-1]["text"].endswith("-") and add["text"][:1].islower() and len(runs[-1]["text"]) > 2 and runs[-1]["text"][-2].isalpha():
                    runs[-1] = {**runs[-1], "text": runs[-1]["text"][:-1]}
                elif j > 0 and runs and not runs[-1]["text"].endswith(" ") and r is ln["runs"][0]:
                    runs[-1] = {**runs[-1], "text": runs[-1]["text"] + " "}
                runs.append(add)
        merged: list[dict] = []
        for r in runs:
            key = (r["font"], r["size"], r["color"], r["bold"], r["italic"])
            if merged and merged[-1]["_k"] == key:
                merged[-1]["text"] += r["text"]
            else:
                merged.append({**r, "_k": key})
        for r in merged:
            r.pop("_k", None)
        first = p["lines"][0]
        out_paras.append({"runs": merged, "before": round(p["before"], 2), "indent": round(max(0.0, first["x0"] - x0), 2) if align == "left" and n > 1 else 0.0})
    slack = max(4.0, 0.025 * (x1 - x0)) if n > 1 else max(8.0, 0.12 * (x1 - x0))
    fx0, fx1 = x0, x1 + slack
    if align == "right":
        fx0, fx1 = x0 - slack, x1
    elif align == "center":
        fx0, fx1 = x0 - slack / 2, x1 + slack / 2
    first_base = lines[0]["base"]
    last_base = lines[-1]["base"]
    top = first_base - 0.93 * lines[0]["size"]
    bottom = last_base + 0.45 * lines[-1]["size"] + 0.4 * pitch
    return {"kind": "text", "order": min(l["idx"] for l in lines), "rect": [fx0, top, fx1, bottom], "paras": out_paras, "align": align,
            "leading": round(pitch, 2) if n > 1 else None, "firstSize": lines[0]["size"], "base0": first_base, "idx": [l["idx"] for l in lines]}


# ---------------------------------------------------------------- writing the document through the core

async def emit(sheet: Sheet, facing: bool = False) -> dict:
    """Create the FormDesign document from a Sheet with core commands (two batches per document plus one for styling)."""
    if not sheet.pages:
        raise CoreError("nothing to import: the file has no pages")
    first = sheet.pages[0]
    await core.call("exec", {"command": "file.new", "params": {"width": round(first["w"], 3), "height": round(first["h"], 3), "pages": len(sheet.pages),
                                                                "facingPages": False, "margins": 0, "columns": 1, "title": sheet.title}}, source=SRC)
    await documents.new_meta()
    uid = await documents.active_uid()

    async def mine() -> None:
        st0 = await core.state()
        if st0.get("active") is None or st0["docs"][st0["active"]]["uid"] != uid:
            for d in st0.get("docs", []):
                if d["uid"] == uid:
                    await core.call("docs.activate", {"index": d["index"]}, source=SRC)
                    return
            raise CoreError("the document being imported was closed")

    cmds: list[dict] = []
    for n, pg in enumerate(sheet.pages, 1):
        if abs(pg["w"] - first["w"]) > 0.05 or abs(pg["h"] - first["h"]) > 0.05:
            cmds.append({"command": "layout.pageSize", "params": {"pages": [n], "width": round(pg["w"], 3), "height": round(pg["h"], 3)}})
    if cmds:
        await mine()
        await core.batch(commands=cmds, atomic=True, source=SRC)
    layers = {}
    if any(p.get("backdrop") for p in sheet.pages):
        await mine()
        r = await core.exec("layer.new", {"name": "PDF backdrop"}, source=SRC)
        lid = r.get("id")
        if lid is not None:
            layers["backdrop"] = lid
            try:
                nl = len((await core.inspect()).get("layers") or [])
                await core.exec("layer.move", {"id": lid, "to": max(0, nl - 1)}, source=SRC)
            except CoreError:
                pass
    await mine()
    st = await core.inspect()
    content_layer = next((l["id"] for l in st["layers"] if l["id"] != layers.get("backdrop")), st.get("activeLayer"))
    if layers:
        await core.exec("layer.activate", {"id": content_layer}, source=SRC)
    cmds = []
    refs: list[tuple[int, dict, int]] = []
    for n, pg in enumerate(sheet.pages):
        items = sorted(pg["items"], key=lambda it: it["order"])
        for it in items:
            k = it["kind"]
            if k == "text":
                x0, y0, x1, y1 = it["rect"]
                if it.get("base0") is not None:
                    r0 = it["paras"][0]["runs"][0]
                    fam0, sty0, _ = await map_font(r0["font"], r0["bold"], r0["italic"], r0.get("flags", 0))
                    new_top = it["base0"] - ascent_of(fam0, sty0) * it["firstSize"]
                    y1 += new_top - y0
                    y0 = new_top
                    it["rect"] = [x0, y0, x1, y1]
                text = "\n".join("".join(r["text"] for r in p["runs"]) for p in it["paras"])
                cmds.append({"command": "frame.create", "params": {"spread": n, "rect": [round(x0, 2), round(y0, 2), round(x1, 2), round(y1, 2)], "content": "text", "text": text}})
                i = len(cmds) - 1
                cmds.append({"command": "object.textFrameOptions", "params": {"inset": 0, "ids": [f"${i}.id"]}})
                refs.append((i, it, n))
            elif k == "image":
                x0, y0, x1, y1 = it["rect"]
                cmds.append({"command": "frame.create", "params": {"spread": n, "rect": [round(x0, 2), round(y0, 2), round(x1, 2), round(y1, 2)], "content": "graphic"}})
                i = len(cmds) - 1
                cmds.append({"command": "file.place", "params": {"path": it["path"], "frame": f"${i}.id"}})
                cmds.append({"command": "object.fit", "params": {"mode": "fitContentToFrame", "ids": [f"${i}.id"]}})
                refs.append((i, it, n))
            elif k == "rect":
                x0, y0, x1, y1 = it["rect"]
                cmds.append({"command": "frame.create", "params": {"spread": n, "rect": [round(x0, 2), round(y0, 2), round(x1, 2), round(y1, 2)], "shape": it.get("shape", "rectangle"), "content": "unassigned"}})
                i = len(cmds) - 1
                ids = [f"${i}.id"]
                if it.get("fill"):
                    cmds.append({"command": "object.color", "params": {"color": it["fill"], "target": "fill", "ids": ids}})
                if it.get("stroke"):
                    cmds.append({"command": "object.stroke", "params": {"swatch": it["stroke"], "weight": round(it["weight"], 3), "ids": ids}})
                refs.append((i, it, n))
            elif k == "line":
                cmds.append({"command": "line.create", "params": {"spread": n, "a": [round(it["a"][0], 2), round(it["a"][1], 2)], "b": [round(it["b"][0], 2), round(it["b"][1], 2)], "weight": round(it["weight"], 3)}})
                i = len(cmds) - 1
                cmds.append({"command": "object.stroke", "params": {"swatch": it["stroke"], "weight": round(it["weight"], 3), "ids": [f"${i}.id"]}})
                refs.append((i, it, n))
            elif k == "polygon":
                x0, y0, x1, y1 = it["rect"]
                cmds.append({"command": "frame.create", "params": {"spread": n, "rect": [round(x0, 2), round(y0, 2), round(x1, 2), round(y1, 2)], "shape": "rectangle", "content": "unassigned"}})
                i = len(cmds) - 1
                refs.append((i, it, n))
    results: list[dict] = []
    if cmds:
        await mine()
        res = await core.batch(commands=cmds, atomic=False, source=SRC)
        results = res.get("results") or []
    frame_of: dict[int, int] = {}
    for i, it, n in refs:
        r = results[i] if i < len(results) else {}
        if isinstance(r, dict) and r.get("id") is not None:
            frame_of[id(it)] = r["id"]
    await mine()
    ins = await core.inspect()
    story_of = {}
    for s in ins.get("stories") or []:
        for f in s.get("frames") or []:
            story_of[f] = s["id"]
    fmt: list[dict] = []
    for i, it, n in refs:
        if it["kind"] != "text":
            continue
        fid = frame_of.get(id(it))
        story = story_of.get(fid)
        if story is None:
            continue
        full = "\n".join("".join(r["text"] for r in p["runs"]) for p in it["paras"])
        pos = 0
        if it.get("leading"):
            fmt.append({"command": "text.select", "params": {"story": story, "anchor": 0, "focus": ubytes(full, len(full))}})
            fmt.append({"command": "type.char", "params": {"attrs": {"leading": it["leading"]}}})
        for p in it["paras"]:
            ptxt = "".join(r["text"] for r in p["runs"])
            a, b = pos, pos + len(ptxt)
            para = {"align": it["align"] if it["align"] != "justify" else "justify"}
            if p["before"] > 0.5:
                para["spaceBefore"] = p["before"]
            if p["indent"] > 0.5:
                para["firstIndent"] = p["indent"]
            fmt.append({"command": "text.select", "params": {"story": story, "anchor": ubytes(full, a), "focus": ubytes(full, max(a, b))}})
            fmt.append({"command": "type.para", "params": {"attrs": para}})
            rp = a
            for r in p["runs"]:
                fam, style, exact = await map_font(r["font"], r["bold"], r["italic"], r.get("flags", 0))
                key = r["font"] or "(unnamed)"
                sheet.report["fonts"][key] = {"family": fam, "style": style, "exact": exact}
                attrs = {"font": fam, "fontStyle": style, "size": r["size"], "fill": r["color"]}
                if r.get("underline"):
                    attrs["underline"] = True
                if r.get("strike"):
                    attrs["strike"] = True
                fmt.append({"command": "text.select", "params": {"story": story, "anchor": ubytes(full, rp), "focus": ubytes(full, rp + len(r["text"]))}})
                fmt.append({"command": "type.char", "params": {"attrs": attrs}})
                rp += len(r["text"])
            pos = b + 1
    if fmt:
        for k in range(0, len(fmt), 1500):
            await mine()
            await core.batch(commands=fmt[k:k + 1500], atomic=False, source=SRC)
    threads = 0
    await mine()
    for it_a, it_b in getattr(sheet, "threads", []):
        fa, fb = frame_of.get(id(it_a)), frame_of.get(id(it_b))
        if fa is not None and fb is not None:
            try:
                await core.exec("frame.thread", {"from": fa, "to": fb}, source=SRC)
                threads += 1
            except CoreError:
                pass
    if threads:
        sheet.report["converted"]["threaded frames"] = threads
    bcmds: list[dict] = []
    for n, pg in enumerate(sheet.pages):
        bd = pg.get("backdrop")
        if not bd or "backdrop" not in layers:
            continue
        bcmds.append({"command": "frame.create", "params": {"spread": n, "rect": [0, 0, round(pg["w"], 2), round(pg["h"], 2)], "content": "graphic"}})
        i = len(bcmds) - 1
        bcmds.append({"command": "file.place", "params": {"path": bd["path"], "frame": f"${i}.id", "page": bd["page"]}})
        bcmds.append({"command": "object.fit", "params": {"mode": "fitContentToFrame", "ids": [f"${i}.id"]}})
        bcmds.append({"command": "object.setLayer", "params": {"layer": layers["backdrop"], "ids": [f"${i}.id"]}})
        bcmds.append({"command": "object.label", "params": {"label": "PDF backdrop", "ids": [f"${i}.id"]}})
        bcmds.append({"command": "object.setFlags", "params": {"locked": True, "ids": [f"${i}.id"]}})
    if bcmds:
        await mine()
        await core.batch(commands=bcmds, atomic=False, source=SRC)
        sheet.report["converted"]["backdrop pages"] = sum(1 for p in sheet.pages if p.get("backdrop"))
    st = await core.state()
    return await documents.overview()


def strip_pdf(src: str, dest: Path, per_page: dict[int, list[int]], strip_types: set[int] | None = None) -> dict[int, int]:
    """Copy of the PDF with the converted objects removed; returns how many objects remain per page."""
    doc = pdfium.PdfDocument(src)
    left: dict[int, int] = {}
    try:
        for i in range(len(doc)):
            pg = doc[i]
            objs = list(pg.get_objects(max_depth=1))
            drop = set(per_page.get(i, []))
            if strip_types is not None:
                drop |= {k for k, o in enumerate(objs) if o.type in strip_types}
            kept = 0
            for k in sorted(drop, reverse=True):
                if k < len(objs):
                    try:
                        pg.remove_obj(objs[k])
                    except Exception:
                        kept += 1
            kept += len(objs) - len(drop)
            pg.gen_content()
            left[i] = kept
            pg.close()
        doc.save(str(dest))
    finally:
        doc.close()
    return left


async def import_pdf(path: str, options: dict | None = None) -> dict:
    options = options or {}
    mode = options.get("backdrop", "auto")
    src = Path(path)
    if not src.is_file():
        raise CoreError(f"no file at {src}")
    work = new_work("pdf")
    sheet = Sheet(src.stem)

    def read() -> None:
        from app.services.pdfread import _lock as pdfium_lock
        with pdfium_lock:
            pdf = pdfium.PdfDocument(str(src))
            taken: dict[int, list[int]] = {}
            try:
                n = len(pdf)
                limit = int(options.get("maxPages", 300))
                if n > limit:
                    sheet.note(f"only the first {limit} of {n} pages were imported")
                for i in range(min(n, limit)):
                    w, h = pdf[i].get_size()
                    page = sheet.page(w, h)
                    taken[i] = read_pdf_page(pdf, i, sheet, work, page, mode)
            finally:
                pdf.close()
            if mode != "never":
                stripped = work / "backdrop.pdf"
                try:
                    left = strip_pdf(str(src), stripped, taken)
                    for i, pg in enumerate(sheet.pages):
                        if left.get(i, 0) > 0:
                            pg["backdrop"] = {"path": str(stripped), "page": i}
                except Exception as exc:
                    sheet.note(f"backdrop could not be built: {type(exc).__name__}: {exc}")

    await asyncio.to_thread(read)
    overview = await emit(sheet)
    uid = await documents.active_uid()
    rep = finish_report(sheet, "pdf", src.name)
    return {"overview": overview, "report": rep}


def finish_report(sheet: Sheet, kind: str, name: str) -> dict:
    pages = len(sheet.pages)
    backdrops = sum(1 for p in sheet.pages if p.get("backdrop"))
    fonts = [{"source": k, **(v if isinstance(v, dict) else {})} for k, v in sheet.report["fonts"].items()]
    return {"source": name, "kind": kind, "pages": pages, "converted": sheet.report["converted"], "keptAsBackdrop": sheet.report["kept"],
            "backdropPages": backdrops, "fonts": fonts, "substituted": [f for f in fonts if f.get("family") and not f.get("exact")], "notes": sheet.report["notes"]}


# ---------------------------------------------------------------- LibreOffice route (.pub, .qxd and .qxp, .pmd and .p65)

async def soffice(path: Path, fmt: str, out: Path) -> Path:
    exe = shutil.which("soffice") or shutil.which("libreoffice")
    if not exe:
        raise CoreError("LibreOffice is not installed, so this format cannot be converted; install LibreOffice or export a PDF from the original application")
    prof = out / "profile"
    cmd = [exe, f"-env:UserInstallation=file://{prof}", "--headless", "--convert-to", fmt, "--outdir", str(out), str(path)]
    proc = await asyncio.create_subprocess_exec(*cmd, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.STDOUT)
    try:
        so, _ = await asyncio.wait_for(proc.communicate(), 180)
    except asyncio.TimeoutError:
        proc.kill()
        raise CoreError("LibreOffice took more than three minutes to read this file")
    ext = fmt.split(":")[0]
    target = out / (path.stem + "." + ext)
    if not target.exists():
        raise CoreError(f"LibreOffice could not convert {path.name}: {so.decode(errors='ignore').strip()[-200:] or 'unreadable file'}")
    return target


NS = {"office": "urn:oasis:names:tc:opendocument:xmlns:office:1.0", "draw": "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0",
      "style": "urn:oasis:names:tc:opendocument:xmlns:style:1.0", "text": "urn:oasis:names:tc:opendocument:xmlns:text:1.0",
      "svg": "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0", "fo": "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0",
      "xlink": "http://www.w3.org/1999/xlink"}


def Q(p: str) -> str:
    a, b = p.split(":")
    return "{" + NS[a] + "}" + b


def length(v: str | None) -> float:
    if not v:
        return 0.0
    m = re.match(r"\s*(-?[\d.]+)\s*(cm|mm|in|pt|px|pc)?", v)
    if not m:
        return 0.0
    n = float(m.group(1))
    return n * {"cm": CM, "mm": CM / 10, "in": 72.0, "pt": 1.0, "px": 0.75, "pc": 12.0, None: CM}[m.group(2)]


class OdgStyles:
    def __init__(self, roots: list) -> None:
        self.by: dict[tuple[str, str], ET.Element] = {}
        self.faces: dict[str, str] = {}
        for root in roots:
            for e in root.iter(Q("style:style")):
                self.by[(e.get(Q("style:family")) or "", e.get(Q("style:name")) or "")] = e
            for e in root.iter(Q("style:font-face")):
                self.faces[e.get(Q("style:name")) or ""] = (e.get(Q("svg:font-family")) or "").strip("'\" ")

    def props(self, family: str, name: str | None, kind: str) -> dict:
        out: dict = {}
        chain = []
        seen = set()
        while name and (family, name) not in seen:
            seen.add((family, name))
            e = self.by.get((family, name))
            if e is None:
                break
            chain.append(e)
            name = e.get(Q("style:parent-style-name"))
        for e in reversed(chain):
            for p in e.findall(Q(kind)):
                out.update({k: v for k, v in p.attrib.items()})
        return out

    def font(self, props: dict) -> str:
        fam = props.get(Q("fo:font-family")) or props.get(Q("svg:font-family"))
        if not fam:
            n = props.get(Q("style:font-name"))
            fam = self.faces.get(n, n) if n else ""
        return (fam or "").strip("'\" ")


def run_props(st: OdgStyles, name: str | None, para_name: str | None) -> dict:
    base = st.props("paragraph", para_name, "style:text-properties")
    base.update(st.props("text", name, "style:text-properties"))
    size = base.get(Q("fo:font-size"), "12pt")
    if size.endswith("%"):
        size = "12pt"
    color = base.get(Q("fo:color"), "#000000")
    return {"font": st.font(base), "size": round(length(size) or 12.0, 2), "color": color if re.fullmatch(r"#[0-9a-fA-F]{6}", color) else "#000000",
            "bold": base.get(Q("fo:font-weight")) in ("bold", "700", "800", "900"), "italic": base.get(Q("fo:font-style")) in ("italic", "oblique"),
            "underline": (base.get(Q("style:text-underline-style")) or "none") != "none", "strike": (base.get(Q("style:text-line-through-style")) or "none") != "none"}


def odg_paragraphs(box: ET.Element, st: OdgStyles) -> tuple[list[dict], str]:
    paras = []
    align = "left"
    for p in box.iter(Q("text:p")):
        pname = p.get(Q("text:style-name"))
        pp = st.props("paragraph", pname, "style:paragraph-properties")
        a = pp.get(Q("fo:text-align"), "start")
        align = {"start": "left", "left": "left", "end": "right", "right": "right", "center": "center", "justify": "justify"}.get(a, align) if not paras else align
        runs: list[dict] = []

        def walk(e: ET.Element, sname: str | None) -> None:
            if e.text:
                runs.append({"text": clean_text(e.text), **run_props(st, sname, pname)})
            for c in e:
                if c.tag == Q("text:span"):
                    walk(c, c.get(Q("text:style-name")) or sname)
                elif c.tag == Q("text:s"):
                    runs.append({"text": " " * int(c.get(Q("text:c"), "1")), **run_props(st, sname, pname)})
                elif c.tag == Q("text:tab"):
                    runs.append({"text": "\t", **run_props(st, sname, pname)})
                elif c.tag == Q("text:line-break"):
                    runs.append({"text": " ", **run_props(st, sname, pname)})
                else:
                    walk(c, sname)
                if c.tail:
                    runs.append({"text": clean_text(c.tail), **run_props(st, sname, pname)})

        walk(p, None)
        runs = [r for r in runs if r["text"] != ""]
        if not runs:
            runs = [{"text": "", **run_props(st, None, pname)}]
        merged: list[dict] = []
        for r in runs:
            key = (r["font"], r["size"], r["color"], r["bold"], r["italic"], r["underline"], r["strike"])
            if merged and merged[-1]["_k"] == key:
                merged[-1]["text"] += r["text"]
            else:
                merged.append({**r, "_k": key})
        for r in merged:
            r.pop("_k", None)
        paras.append({"runs": merged, "before": length(pp.get(Q("fo:margin-top"))), "indent": length(pp.get(Q("fo:text-indent")))})
    return [p for p in paras], align


async def import_odg(odg: Path, pdf: Path | None, name: str, label: str, options: dict | None = None) -> dict:
    options = options or {}
    sheet = Sheet(Path(name).stem)
    work = new_work("odg")
    z = zipfile.ZipFile(odg)
    content = ET.fromstring(z.read("content.xml"))
    styles_root = ET.fromstring(z.read("styles.xml"))
    st = OdgStyles([content, styles_root])
    layouts: dict[str, tuple[float, float]] = {}
    for pl in styles_root.iter(Q("style:page-layout")):
        pr = pl.find(Q("style:page-layout-properties"))
        if pr is not None:
            layouts[pl.get(Q("style:name")) or ""] = (length(pr.get(Q("fo:page-width"))), length(pr.get(Q("fo:page-height"))))
    masters = {m.get(Q("style:name")): layouts.get(m.get(Q("style:page-layout-name")) or "", (595.0, 842.0)) for m in styles_root.iter(Q("style:master-page"))}
    body = content.find(".//" + Q("office:drawing"))
    pages = list(body.findall(Q("draw:page"))) if body is not None else []
    layer_names: list[str] = []
    for lay in styles_root.iter(Q("draw:layer")):
        layer_names.append(lay.get(Q("draw:name")) or "")
    by_name: dict[str, dict] = {}
    chains: list[tuple[str, str]] = []
    nofill = 0
    for pi, dp in enumerate(pages):
        w, h = masters.get(dp.get(Q("draw:master-page-name")), (595.0, 842.0))
        page = sheet.page(w or 595.0, h or 842.0)
        order = 0

        def visit(e: ET.Element) -> None:
            nonlocal order, nofill
            for c in e:
                tag = c.tag
                order += 1
                if tag == Q("draw:g"):
                    visit(c)
                    continue
                x, y, ww, hh = (length(c.get(Q("svg:x"))), length(c.get(Q("svg:y"))), length(c.get(Q("svg:width"))), length(c.get(Q("svg:height"))))
                gprops = st.props("graphic", c.get(Q("draw:style-name")), "style:graphic-properties")
                fill = gprops.get(Q("draw:fill-color")) if gprops.get(Q("draw:fill")) == "solid" else None
                stroke = gprops.get(Q("svg:stroke-color")) if gprops.get(Q("draw:stroke")) not in (None, "none") else None
                sw = length(gprops.get(Q("svg:stroke-width"))) or 0.75
                if tag == Q("draw:frame"):
                    tb = c.find(Q("draw:text-box"))
                    im = c.find(Q("draw:image"))
                    if tb is not None:
                        paras, align = odg_paragraphs(tb, st)
                        if not any(r["text"].strip() for p in paras for r in p["runs"]):
                            return_none = True
                        else:
                            size = max(r["size"] for p in paras for r in p["runs"])
                            it = {"kind": "text", "order": order, "rect": [x, y, x + ww, y + max(hh, size * 1.3)], "paras": paras, "align": align, "leading": None,
                                  "firstSize": size, "idx": []}
                            if fill:
                                page["items"].append({"kind": "rect", "order": order - 0.5, "rect": [x, y, x + ww, y + hh], "fill": fill, "stroke": stroke, "weight": sw})
                            page["items"].append(it)
                            nm = c.get(Q("draw:name"))
                            if nm:
                                by_name[nm] = it
                            nx = tb.get(Q("draw:chain-next-name"))
                            if nm and nx:
                                chains.append((nm, nx))
                            sheet.count("text frames")
                            for p in paras:
                                for r in p["runs"]:
                                    sheet.report["fonts"].setdefault(r["font"] or "(default)", 0)
                    elif im is not None:
                        href = im.get(Q("xlink:href")) or ""
                        if href in z.namelist():
                            out = work / f"img{pi}-{order}{Path(href).suffix or '.png'}"
                            out.write_bytes(z.read(href))
                            page["items"].append({"kind": "image", "order": order, "rect": [x, y, x + ww, y + hh], "path": str(out), "px": (0, 0)})
                            sheet.count("pictures placed")
                        else:
                            sheet.keep("linked pictures that were not embedded")
                elif tag in (Q("draw:rect"), Q("draw:custom-shape")):
                    if fill or stroke:
                        page["items"].append({"kind": "rect", "order": order, "rect": [x, y, x + ww, y + hh], "fill": fill, "stroke": stroke, "weight": sw})
                        sheet.count("rectangles")
                    tb = c.find(Q("draw:text-box"))
                    if c.find(Q("text:p")) is not None and any((p.text or "").strip() or len(p) for p in c.findall(Q("text:p"))):
                        paras, align = odg_paragraphs(c, st)
                        size = max(r["size"] for p in paras for r in p["runs"])
                        page["items"].append({"kind": "text", "order": order + 0.1, "rect": [x, y, x + ww, y + max(hh, size * 1.3)], "paras": paras, "align": "center", "leading": None, "firstSize": size, "idx": []})
                        sheet.count("text frames")
                elif tag == Q("draw:line"):
                    x1, y1, x2, y2 = (length(c.get(Q("svg:" + k))) for k in ("x1", "y1", "x2", "y2"))
                    page["items"].append({"kind": "line", "order": order, "a": [x1, y1], "b": [x2, y2], "weight": sw, "stroke": stroke or "#000000"})
                    sheet.count("lines")
                elif tag in (Q("draw:ellipse"), Q("draw:circle")):
                    if fill or stroke:
                        page["items"].append({"kind": "rect", "order": order, "rect": [x, y, x + ww, y + hh], "fill": fill, "stroke": stroke, "weight": sw})
                        sheet.count("shapes (drawn as rectangles)")
                elif tag in (Q("draw:polygon"), Q("draw:polyline"), Q("draw:path"), Q("draw:regular-polygon")):
                    sheet.keep("polygons and curves")
        if dp is not None:
            visit(dp)
    sheet.threads = [(by_name[a], by_name[b]) for a, b in chains if a in by_name and b in by_name]
    custom_layers = [l for l in layer_names if l and l not in ("layout", "background", "backgroundobjects", "controls", "measurelines")]
    if custom_layers:
        sheet.note("layers in the source: " + ", ".join(custom_layers) + " (their objects were imported into one layer)")
    if pdf is not None and options.get("backdrop", "auto") != "never" and sheet.report["kept"]:
        try:
            dest = work / "backdrop.pdf"
            strip = {raw.FPDF_PAGEOBJ_TEXT, raw.FPDF_PAGEOBJ_IMAGE}
            left = strip_pdf(str(pdf), dest, {}, strip)
            for i, pg in enumerate(sheet.pages):
                if left.get(i, 0) > 0:
                    pg["backdrop"] = {"path": str(dest), "page": i}
        except Exception as exc:
            sheet.note(f"backdrop could not be built: {type(exc).__name__}: {exc}")
    overview = await emit(sheet)
    rep = finish_report(sheet, label, name)
    return {"overview": overview, "report": rep}


async def import_soffice(path: str, options: dict | None = None) -> dict:
    src = Path(path)
    ext = src.suffix.lower()
    label = "qxp" if ext in QXP_EXTS else "pm" if ext in PM_EXTS else "pub"
    if label == "qxp" and not qxp_signature(src):
        raise CoreError(f"{src.name} is not a .qxd or .qxp document: its header lacks the MMXPR or IIXPR signature (the file may be damaged or have the wrong extension)")
    work = new_work("lo")
    safe = work / ("source" + ext)
    shutil.copyfile(src, safe)
    try:
        odg = await soffice(safe, "odg:draw8", work)
    except CoreError as exc:
        if label == "qxp":
            raise CoreError(f"{exc}. {QXP_VERSIONS}") from exc
        raise
    pdf = None
    try:
        pdf = await soffice(safe, "pdf", work)
    except CoreError:
        pdf = None
    try:
        result = await import_odg(odg, pdf, src.name, label, options)
    except CoreError as exc:
        # When libqxp declines a file, LibreOffice tries its other filters and can hand back an empty drawing rather
        # than an error (random bytes named .qxp did), so an empty result is reported as the reader's limit.
        if label == "qxp" and "no pages" in str(exc):
            raise CoreError(f"{src.name} is a .qxd or .qxp document LibreOffice could not read. {QXP_VERSIONS}") from exc
        raise
    result["report"]["via"] = "LibreOffice " + ("libmspub" if label == "pub" else "libqxp" if label == "qxp" else "libpagemaker")
    return result


def indd_message(name: str) -> dict:
    return {"kind": "indd", "name": name, "message": "The .indd file format is closed and has no public reader, so FormDesign cannot open it directly. In the application that made the file, save or export it as IDML (.idml), and open the .idml file here; text, frames, styles and swatches carry over.",
            "steps": ["Open the file in the application that made it.", "Save or export it as IDML (.idml), using the Save As or Export command.", "Open the saved .idml file in FormDesign with File > Open."]}


def dwg_message(name: str) -> dict:
    return {"kind": "dwg", "name": name, "message": f"{name} is a DWG drawing. DWG is a closed format and has no permissively licensed reader (the available readers need a proprietary converter or carry the GPL), so FormDesign cannot open it directly. Please export a DXF (.dxf) from the application that made the drawing and open that file instead."}


async def open_layout(path: str, options: dict | None = None) -> dict:
    p = Path(path).expanduser()
    ext = p.suffix.lower()
    if ext in INDD_EXTS:
        raise CoreError(indd_message(p.name)["message"])
    if ext in DWG_EXTS:
        raise CoreError(dwg_message(p.name)["message"])
    if ext in DXF_EXTS:
        from app.services import dxfimport
        return await dxfimport.import_dxf(str(p), options)
    if ext == ".pdf":
        return await import_pdf(str(p), options)
    if ext in SOFFICE_EXTS:
        try:
            return await import_soffice(str(p), options)
        except CoreError as exc:
            if "could not convert" in str(exc) or "unreadable" in str(exc):
                raise CoreError(f"{p.name} could not be read: {exc}") from exc
            raise
    raise CoreError(f"{p.name} is not a layout format FormDesign converts (.pdf, .pub, .qxd, .qxp, .pmd, .p65, .dxf)")


def probe(path: str) -> dict:
    p = Path(path).expanduser()
    ext = p.suffix.lower()
    if ext in INDD_EXTS:
        return indd_message(p.name)
    if ext in DWG_EXTS:
        return dwg_message(p.name)
    if ext in DXF_EXTS:
        return {"kind": "dxf", "name": p.name, "choices": ["layout"]}
    if ext == ".pdf":
        try:
            d = pdfium.PdfDocument(str(p))
            n = len(d)
            d.close()
        except Exception:
            n = None
        return {"kind": "pdf", "name": p.name, "pages": n, "choices": ["layout", "place"]}
    if ext in SOFFICE_EXTS:
        return {"kind": "qxp" if ext in QXP_EXTS else "pm" if ext in PM_EXTS else "pub", "name": p.name, "choices": ["layout"]}
    return {"kind": "native", "name": p.name}
