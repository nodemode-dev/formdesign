"""Finishes an exported PDF with what the core's PDF writer (krilla) cannot write: page transitions, AcroForm fields,
button actions beyond links, media (Screen annotations with Rendition actions) and the PDF/X identity (output intent,
GTS_PDFXVersion, Trapped, XMP, boxes). Everything here is pypdf over the finished file."""

import asyncio
import base64
import hashlib
import io
import shutil
import subprocess
import tempfile
from datetime import datetime, timezone
from pathlib import Path

from pypdf import PdfReader, PdfWriter
from pypdf.generic import (ArrayObject, BooleanObject, ByteStringObject, DecodedStreamObject, DictionaryObject, FloatObject,
                           IndirectObject, NameObject, NumberObject, TextStringObject)

from app.engine.core import CoreError, core

ICC = Path(__file__).resolve().parent.parent / "assets" / "CGATS001Compat-v2-micro.icc"

PDFX = {
    "x1a": {"id": "PDF/X-1a:2001", "short": "X-1a:2001", "header": "%PDF-1.4", "conformance": "PDF/X-1a:2001"},
    "x3": {"id": "PDF/X-3:2002", "short": "X-3:2002", "header": "%PDF-1.4", "conformance": "PDF/X-3:2002"},
    "x4": {"id": "PDF/X-4", "short": "X-4", "header": "%PDF-1.6", "conformance": "PDF/X-4"},
}

TRANS = {
    "fade": ("Fade", None), "dissolve": ("Dissolve", None), "wipe": ("Wipe", "di4"), "push": ("Push", "di2"), "cover": ("Cover", "di2"),
    "uncover": ("Uncover", "di2"), "split": ("Split", "dm"), "blinds": ("Blinds", "dm"), "box": ("Box", "m"), "comb": ("Blinds", "vert"),
    "zoomIn": ("Box", "m"), "zoomOut": ("Box", "mi"), "pageTurn": ("Push", "di2"),
}
LAYOUTS = {"single": "SinglePage", "continuous": "OneColumn", "twoUp": "TwoPageLeft", "twoUpCover": "TwoPageRight", "twoUpContinuous": "TwoColumnLeft"}
TRUE = {"1", "true", "yes", "on", "checked", "x"}


def _n(x: float) -> FloatObject:
    return FloatObject(f"{x:.3f}")


def _rect(r: list[float]) -> ArrayObject:
    return ArrayObject([_n(v) for v in r])


def _esc(s: str) -> bytes:
    return s.encode("latin-1", "replace").replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)")


def _truthy(v) -> bool:
    return v is True or str(v).strip().lower() in TRUE


class Builder:
    def __init__(self, data: bytes, info: dict | None, media_dir: Path | None, options: dict) -> None:
        self.w = PdfWriter(clone_from=io.BytesIO(data))
        self.info = info
        self.media_dir = media_dir
        self.options = options
        self.warnings: list[str] = []
        self.fields = ArrayObject()
        self.names: set[str] = set()
        self.radios: dict[str, DictionaryObject] = {}
        self.fonts: DictionaryObject | None = None
        self.counts = {"fields": 0, "buttons": 0, "media": 0, "transitions": 0}

    # ---- helpers
    def add(self, obj) -> IndirectObject:
        return self.w._add_object(obj)

    def stream(self, data: bytes, d: dict | None = None) -> IndirectObject:
        s = DecodedStreamObject()
        s.set_data(data)
        for k, v in (d or {}).items():
            s[NameObject(k)] = v
        return self.add(s)

    def box(self, page) -> tuple[float, float]:
        t = page.trimbox
        return float(t.left), float(t.top)

    def pdf_rect(self, page, r: list[float]) -> list[float]:
        left, top = self.box(page)
        return [left + r[0], top - r[3], left + r[2], top - r[1]]

    def annots(self, page) -> ArrayObject:
        a = page.get("/Annots")
        a = a.get_object() if a is not None else None
        if not isinstance(a, ArrayObject):
            a = ArrayObject()
            page[NameObject("/Annots")] = a
        return a

    def font_resources(self) -> DictionaryObject:
        if self.fonts is None:
            helv = self.add(DictionaryObject({NameObject("/Type"): NameObject("/Font"), NameObject("/Subtype"): NameObject("/Type1"), NameObject("/BaseFont"): NameObject("/Helvetica"), NameObject("/Encoding"): NameObject("/WinAnsiEncoding")}))
            zadb = self.add(DictionaryObject({NameObject("/Type"): NameObject("/Font"), NameObject("/Subtype"): NameObject("/Type1"), NameObject("/BaseFont"): NameObject("/ZapfDingbats")}))
            self.fonts = DictionaryObject({NameObject("/Helv"): helv, NameObject("/ZaDb"): zadb})
        return self.fonts

    def form(self, bbox: list[float], content: bytes, fonts: bool = True) -> IndirectObject:
        res = DictionaryObject()
        if fonts:
            res[NameObject("/Font")] = self.font_resources()
        return self.stream(content, {"/Type": NameObject("/XObject"), "/Subtype": NameObject("/Form"), "/BBox": _rect(bbox), "/Resources": res})

    # ---- transitions
    def transitions(self) -> None:
        for pg, entry in zip(self.w.pages, (self.info or {}).get("pages", [])):
            t = entry.get("transition")
            if not isinstance(t, dict) or t.get("kind") not in TRANS:
                continue
            style, mode = TRANS[t["kind"]]
            horizontal = t.get("horizontal", True) is not False
            d = DictionaryObject({NameObject("/Type"): NameObject("/Trans"), NameObject("/S"): NameObject("/" + style), NameObject("/D"): _n(max(0.01, float(t.get("duration") or 1.0)))})
            if mode in ("dm", "vert"):
                d[NameObject("/Dm")] = NameObject("/H" if horizontal and mode == "dm" else "/V")
                d[NameObject("/M")] = NameObject("/I")
            elif mode in ("m", "mi"):
                d[NameObject("/M")] = NameObject("/I" if mode == "mi" or t["kind"] == "zoomOut" else "/O")
            elif mode == "di4":
                d[NameObject("/Di")] = NumberObject(0 if horizontal else 270)
            elif mode == "di2":
                d[NameObject("/Di")] = NumberObject(0 if horizontal else 270)
            pg[NameObject("/Trans")] = d
            self.counts["transitions"] += 1

    # ---- form fields
    def unique(self, name: str) -> str:
        base = (name or "field").replace(".", "_") or "field"
        n, k = base, 2
        while n in self.names:
            n, k = f"{base}_{k}", k + 1
        self.names.add(n)
        return n

    def text_ap(self, w: float, h: float, lines: list[str], size: float, multiline: bool) -> IndirectObject:
        out = b"/Tx BMC q BT "
        pad = 2.0
        if multiline or len(lines) > 1:
            lead = size * 1.15
            y = h - pad - size
            out += f"/Helv {size:.2f} Tf 0 g ".encode()
            first = True
            for ln in lines:
                if y < -size:
                    break
                out +=(f"{pad:.2f} {y:.2f} Td " if first else f"0 {-lead:.2f} Td ").encode() + b"(" + _esc(ln) + b") Tj "
                first = False
                y -= lead
        else:
            y = (h - size) / 2 + size * 0.22
            out += f"/Helv {size:.2f} Tf 0 g {pad:.2f} {y:.2f} Td ".encode() + b"(" + _esc(lines[0] if lines else "") + b") Tj "
        out += b"ET Q EMC"
        return self.form([0, 0, w, h], out)

    def wrap(self, text: str, width: float, size: float) -> list[str]:
        per = max(1, int((width - 4) / (size * 0.5)))
        out: list[str] = []
        for para in str(text).split("\n"):
            words, cur = para.split(" "), ""
            for wd in words:
                if cur and len(cur) + 1 + len(wd) > per:
                    out.append(cur)
                    cur = wd
                else:
                    cur = f"{cur} {wd}" if cur else wd
            out.append(cur)
        return out

    def fields_for(self, page, ref: IndirectObject, items: list[dict]) -> None:
        for it in items:
            if it["type"] == "form":
                self.field(page, ref, it)
            elif it["type"] == "button":
                self.button(page, ref, it)

    def widget(self, page_ref, rect: list[float]) -> DictionaryObject:
        return DictionaryObject({NameObject("/Type"): NameObject("/Annot"), NameObject("/Subtype"): NameObject("/Widget"), NameObject("/Rect"): _rect(rect), NameObject("/F"): NumberObject(4), NameObject("/P"): page_ref})

    def field(self, page, page_ref, it: dict) -> None:
        f = it["form"]
        kind = f.get("kind", "textField")
        rect = self.pdf_rect(page, it["rect"])
        w, h = rect[2] - rect[0], rect[3] - rect[1]
        if w <= 0 or h <= 0:
            return
        name = str(f.get("name") or f"field{it['id']}")
        value = f.get("value", "")
        raw_opts = f.get("options") or []
        if isinstance(raw_opts, str):
            raw_opts = [o.strip() for o in raw_opts.replace("\n", ",").split(",") if o.strip()]
        opts = [str(o.get("label") or o.get("value")) if isinstance(o, dict) else str(o) for o in raw_opts]
        ff = 2 if f.get("required") else 0
        size = float(f.get("fontSize") or 0)
        draw = size if size > 0 else max(6.0, min(12.0, h - 4))
        wd = self.widget(page_ref, rect)
        wd[NameObject("/DA")] = TextStringObject(f"/Helv {(size if size > 0 else draw):g} Tf 0 g")
        if kind == "textField":
            multi = bool(f.get("multiline"))
            wd[NameObject("/FT")] = NameObject("/Tx")
            wd[NameObject("/T")] = TextStringObject(self.unique(name))
            wd[NameObject("/Ff")] = NumberObject(ff | (4096 if multi else 0))
            if str(value) != "":
                wd[NameObject("/V")] = TextStringObject(str(value))
                wd[NameObject("/DV")] = TextStringObject(str(value))
            lines = self.wrap(str(value), w, draw) if multi else [str(value)]
            wd[NameObject("/AP")] = DictionaryObject({NameObject("/N"): self.text_ap(w, h, lines, draw, multi)})
        elif kind in ("checkBox", "radioButton"):
            on = "Yes" if kind == "checkBox" else (opts[0] if opts else f"Choice{it['id']}")
            on = "".join(ch for ch in on if ch.isalnum()) or "Yes"
            checked = _truthy(value) or (kind == "radioButton" and str(value) == on)
            # Drawn as paths so the mark shows in viewers that lack ZapfDingbats.
            if kind == "checkBox":
                mark = b"q 0 G 1 J 1 j %.2f w %.2f %.2f m %.2f %.2f l %.2f %.2f l S Q" % (max(1.0, min(w, h) * 0.12), w * 0.2, h * 0.5, w * 0.42, h * 0.22, w * 0.8, h * 0.8)
            else:
                cx, cy, rr, k = w / 2, h / 2, min(w, h) * 0.28, 0.5523 * min(w, h) * 0.28
                mark = b"q 0 g %.2f %.2f m %.2f %.2f %.2f %.2f %.2f %.2f c %.2f %.2f %.2f %.2f %.2f %.2f c %.2f %.2f %.2f %.2f %.2f %.2f c %.2f %.2f %.2f %.2f %.2f %.2f c f Q" % (
                    cx + rr, cy, cx + rr, cy + k, cx + k, cy + rr, cx, cy + rr, cx - k, cy + rr, cx - rr, cy + k, cx - rr, cy,
                    cx - rr, cy - k, cx - k, cy - rr, cx, cy - rr, cx + k, cy - rr, cx + rr, cy - k, cx + rr, cy)
            ap_on, ap_off = self.form([0, 0, w, h], mark, fonts=False), self.form([0, 0, w, h], b"q Q", fonts=False)
            wd[NameObject("/FT")] = NameObject("/Btn")
            wd[NameObject("/MK")] = DictionaryObject({NameObject("/CA"): TextStringObject("4" if kind == "checkBox" else "l")})
            wd[NameObject("/DA")] = TextStringObject(f"/ZaDb {min(w, h) * 0.8:.1f} Tf 0 g")
            wd[NameObject("/AP")] = DictionaryObject({NameObject("/N"): DictionaryObject({NameObject("/" + on): ap_on, NameObject("/Off"): ap_off})})
            state = NameObject("/" + on) if checked else NameObject("/Off")
            if kind == "checkBox":
                wd[NameObject("/T")] = TextStringObject(self.unique(name))
                wd[NameObject("/Ff")] = NumberObject(ff)
                wd[NameObject("/V")] = state
                wd[NameObject("/AS")] = state
            else:
                key = name.replace(".", "_")
                parent = self.radios.get(key)
                if parent is None:
                    self.names.add(key)
                    parent = DictionaryObject({NameObject("/FT"): NameObject("/Btn"), NameObject("/T"): TextStringObject(key), NameObject("/Ff"): NumberObject(ff | 32768 | 16384), NameObject("/Kids"): ArrayObject(), NameObject("/V"): NameObject("/Off")})
                    self.radios[key] = parent
                    self.fields.append(self.add(parent))
                    self.counts["fields"] += 1
                wd[NameObject("/AS")] = state
                if checked and parent["/V"] == "/Off":
                    parent[NameObject("/V")] = NameObject("/" + on)
                wd[NameObject("/Parent")] = self.fields[[i for i, r in enumerate(self.fields) if r.get_object() is parent][0]]
                wr = self.add(wd)
                parent["/Kids"].append(wr)
                self.annots(page).append(wr)
                return
        elif kind in ("comboBox", "listBox"):
            combo = kind == "comboBox"
            wd[NameObject("/FT")] = NameObject("/Ch")
            wd[NameObject("/T")] = TextStringObject(self.unique(name))
            wd[NameObject("/Ff")] = NumberObject(ff | (131072 if combo else 0))
            wd[NameObject("/Opt")] = ArrayObject([TextStringObject(o) for o in opts])
            sel = str(value)
            if sel != "":
                wd[NameObject("/V")] = TextStringObject(sel)
                wd[NameObject("/DV")] = TextStringObject(sel)
                if not combo and sel in opts:
                    wd[NameObject("/I")] = ArrayObject([NumberObject(opts.index(sel))])
            lines = [sel] if combo else opts
            wd[NameObject("/AP")] = DictionaryObject({NameObject("/N"): self.text_ap(w, h, lines, draw, not combo)})
        elif kind == "signature":
            wd[NameObject("/FT")] = NameObject("/Sig")
            wd[NameObject("/T")] = TextStringObject(self.unique(name))
            wd.pop(NameObject("/DA"), None)
            if f.get("required"):
                wd[NameObject("/Ff")] = NumberObject(2)
        else:
            self.warnings.append(f"form field {name}: unknown kind {kind}, left out")
            return
        ref = self.add(wd)
        self.fields.append(ref)
        self.annots(page).append(ref)
        self.counts["fields"] += 1

    # ---- button actions that are not links (the core already writes links for the page and web ones)
    def button(self, page, page_ref, it: dict) -> None:
        b = it["button"]
        a = b.get("action") or {}
        kind = a.get("kind")
        act: DictionaryObject | None = None
        if kind == "submitForm" and a.get("url"):
            act = DictionaryObject({NameObject("/S"): NameObject("/SubmitForm"), NameObject("/F"): DictionaryObject({NameObject("/FS"): NameObject("/URL"), NameObject("/F"): TextStringObject(str(a["url"]))}), NameObject("/Flags"): NumberObject(4)})
        elif kind == "resetForm":
            act = DictionaryObject({NameObject("/S"): NameObject("/ResetForm")})
        elif kind == "print":
            act = DictionaryObject({NameObject("/S"): NameObject("/Named"), NameObject("/N"): NameObject("/Print")})
        elif kind in ("showHide", "show", "hide") and a.get("target"):
            hide = kind == "hide" or (kind == "showHide" and a.get("show") is False)
            act = DictionaryObject({NameObject("/S"): NameObject("/Hide"), NameObject("/T"): TextStringObject(str(a["target"]).replace(".", "_")), NameObject("/H"): BooleanObject(hide)})
        if act is None:
            return
        rect = self.pdf_rect(page, it["rect"])
        wd = self.widget(page_ref, rect)
        wd[NameObject("/FT")] = NameObject("/Btn")
        wd[NameObject("/T")] = TextStringObject(self.unique(str(b.get("name") or f"Button{it['id']}")))
        wd[NameObject("/Ff")] = NumberObject(65536)
        wd[NameObject("/A")] = act
        wd[NameObject("/MK")] = DictionaryObject({NameObject("/CA"): TextStringObject("")})
        ref = self.add(wd)
        self.fields.append(ref)
        self.annots(page).append(ref)
        self.counts["buttons"] += 1

    def acroform(self) -> None:
        if not self.fields:
            return
        form = DictionaryObject({NameObject("/Fields"): self.fields, NameObject("/DA"): TextStringObject("/Helv 0 Tf 0 g"), NameObject("/DR"): DictionaryObject({NameObject("/Font"): self.font_resources()})})
        self.w._root_object[NameObject("/AcroForm")] = form

    # ---- media
    def poster(self, path: Path, rect: list[float]) -> IndirectObject:
        w, h = rect[2] - rect[0], rect[3] - rect[1]
        content = b"q 0.12 g 0 0 %.2f %.2f re f Q " % (w, h)
        res = DictionaryObject()
        jpg = None
        if path.suffix.lower() in (".mp4", ".mov", ".m4v", ".webm", ".avi", ".mkv") and shutil.which("ffmpeg"):
            out = path.with_suffix(".poster.jpg")
            try:
                subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", str(path), "-frames:v", "1", "-vf", "scale=720:-2", str(out)], check=True, timeout=30, capture_output=True)
                jpg = out.read_bytes()
            except (subprocess.SubprocessError, OSError):
                jpg = None
        if jpg:
            from PIL import Image
            im = Image.open(io.BytesIO(jpg)).convert("RGB")
            buf = io.BytesIO()
            im.save(buf, "JPEG", quality=85)
            img = self.stream(buf.getvalue(), {"/Type": NameObject("/XObject"), "/Subtype": NameObject("/Image"), "/Width": NumberObject(im.width), "/Height": NumberObject(im.height), "/ColorSpace": NameObject("/DeviceRGB"), "/BitsPerComponent": NumberObject(8), "/Filter": NameObject("/DCTDecode")})
            s = min(w / im.width, h / im.height)
            iw, ih = im.width * s, im.height * s
            content += b"q %.3f 0 0 %.3f %.3f %.3f cm /Poster Do Q " % (iw, ih, (w - iw) / 2, (h - ih) / 2)
            res[NameObject("/XObject")] = DictionaryObject({NameObject("/Poster"): img})
        else:
            c = min(w, h) * 0.18
            cx, cy = w / 2, h / 2
            content += b"q 1 g %.2f %.2f m %.2f %.2f l %.2f %.2f l f Q " % (cx - c * 0.7, cy - c, cx - c * 0.7, cy + c, cx + c, cy)
        return self.stream(content, {"/Type": NameObject("/XObject"), "/Subtype": NameObject("/Form"), "/BBox": _rect([0, 0, w, h]), "/Resources": res})

    def media(self, page, page_ref, it: dict, page_actions: list) -> None:
        if self.media_dir is None:
            return
        src = self.media_dir / it["file"]
        if not src.is_file():
            return
        o = it.get("options") or {}
        rect = self.pdf_rect(page, it["rect"])
        fname = str(it.get("assetName") or src.name)
        name = Path(fname).stem or "Media"
        mime = str(it.get("mime") or "video/mp4")
        ef = DecodedStreamObject()
        ef.set_data(src.read_bytes())
        ef[NameObject("/Type")] = NameObject("/EmbeddedFile")
        ef[NameObject("/Subtype")] = NameObject("/" + mime)
        ef_ref = self.add(ef)
        spec = self.add(DictionaryObject({NameObject("/Type"): NameObject("/Filespec"), NameObject("/F"): TextStringObject(fname), NameObject("/UF"): TextStringObject(fname), NameObject("/EF"): DictionaryObject({NameObject("/F"): ef_ref})}))
        be = DictionaryObject({NameObject("/C"): BooleanObject(o.get("controls", True) is not False), NameObject("/RC"): NumberObject(0 if o.get("loop") else 1)})
        clip = DictionaryObject({NameObject("/Type"): NameObject("/MediaClip"), NameObject("/S"): NameObject("/MCD"), NameObject("/N"): TextStringObject(name), NameObject("/CT"): TextStringObject(mime), NameObject("/D"): spec, NameObject("/P"): DictionaryObject({NameObject("/TF"): TextStringObject("TEMPACCESS")})})
        rendition = DictionaryObject({NameObject("/Type"): NameObject("/Rendition"), NameObject("/S"): NameObject("/MR"), NameObject("/N"): TextStringObject(name), NameObject("/C"): clip,
                                      NameObject("/P"): DictionaryObject({NameObject("/Type"): NameObject("/MediaPlayParams"), NameObject("/BE"): be}),
                                      NameObject("/SP"): DictionaryObject({NameObject("/Type"): NameObject("/MediaScreenParams"), NameObject("/BE"): DictionaryObject({NameObject("/O"): _n(1.0), NameObject("/W"): NumberObject(0)})})})
        annot = DictionaryObject({NameObject("/Type"): NameObject("/Annot"), NameObject("/Subtype"): NameObject("/Screen"), NameObject("/Rect"): _rect(rect), NameObject("/F"): NumberObject(4), NameObject("/P"): page_ref, NameObject("/T"): TextStringObject(name),
                                  NameObject("/MK"): DictionaryObject({NameObject("/BG"): ArrayObject([_n(0.0)])})})
        annot[NameObject("/AP")] = DictionaryObject({NameObject("/N"): self.poster(src, rect)})
        aref = self.add(annot)
        act = DictionaryObject({NameObject("/Type"): NameObject("/Action"), NameObject("/S"): NameObject("/Rendition"), NameObject("/OP"): NumberObject(0), NameObject("/AN"): aref, NameObject("/R"): rendition})
        annot[NameObject("/A")] = act
        self.annots(page).append(aref)
        if o.get("playOnPageLoad"):
            page_actions.append(DictionaryObject({NameObject("/Type"): NameObject("/Action"), NameObject("/S"): NameObject("/Rendition"), NameObject("/OP"): NumberObject(0), NameObject("/AN"): aref, NameObject("/R"): rendition}))
        self.counts["media"] += 1

    def interactive(self) -> None:
        if not self.info:
            return
        self.transitions()
        for pg, entry in zip(self.w.pages, self.info.get("pages", [])):
            ref = pg.indirect_reference
            actions: list = []
            for it in entry.get("items", []):
                if it["type"] == "media":
                    self.media(pg, ref, it, actions)
            self.fields_for(pg, ref, [i for i in entry.get("items", []) if i["type"] != "media"])
            if actions:
                chain = actions[0]
                cur = chain
                for nxt in actions[1:]:
                    cur[NameObject("/Next")] = nxt
                    cur = nxt
                aa = pg.get("/AA")
                aa = aa.get_object() if aa is not None else DictionaryObject()
                aa[NameObject("/O")] = chain
                pg[NameObject("/AA")] = aa
        self.acroform()
        root = self.w._root_object
        o = self.options
        layout = LAYOUTS.get(str(o.get("pageLayout") or ""))
        if layout:
            root[NameObject("/PageLayout")] = NameObject("/" + layout)
        if o.get("fullScreen"):
            root[NameObject("/PageMode")] = NameObject("/FullScreen")
        elif o.get("bookmarksPanel") and "/Outlines" in root:
            root[NameObject("/PageMode")] = NameObject("/UseOutlines")
        view = o.get("view")
        if view and len(self.w.pages):
            first = self.w.pages[0].indirect_reference
            dest = {"fitPage": [first, NameObject("/Fit")], "fitWidth": [first, NameObject("/FitH"), NumberObject(0)], "actual": [first, NameObject("/XYZ"), NumberObject(0), NumberObject(0), NumberObject(1)]}.get(str(view))
            if dest:
                root[NameObject("/OpenAction")] = ArrayObject(dest)
        self.w.pdf_header = "%PDF-1.7"

    # ---- PDF/X
    def rgb_images_to_cmyk(self) -> int:
        from PIL import Image
        from pypdf.filters import _xobj_to_image
        done = 0
        seen: set[int] = set()
        for pg in self.w.pages:
            xo = (pg.get("/Resources") or {}).get("/XObject")
            xo = xo.get_object() if xo is not None else {}
            for k in list(xo):
                ref = xo[k]
                obj = ref.get_object()
                if obj.get("/Subtype") != "/Image" or id(obj) in seen:
                    continue
                seen.add(id(obj))
                cs = obj.get("/ColorSpace")
                cs = cs.get_object() if cs is not None else None
                rgb = cs == "/DeviceRGB" or (isinstance(cs, ArrayObject) and len(cs) and cs[0] == "/ICCBased" and cs[1].get_object().get("/N") == 3)
                if not rgb:
                    continue
                try:
                    _, _, im = _xobj_to_image(obj)
                    im = im.convert("RGB").convert("CMYK")
                except Exception:  # noqa: BLE001
                    self.warnings.append("an RGB picture could not be converted to CMYK")
                    continue
                import zlib
                for key in ("/Filter", "/DecodeParms", "/Decode"):
                    obj.pop(NameObject(key), None)
                obj._data = zlib.compress(im.tobytes())  # type: ignore[attr-defined]
                obj[NameObject("/Filter")] = NameObject("/FlateDecode")
                obj[NameObject("/ColorSpace")] = NameObject("/DeviceCMYK")
                obj[NameObject("/BitsPerComponent")] = NumberObject(8)
                obj[NameObject("/Width")] = NumberObject(im.width)
                obj[NameObject("/Height")] = NumberObject(im.height)
                done += 1
        return done

    def pdfx(self, level: str, title: str) -> None:
        spec = PDFX[level]
        w = self.w
        root = w._root_object
        # Links and widgets do not belong in a press file; the print boxes do.
        for pg in w.pages:
            if "/Annots" in pg:
                del pg["/Annots"]
            if "/Trans" in pg:
                del pg["/Trans"]
            media = pg.mediabox
            trim = pg.trimbox
            if "/TrimBox" not in pg:
                pg[NameObject("/TrimBox")] = ArrayObject([FloatObject(float(v)) for v in (trim.left, trim.bottom, trim.right, trim.top)])
            if "/BleedBox" not in pg:
                pg[NameObject("/BleedBox")] = ArrayObject([FloatObject(float(v)) for v in (media.left, media.bottom, media.right, media.top)])
            pg.pop(NameObject("/ArtBox"), None)
        if level == "x1a":
            n = self.rgb_images_to_cmyk()
            if n:
                self.warnings.append(f"{n} RGB {'picture was' if n == 1 else 'pictures were'} converted to CMYK with the plain formula; convert pictures with a profile for press colour")
        # The bundled CC0 CGATS TR 001 profile is a compact CMYK to Lab table filed as a scanner profile; an output
        # intent wants an output (printer) class profile, so the class is set to prtr and the unset profile ID cleared.
        icc = bytearray(ICC.read_bytes())
        icc[12:16] = b"prtr"
        icc[84:100] = bytes(16)
        prof = DecodedStreamObject()
        prof.set_data(bytes(icc))
        prof[NameObject("/N")] = NumberObject(4)
        intent = DictionaryObject({NameObject("/Type"): NameObject("/OutputIntent"), NameObject("/S"): NameObject("/GTS_PDFX"), NameObject("/OutputConditionIdentifier"): TextStringObject("CGATS TR 001"),
                                   NameObject("/OutputCondition"): TextStringObject("CGATS TR 001 (SWOP) compatible, compact profile"), NameObject("/RegistryName"): TextStringObject("http://www.color.org"),
                                   NameObject("/Info"): TextStringObject("CGATS TR 001 compatible CMYK"), NameObject("/DestOutputProfile"): w._add_object(prof)})
        root[NameObject("/OutputIntents")] = ArrayObject([w._add_object(intent)])
        now = datetime.now(timezone.utc)
        stamp = now.strftime("D:%Y%m%d%H%M%S+00'00'")
        info = w._info.get_object() if w._info is not None else None
        if info is None:
            info = DictionaryObject()
            w._info = w._add_object(info)
        info[NameObject("/GTS_PDFXVersion")] = TextStringObject(spec["id"])
        info[NameObject("/Trapped")] = NameObject("/False")
        info[NameObject("/ModDate")] = TextStringObject(stamp)
        if "/CreationDate" not in info:
            info[NameObject("/CreationDate")] = TextStringObject(stamp)
        info[NameObject("/Title")] = TextStringObject(title or "Untitled")
        info.setdefault(NameObject("/Creator"), TextStringObject("formdes"))
        info[NameObject("/Producer")] = TextStringObject("formdes")
        iso = now.strftime("%Y-%m-%dT%H:%M:%S+00:00")
        esc = lambda s: s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")  # noqa: E731
        xmp = f"""<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:pdf="http://ns.adobe.com/pdf/1.3/" xmlns:pdfx="http://ns.adobe.com/pdfx/1.3/" xmlns:pdfxid="http://www.npes.org/pdfx/ns/id/" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:xmpMM="http://ns.adobe.com/xap/1.0/mm/" xmlns:dc="http://purl.org/dc/elements/1.1/">
<pdf:Producer>formdes</pdf:Producer><pdf:Trapped>False</pdf:Trapped>
<pdfxid:GTS_PDFXVersion>{spec['id']}</pdfxid:GTS_PDFXVersion>{"<pdfxid:GTS_PDFXConformance>" + spec['conformance'] + "</pdfxid:GTS_PDFXConformance>" if level == "x1a" else ""}
<pdfx:GTS_PDFXVersion>{spec['id']}</pdfx:GTS_PDFXVersion>
<xmp:CreatorTool>formdes</xmp:CreatorTool><xmp:CreateDate>{iso}</xmp:CreateDate><xmp:ModifyDate>{iso}</xmp:ModifyDate><xmp:MetadataDate>{iso}</xmp:MetadataDate>
<xmpMM:DocumentID>uuid:{hashlib.md5((title + iso).encode()).hexdigest()}</xmpMM:DocumentID><xmpMM:InstanceID>uuid:{hashlib.md5((iso + title).encode()).hexdigest()}</xmpMM:InstanceID>
<dc:format>application/pdf</dc:format><dc:title><rdf:Alt><rdf:li xml:lang="x-default">{esc(title or 'Untitled')}</rdf:li></rdf:Alt></dc:title>
</rdf:Description></rdf:RDF></x:xmpmeta>
<?xpacket end="w"?>"""
        md = DecodedStreamObject()
        md.set_data(xmp.encode("utf-8"))
        md[NameObject("/Type")] = NameObject("/Metadata")
        md[NameObject("/Subtype")] = NameObject("/XML")
        root[NameObject("/Metadata")] = w._add_object(md)
        w.pdf_header = spec["header"]
        ident = hashlib.md5((title + iso).encode()).digest()
        w._ID = ArrayObject([ByteStringObject(ident), ByteStringObject(ident)])

    def finish(self) -> bytes:
        out = io.BytesIO()
        self.w.write(out)
        return out.getvalue()


def check_pdfx(data: bytes, level: str) -> list[str]:
    """What a press checker reads first: version keys, output intent, boxes, embedded fonts, colour spaces."""
    issues: list[str] = []
    r = PdfReader(io.BytesIO(data))
    spec = PDFX[level]
    info = r.trailer.get("/Info")
    info = info.get_object() if info is not None else {}
    if info.get("/GTS_PDFXVersion") != spec["id"]:
        issues.append("GTS_PDFXVersion missing from Info")
    if info.get("/Trapped") not in ("/False", "/True"):
        issues.append("Trapped missing from Info")
    root = r.trailer["/Root"]
    oi = root.get("/OutputIntents")
    if not oi or oi[0].get_object().get("/S") != "/GTS_PDFX":
        issues.append("no GTS_PDFX output intent")
    elif "/DestOutputProfile" not in oi[0].get_object():
        issues.append("output intent has no profile")
    if "/ID" not in r.trailer:
        issues.append("no file identifier")
    meta = root.get("/Metadata")
    if meta is None or spec["id"].encode() not in meta.get_object().get_data():
        issues.append("XMP does not carry the PDF/X version")
    for i, pg in enumerate(r.pages):
        if "/TrimBox" not in pg and "/ArtBox" not in pg:
            issues.append(f"page {i + 1}: no TrimBox")
        if "/Annots" in pg:
            issues.append(f"page {i + 1}: annotations")
        res = pg.get("/Resources") or {}
        for fk, fv in (res.get("/Font") or {}).items():
            f = fv.get_object()
            d = f.get("/DescendantFonts")
            f2 = d[0].get_object() if d else f
            fd = f2.get("/FontDescriptor")
            fd = fd.get_object() if fd is not None else {}
            if not any(k in fd for k in ("/FontFile", "/FontFile2", "/FontFile3")):
                issues.append(f"page {i + 1}: font {f.get('/BaseFont')} is not embedded")
        for k, v in (res.get("/ExtGState") or {}).items():
            g = v.get_object()
            if g.get("/SMask", "/None") != "/None" or float(g.get("/ca", 1)) < 1 or float(g.get("/CA", 1)) < 1 or g.get("/BM", "/Normal") not in ("/Normal", "/Compatible"):
                issues.append(f"page {i + 1}: transparency in {k}")
        if level == "x1a":
            for k, v in (res.get("/XObject") or {}).items():
                o = v.get_object()
                if o.get("/Subtype") == "/Image":
                    cs = o.get("/ColorSpace")
                    cs = cs.get_object() if cs is not None else None
                    if cs == "/DeviceRGB" or (isinstance(cs, list) and cs and cs[0] in ("/ICCBased", "/CalRGB") and (cs[0] == "/CalRGB" or cs[1].get_object().get("/N") == 3)):
                        issues.append(f"page {i + 1}: RGB picture {k}")
            try:
                content = pg.get_contents().get_data() if pg.get_contents() else b""
            except Exception:  # noqa: BLE001
                content = b""
            import re
            if re.search(rb"(?<![A-Za-z])(rg|RG)(?![A-Za-z])", content):
                issues.append(f"page {i + 1}: RGB colour in the page content")
    return issues


def _read(result: dict, path: str | None) -> bytes:
    if path:
        return Path(path).read_bytes()
    return base64.b64decode(result.get("base64", ""))


async def apply(result: dict, options: dict, path: str | None, source: str) -> dict:
    """Post-process a `file.exportPdf` reply when the export is interactive or asks for PDF/X."""
    level = result.get("pdfx") if isinstance(result, dict) else None
    # A PDF/X file is a press file: no fields, media or transitions, so a standard wins over the interactive kind.
    interactive = bool(options.get("media") or options.get("interactive")) and not level
    if not level and not interactive:
        return result
    data = _read(result, path)
    tmp = Path(tempfile.mkdtemp(prefix="fd-pdfpost-"))
    try:
        info = None
        if interactive:
            try:
                info = await core.exec("file.interactiveInfo", {"pages": options.get("pages"), "mediaDir": str(tmp)} if options.get("pages") else {"mediaDir": str(tmp)}, source=source)
            except CoreError:
                info = None

        def work() -> tuple[bytes, list[str], dict]:
            b = Builder(data, info, tmp, options)
            if interactive:
                b.interactive()
            issues: list[str] = []
            out_title = str(options.get("title") or "")
            if level:
                if not out_title:
                    t = PdfReader(io.BytesIO(data)).metadata
                    out_title = str(t.title) if t and t.title else ""
                b.pdfx(level, out_title)
            out = b.finish()
            if level:
                issues = check_pdfx(out, level)
            return out, b.warnings + issues, b.counts

        out, notes, counts = await asyncio.to_thread(work)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    result = dict(result)
    if path:
        Path(path).write_bytes(out)
    else:
        result["base64"] = base64.b64encode(out).decode()
    result["bytes"] = len(out)
    result["warnings"] = [*(result.get("warnings") or []), *notes]
    if interactive:
        result["interactive"] = counts
    if level:
        result["standard"] = PDFX[level]["id"]
    return result
