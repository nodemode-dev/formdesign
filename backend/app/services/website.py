import asyncio
import hashlib
import io
import json
import re
import secrets
import shutil
import time
import unicodedata
import zipfile
from dataclasses import dataclass, field
from datetime import datetime, timezone
from email.utils import format_datetime
from html import escape, unescape
from pathlib import Path

from PIL import Image

from app import __version__
from app.core import config
from app.engine.core import CoreError, core
from app.engine.documents import documents
from app.intake.store import packs
from app.services import motion
from app.templates import registry

PUBLISHED_DIR = config.DATA_DIR / "Published"
SESSION = secrets.token_hex(6)
SPLITS = ("auto", "articles", "pages", "single")
LAYOUTS = ("article", "landing", "card", "email", "docs")
IMG_MAX = 1600
IMG_SMALL = 720
PAGE_PX = 1400

SLOT_ROLES = {
    "title": "title", "headline": "title", "name": "byline", "subtitle": "dek", "deck": "dek", "dek": "dek", "tagline": "dek", "role": "dek",
    "intro": "lead", "summary": "lead", "kicker": "kicker", "section": "kicker", "label": "kicker", "issue": "furniture", "folio": "furniture",
    "byline": "byline", "credit": "byline", "author": "byline", "quotecredit": "credit", "date": "meta", "eventdate": "meta", "eventtime": "meta",
    "location": "meta", "duedate": "meta", "caption": "caption", "quote": "quote", "pullquote": "quote", "cta": "cta", "url": "contact",
    "email": "contact", "phone": "contact", "address": "contact", "contact": "contact", "nav": "nav", "footer": "footer", "legal": "footer",
    "logo": "brand", "brand": "brand", "stat": "stat", "toc": "toc", "hero": "hero", "image": "image", "portrait": "image", "gallery": "image",
    "product": "image", "photo": "image", "cover": "hero",
}
STYLE_ROLES = [
    (re.compile(r"folio|running|page ?num"), "furniture"),
    (re.compile(r"caption"), "caption"),
    (re.compile(r"pull ?quote|^quote$|\bquote$"), "quote"),
    (re.compile(r"kicker|eyebrow|overline|^label$"), "kicker"),
    (re.compile(r"sub ?title|deck$|\bdek\b|standfirst|tagline"), "dek"),
    (re.compile(r"byline|^author$"), "byline"),
    (re.compile(r"^cta$|button"), "cta"),
    (re.compile(r"^stat$"), "stat"),
    (re.compile(r"^nav$"), "nav"),
    (re.compile(r"title|headline|masthead"), "title"),
]
HEAD_ROLES = ("kicker", "title", "dek", "byline", "meta")
LANGS = {"English": "en", "German": "de", "French": "fr", "Spanish": "es", "Italian": "it", "Portuguese": "pt", "Dutch": "nl", "Swedish": "sv",
         "Danish": "da", "Norwegian": "nb", "Finnish": "fi", "Polish": "pl", "Czech": "cs", "Russian": "ru", "Japanese": "ja", "Chinese": "zh",
         "Korean": "ko", "Arabic": "ar", "Hebrew": "he", "Greek": "el", "Turkish": "tr", "Hungarian": "hu", "Ukrainian": "uk", "Catalan": "ca"}
SERIF_HINTS = ("serif", "garamond", "baskerville", "lora", "merriweather", "newsreader", "playfair", "fraunces", "cormorant", "abril", "times", "georgia")
MONO_HINTS = ("mono", "code", "courier")
PLACEHOLDER = re.compile(r"ph-[0-9a-f]{12}\.")
URL_RE = re.compile(r"(https?://[^\s<]+|www\.[^\s<]+|[\w.+-]+@[\w-]+\.[\w.-]+)")


@dataclass
class Block:
    raw: dict
    role: str
    order: int

    @property
    def kind(self) -> str:
        return self.raw["kind"]

    @property
    def page(self) -> int:
        return int(self.raw.get("page", 0))

    @property
    def paras(self) -> list[dict]:
        return self.raw.get("paras") or []

    @property
    def text(self) -> str:
        return "\n".join(plain(p) for p in self.paras).strip()

    @property
    def chars(self) -> int:
        return len(self.text)

    def center(self) -> tuple[float, float]:
        b = self.raw.get("bounds") or [0, 0, 0, 0]
        return ((b[0] + b[2]) / 2, (b[1] + b[3]) / 2)

    def aspect(self) -> float:
        b = self.raw.get("bounds") or [0, 0, 1, 1]
        return max(0.01, b[2] - b[0]) / max(0.01, b[3] - b[1])


@dataclass
class WebPage:
    slug: str
    title: str
    blocks: list[Block]
    kind: str = "article"
    dek: str = ""
    byline: str = ""
    image: Block | None = None
    pages: list[int] = field(default_factory=list)
    words: int = 0

    @property
    def href(self) -> str:
        return "index.html" if self.slug == "index" else f"{self.slug}.html"


@dataclass
class Site:
    files: dict[str, bytes] = field(default_factory=dict)
    pages: list[WebPage] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    split: str = "single"
    layout: str = "article"
    title: str = ""

    def add(self, path: str, data: bytes | str) -> None:
        self.files[path] = data.encode() if isinstance(data, str) else data

    def summary(self) -> dict:
        return {"title": self.title, "split": self.split, "layout": self.layout, "files": len(self.files),
                "bytes": sum(len(v) for v in self.files.values()),
                "pages": [{"file": p.href, "title": p.title, "kind": p.kind, "words": p.words} for p in self.pages], "warnings": self.warnings}


def plain(p: dict) -> str:
    return "".join(r.get("t", "") for r in p.get("runs", []))


def slugify(text: str, used: set[str] | None = None, fallback: str = "page") -> str:
    s = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode().lower()
    s = re.sub(r"[^a-z0-9]+", "-", s).strip("-")[:60].strip("-") or fallback
    if used is not None:
        base, n = s, 2
        while s in used:
            s = f"{base}-{n}"
            n += 1
        used.add(s)
    return s


def css_class(name: str) -> str:
    return "ps-" + slugify(name, fallback="basic")


def hex_rgb(h: str) -> tuple[int, int, int]:
    h = h.lstrip("#")
    return int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)


def luminance(h: str) -> float:
    def ch(c: int) -> float:
        v = c / 255
        return v / 12.92 if v <= 0.03928 else ((v + 0.055) / 1.055) ** 2.4
    r, g, b = hex_rgb(h)
    return 0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)


def contrast(a: str, b: str) -> float:
    la, lb = sorted((luminance(a), luminance(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def stack(family: str) -> str:
    f = family.lower()
    generic = "ui-monospace, monospace" if any(k in f for k in MONO_HINTS) else (
        "Georgia, 'Times New Roman', serif" if any(k in f for k in SERIF_HINTS) else "system-ui, -apple-system, 'Segoe UI', sans-serif")
    return f"\"{family}\", {generic}"


def language(core_lang: str, override: str | None) -> str:
    if override:
        return override
    head = (core_lang or "English").split(":")[0].strip()
    code = LANGS.get(head, "en")
    if head == "English" and "UK" in core_lang:
        return "en-GB"
    if head == "English" and "USA" in core_lang:
        return "en-US"
    return code


def role_of(raw: dict, styles: dict) -> str:
    slot = (raw.get("slot") or "").lower()
    if raw["kind"] in ("image", "video", "audio"):
        base = re.sub(r"\d+$", "", slot)
        r = SLOT_ROLES.get(base)
        return r if r in ("hero", "brand", "image") else "image"
    if slot:
        parts = [re.sub(r"\d+$", "", s) for s in slot.split(".")]
        if parts[0] == "stat" and parts[-1] == "label":
            return "statlabel"
        for base in reversed(parts):
            if base in SLOT_ROLES:
                return SLOT_ROLES[base]
        return "body"
    first = (raw.get("paras") or [{}])[0].get("style", "")
    name = first.lower()
    for rx, role in STYLE_ROLES:
        if rx.search(name):
            return role
    return "body"


class Builder:
    def __init__(self, content: dict, options: dict, meta, rt: dict | None, pack: dict | None) -> None:
        self.c = content
        self.o = options
        self.meta = meta
        self.rt = rt or {}
        self.pack = pack or {}
        self.site = Site()
        self.styles = content.get("styles", {}).get("paragraph", {})
        self.cstyles = content.get("styles", {}).get("character", {})
        self.blocks = [Block(b, role_of(b, self.styles), i) for i, b in enumerate(content.get("blocks", []))]
        self.images: dict[int, dict] = {}
        self.page_url: dict[int, str] = {}
        sw = content.get("swatches", {})
        self.paper = sw.get("Paper Color") or sw.get("[Paper]") or "#ffffff"
        self.ink = sw.get("Ink") or "#1b1a1f"
        if contrast(self.ink, self.paper) < 4.5:
            self.ink = "#1b1a1f" if luminance(self.paper) > 0.4 else "#f5f3ef"
        self.accent = sw.get("Accent") or next((v for k, v in sw.items() if not k.startswith("[") and "=" not in k and contrast(v, self.paper) > 2.5), "#d9481c")
        self.muted = sw.get("Muted") or "#6e6a66"
        self.tint = sw.get("Tint") or "#f4f2ee"
        self.body_style = self._body_style()
        self.body_pt = float(self.styles.get(self.body_style, {}).get("size") or 10)
        self.fonts_used: set[tuple[str, str]] = set()
        self.used: set[str] = {self.body_style}
        self.placeholders = 0
        self._h1 = False
        self._img_seen = False
        self.mo: dict | None = None
        self.mo_ids: set[str] = set()
        self.mo_used: set[str] = set()

    def anim(self, b: Block, html: str) -> str:
        if not self.mo or not html:
            return html
        for i in [b.raw.get("item"), *(b.raw.get("frames") or [])]:
            if i is not None and str(i) in self.mo_ids:
                self.mo_used.add(str(i))
                return f'<div class="fd-mo" data-fd="{i}">{html}</div>'
        return html

    def page_motion(self) -> str:
        if not self.mo or not self.mo_used:
            return ""
        anims = {k: v for k, v in self.mo["anims"].items() if k in self.mo_used}
        groups = [g for g in self.mo["groups"] if any(str(i) in anims for i in g)]
        if not anims:
            return ""
        return motion.page_script({"mode": "flow", "anims": anims, "groups": groups})

    def _body_style(self) -> str:
        counts: dict[str, int] = {}
        for b in self.blocks:
            if b.role in ("body", "lead"):
                for p in b.paras:
                    counts[p["style"]] = counts.get(p["style"], 0) + len(plain(p))
        if counts:
            return max(counts, key=counts.get)
        return "Basic Paragraph" if "Basic Paragraph" in self.styles else next(iter(self.styles), "Basic Paragraph")

    @property
    def layout(self) -> str:
        want = self.o.get("layout")
        if want in LAYOUTS:
            return want
        hint = (self.rt.get("web") or {}).get("layout")
        return hint if hint in LAYOUTS else "article"

    def split_mode(self) -> str:
        want = self.o.get("split", "auto")
        if want in SPLITS and want != "auto":
            return want
        if self.c.get("articles"):
            return "articles"
        n = len(self.c.get("pages", []))
        if self.layout in ("landing", "card", "email", "docs") or self.c.get("intent") == "web":
            return "pages" if n > 1 else "single"
        if n <= 1:
            return "single"
        titles = sum(1 for b in self.blocks if b.role == "title")
        return "articles" if titles >= 2 else "single"

    def title_text(self, blocks: list[Block]) -> str:
        t = next((b for b in blocks if b.role == "title" and b.text), None)
        if t:
            return re.sub(r"\s+", " ", t.text).strip()
        best = max((b for b in blocks if b.kind == "story" and b.text), default=None,
                   key=lambda b: float(self.styles.get(b.paras[0]["style"], {}).get("size") or 0))
        if best and float(self.styles.get(best.paras[0]["style"], {}).get("size") or 0) >= self.body_pt * 1.5:
            return re.sub(r"\s+", " ", plain(best.paras[0])).strip()[:120]
        return ""

    def make_page(self, slug: str, blocks: list[Block], kind: str, fallback: str) -> WebPage:
        title = self.title_text(blocks) or fallback
        dek = next((b.text for b in blocks if b.role in ("dek", "lead") and b.text), "")
        byline = next((b.text for b in blocks if b.role == "byline" and b.text), "")
        image = next((b for b in blocks if b.kind == "image" and b.role in ("hero", "image")), None)
        words = sum(len(b.text.split()) for b in blocks if b.kind == "story" and b.role not in ("furniture", "nav", "footer"))
        pages = sorted({b.page for b in blocks})
        return WebPage(slug, title, blocks, kind, dek, byline, image, pages, words)

    def split(self) -> list[WebPage]:
        mode = self.split_mode()
        self.site.split = mode
        used = {"index", "edition", "404", "feed", "sitemap"}
        doc_title = self.site.title
        out: list[WebPage] = []
        if mode == "single":
            out.append(self.make_page("index", list(self.blocks), "article", doc_title))
        elif mode == "pages":
            by_page: dict[int, list[Block]] = {}
            for b in self.blocks:
                by_page.setdefault(b.page, []).append(b)
            names = {p["index"]: p["name"] for p in self.c.get("pages", [])}
            for k, pi in enumerate(sorted(by_page)):
                blocks = by_page[pi]
                if k == 0:
                    out.append(self.make_page("index", blocks, "page", doc_title))
                else:
                    t = self.title_text(blocks) or f"Page {names.get(pi, pi + 1)}"
                    out.append(self.make_page(slugify(t, used), blocks, "page", t))
        else:
            groups = self._article_groups()
            front: list[Block] = []
            if groups and groups[0][0] is None:
                front = groups.pop(0)[1]
            if len(groups) > 1 and min(b.page for b in groups[0][1]) == 0 and all(b.page == 0 for b in front):
                words = sum(len(b.text.split()) for b in groups[0][1] if b.kind == "story")
                if words < 150:
                    front = front + groups.pop(0)[1]
            merged: list[tuple[str | None, list[Block]]] = []
            for name, blocks in groups:
                words = sum(len(b.text.split()) for b in blocks if b.kind == "story")
                if merged and words < 120:
                    merged[-1] = (merged[-1][0], merged[-1][1] + blocks)
                else:
                    merged.append((name, blocks))
            groups = merged if len(merged) >= 2 else []
            home = self.make_page("index", front, "home", doc_title)
            out.append(home)
            for name, blocks in groups:
                p = self.make_page("", blocks, "article", name or "Untitled")
                p.slug = slugify(p.title, used, fallback="article")
                out.append(p)
            if len(out) == 1:
                out[0] = self.make_page("index", list(self.blocks), "article", doc_title)
                self.site.split = "single"
        for p in out:
            for pi in p.pages:
                self.page_url.setdefault(pi, p.href)
        return out

    def _article_groups(self) -> list[tuple[str | None, list[Block]]]:
        arts = self.c.get("articles") or []
        if arts:
            by_item: dict[int, Block] = {}
            for b in self.blocks:
                by_item[b.raw.get("item")] = b
                for f in b.raw.get("frames") or []:
                    by_item[f] = b
            groups = []
            for a in arts:
                seen, blocks = set(), []
                for iid in a.get("items", []):
                    b = by_item.get(iid)
                    if b and b.order not in seen:
                        seen.add(b.order)
                        blocks.append(b)
                if blocks:
                    groups.append((a.get("name") or None, blocks))
            return groups
        groups: list[tuple[str | None, list[Block]]] = []
        cur: list[Block] = []
        cur_name: str | None = None
        for b in self.blocks:
            if b.role == "furniture":
                continue
            if b.role == "title" and b.text:
                carry = []
                while cur and cur[-1].role == "kicker" and cur[-1].page == b.page:
                    carry.insert(0, cur.pop())
                if cur:
                    groups.append((cur_name, cur))
                cur, cur_name = carry + [b], re.sub(r"\s+", " ", b.text).strip()
                continue
            cur.append(b)
        if cur:
            groups.append((cur_name, cur))
        return groups

    async def image(self, b: Block, wide: bool = False) -> dict | None:
        aid = b.raw.get("asset")
        if aid is None:
            return None
        if aid in self.images:
            return self.images[aid]
        try:
            r = await core.call("web.asset", {"asset": aid})
        except CoreError as exc:
            self.site.warnings.append(str(exc))
            self.images[aid] = None
            return None
        data = r.bin or b""
        mime = r.result.get("mime", "")
        name = slugify(Path(r.result.get("name") or "image").stem, fallback="image")[:40]
        info: dict | None = None
        if b.kind in ("video", "audio"):
            ext = Path(r.result.get("name") or "").suffix.lower() or (".mp4" if b.kind == "video" else ".mp3")
            path = f"assets/media/{name}-{aid}{ext}"
            self.site.add(path, data)
            info = {"src": path}
            if b.kind == "video":
                got = await motion.poster_bytes(await motion.media_for(self.meta, int(b.raw["item"])), data, ext)
                if got:
                    info["poster"] = f"assets/media/{name}-{aid}-poster{got[1]}"
                    self.site.add(info["poster"], got[0])
        elif mime == "image/svg+xml":
            path = f"assets/img/{name}-{aid}.svg"
            self.site.add(path, data)
            info = {"src": path, "w": None, "h": None}
        elif mime == "image/gif" and data[:6] in (b"GIF89a", b"GIF87a") and b"NETSCAPE2.0" in data[:2048]:
            path = f"assets/img/{name}-{aid}.gif"
            self.site.add(path, data)
            px = r.result.get("pixels") or [None, None]
            info = {"src": path, "w": px[0], "h": px[1]}
        else:
            try:
                info = await asyncio.to_thread(self._encode, data, f"assets/img/{name}-{aid}")
            except Exception as exc:
                self.site.warnings.append(f"{r.result.get('name')}: not converted for the web ({exc.__class__.__name__}); it appears in the edition only")
        self.images[aid] = info
        return info

    def _encode(self, data: bytes, stem: str) -> dict:
        im = Image.open(io.BytesIO(data))
        im.load()
        if im.mode not in ("RGB", "RGBA"):
            im = im.convert("RGBA" if "A" in im.getbands() or im.mode == "P" else "RGB")
        w, h = im.size
        out: dict = {}
        big = im.copy()
        if w > IMG_MAX:
            big = im.resize((IMG_MAX, round(h * IMG_MAX / w)), Image.LANCZOS)
        buf = io.BytesIO()
        big.save(buf, "WEBP", quality=82, method=4)
        self.site.add(f"{stem}.webp", buf.getvalue())
        out.update(src=f"{stem}.webp", w=big.size[0], h=big.size[1])
        if big.size[0] > IMG_SMALL * 1.4:
            sm = im.resize((IMG_SMALL, round(h * IMG_SMALL / w)), Image.LANCZOS)
            buf = io.BytesIO()
            sm.save(buf, "WEBP", quality=80, method=4)
            self.site.add(f"{stem}-{IMG_SMALL}.webp", buf.getvalue())
            out["small"] = f"{stem}-{IMG_SMALL}.webp"
        return out

    def alt_for(self, b: Block) -> str:
        alt = (b.raw.get("alt") or "").strip()
        if alt:
            return alt
        name = b.raw.get("name") or ""
        for img in self.pack.get("images", []):
            if img.get("name") == name or Path(str(img.get("path", ""))).name == name:
                return (img.get("alt") or img.get("caption") or "").strip()
        return ""

    def href(self, h) -> str:
        if isinstance(h, dict):
            return self.page_url.get(int(h.get("page", 0)), "index.html")
        return str(h)

    def runs_html(self, runs: list[dict], link: bool = True) -> str:
        out = []
        for r in runs:
            t = escape(r.get("t", ""), quote=False).replace("\n", "<br>")
            if not t:
                continue
            style = []
            if r.get("color") and contrast(r["color"], self.paper) >= 2.0:
                style.append(f"color:{r['color']}")
            if r.get("caps") == "all":
                style.append("text-transform:uppercase")
            elif r.get("caps") == "small":
                style.append("font-variant:small-caps")
            if r.get("size") and 0.5 <= float(r["size"]) <= 3:
                style.append(f"font-size:{float(r['size']):.2f}em")
            if r.get("f"):
                style.append(f"font-family:{stack(r['f'])}".replace('"', "'"))
                self.fonts_used.add((r["f"], "Bold" if r.get("b") else "Regular"))
            if r.get("b"):
                t = f"<strong>{t}</strong>"
            if r.get("i"):
                t = f"<em>{t}</em>"
            if r.get("u") and not r.get("href"):
                t = f"<u>{t}</u>"
            if r.get("s"):
                t = f"<s>{t}</s>"
            if r.get("pos") in ("sup", "sub"):
                t = f"<{r['pos']}>{t}</{r['pos']}>"
            cls = f' class="cs-{slugify(r["c"], fallback="char")}"' if r.get("c") else ""
            if cls or style:
                st = f' style="{";".join(style)}"' if style else ""
                t = f"<span{cls}{st}>{t}</span>"
            if link and r.get("href") is not None:
                href = self.href(r["href"])
                ext = ' rel="noopener"' if href.startswith("http") else ""
                t = f'<a href="{escape(href)}"{ext}>{t}</a>'
            out.append(t)
        return "".join(out)

    def autolink(self, html_text: str) -> str:
        if "<a " in html_text:
            return html_text

        def sub(m: re.Match) -> str:
            v = m.group(1).rstrip(".,;:)")
            tail = m.group(1)[len(v):]
            href = f"mailto:{v}" if "@" in v and not v.startswith("http") else (v if v.startswith("http") else f"https://{v}")
            return f'<a href="{escape(unescape(href))}">{v}</a>{tail}'
        return URL_RE.sub(sub, html_text)

    def para_tag(self, style: str, role: str, first: bool, has_h1: bool) -> str:
        sp = self.styles.get(style, {})
        tag = (sp.get("tag") or "").lower()
        if tag in ("h1", "h2", "h3", "h4", "h5", "h6", "blockquote", "pre"):
            return "h2" if tag == "h1" and has_h1 else tag
        n = style.lower()
        if role == "body":
            if re.search(r"heading 1|head 1|\bsubhead|crosshead|^h1$|section head", n):
                return "h2"
            if re.search(r"heading 2|head 2|^h2$", n):
                return "h3"
            if re.search(r"heading 3|head 3|^h3$", n):
                return "h4"
            if re.search(r"block ?quote", n):
                return "blockquote"
            if re.search(r"^code", n):
                return "pre"
        return "p"

    def story_html(self, b: Block, has_h1: bool, inserts: dict[int, list[str]] | None = None) -> str:
        out: list[str] = []
        list_open: str | None = None
        drop = bool(self.styles.get(b.paras[0]["style"], {}).get("dropCap")) if b.paras else False
        for i, p in enumerate(b.paras):
            inner = self.autolink(self.runs_html(p.get("runs", []))) if b.role == "contact" else self.runs_html(p.get("runs", []))
            if not inner.strip():
                if inserts and i in inserts:
                    out.extend(inserts.pop(i))
                continue
            cls = [css_class(p["style"])]
            self.used.add(p["style"])
            if drop and i == 0:
                cls.append("dropcap")
            align = p.get("align")
            sp_align = self.styles.get(p["style"], {}).get("align")
            st = f' style="text-align:{align}"' if align in ("center", "right") and align != sp_align else ""
            dirattr = ' dir="rtl"' if p.get("rtl") else ""
            lst = p.get("list") or self.styles.get(p["style"], {}).get("list")
            if lst:
                want = "ol" if lst == "number" else "ul"
                if list_open != want:
                    if list_open:
                        out.append(f"</{list_open}>")
                    out.append(f'<{want} class="{cls[0]}">')
                    list_open = want
                out.append(f"<li{dirattr}>{inner}</li>")
            else:
                if list_open:
                    out.append(f"</{list_open}>")
                    list_open = None
                tag = self.para_tag(p["style"], b.role, i == 0, has_h1)
                out.append(f'<{tag} class="{" ".join(cls)}"{st}{dirattr}>{inner}</{tag}>')
            if inserts and i in inserts:
                if list_open:
                    out.append(f"</{list_open}>")
                    list_open = None
                out.extend(inserts.pop(i))
        if list_open:
            out.append(f"</{list_open}>")
        if inserts:
            for rest in inserts.values():
                out.extend(rest)
        return "\n".join(out)

    def heading_html(self, b: Block, tag: str, cls: str) -> str:
        parts = [self.runs_html(p.get("runs", [])) for p in b.paras if plain(p).strip()]
        style = css_class(b.paras[0]["style"]) if b.paras else ""
        if b.paras:
            self.used.add(b.paras[0]["style"])
        return f'<{tag} class="{cls} {style}">{"<br>".join(parts)}</{tag}>'

    async def figure_html(self, b: Block, caption: Block | None, cls: str = "fig", eager: bool = False) -> str:
        info = await self.image(b)
        if not info:
            return ""
        cap = f"<figcaption>{self.runs_html_join(caption)}</figcaption>" if caption else ""
        src = escape(info["src"])
        if b.kind in ("video", "audio"):
            o = await motion.media_for(self.meta, int(b.raw["item"]))
            attrs = "".join(a for on, a in ((o["controls"], " controls"), (o["autoplay"], " autoplay muted" if b.kind == "video" else " autoplay"), (o["loop"], " loop")) if on)
            if b.kind == "video":
                attrs += ' playsinline' + (f' poster="{escape(info["poster"])}"' if info.get("poster") else "")
            return self.anim(b, f'<figure class="{cls} media"><{b.kind} src="{src}"{attrs} preload="metadata"></{b.kind}>{cap}</figure>')
        alt = self.alt_for(b)
        if PLACEHOLDER.match(b.raw.get("name") or ""):
            self.placeholders += 1
        elif not alt and b.role != "brand":
            self.site.warnings.append(f"{b.raw.get('name')} has no alt text (Object Export Options, or the image's description in Make)")
        ar = min(3.0, max(0.5, b.aspect()))
        dims = f' width="{info["w"]}" height="{info["h"]}"' if info.get("w") else ""
        srcset = f' srcset="{escape(info["small"])} {IMG_SMALL}w, {src} {info["w"]}w" sizes="(max-width: 48rem) 100vw, 72rem"' if info.get("small") else ""
        load = ' fetchpriority="high"' if eager else ' loading="lazy"'
        shape = f"--ar:{ar:.3f}" if "feature" in cls or "cover" in cls else f"aspect-ratio:{ar:.3f}"
        img = f'<img src="{src}"{srcset}{dims} alt="{escape(alt)}" decoding="async"{load} style="{shape}">'
        if b.raw.get("href") is not None:
            img = f'<a href="{escape(self.href(b.raw["href"]))}">{img}</a>'
        x0, _, x1, _ = b.raw.get("bounds") or [0, 0, 1, 0]
        share = (x1 - x0) / max(1.0, float(self.c.get("pageWidth") or 612)) * 140
        width = f' style="max-width:{max(30.0, share):.0f}%"' if share < 85 and "feature" not in cls and "cover" not in cls else ""
        return self.anim(b, f'<figure class="{cls}"{width}>{img}{cap}</figure>')

    def runs_html_join(self, b: Block) -> str:
        return "<br>".join(self.runs_html(p.get("runs", [])) for p in b.paras if plain(p).strip())

    def pair_captions(self, blocks: list[Block]) -> dict[int, Block]:
        pairs: dict[int, Block] = {}
        imgs = [b for b in blocks if b.kind in ("image", "video") and b.role != "brand"]
        caps = [b for b in blocks if b.role == "caption"]
        ph = float(self.c.get("pageHeight") or 792)
        for cb in caps:
            slot = (cb.raw.get("slot") or "")
            num = re.sub(r"^\D+", "", slot)
            match = next((ib for ib in imgs if num and re.sub(r"^\D+", "", ib.raw.get("slot") or "") == num and ib.order not in pairs), None)
            if not match:
                cx, cy = cb.center()
                near = [(abs(ib.center()[0] - cx) + abs(ib.center()[1] - cy), ib) for ib in imgs if ib.page == cb.page and ib.order not in pairs]
                near.sort(key=lambda x: x[0])
                if near and near[0][0] < ph * 0.6:
                    match = near[0][1]
            if match:
                pairs[match.order] = cb
        return pairs

    @staticmethod
    def _split(blocks: list[Block], axis: int) -> list[list[Block]]:
        lo, hi = (0, 2) if axis == 0 else (1, 3)
        groups: list[list[Block]] = []
        cur: list[Block] = []
        end = float("-inf")
        for b in sorted(blocks, key=lambda x: x.raw["bounds"][lo]):
            s, e = b.raw["bounds"][lo], b.raw["bounds"][hi]
            if cur and s > end - 0.5:
                groups.append(cur)
                cur, end = [], float("-inf")
            cur.append(b)
            end = max(end, e)
        if cur:
            groups.append(cur)
        return groups

    def _merge_strips(self, strips: list[list[Block]]) -> list[list[Block]]:
        def sig(s: list[Block]) -> tuple:
            return tuple(b.kind for b in sorted(s, key=lambda x: x.raw["bounds"][0]))
        out: list[list[Block]] = []
        first: list[tuple] = []
        near = 0.025 * float(self.c.get("pageWidth") or 612)
        for s in strips:
            if out:
                cur = out[-1]
                gap = min(b.raw["bounds"][1] for b in s) - max(b.raw["bounds"][3] for b in cur)
                n_cur, n_s = len(self._split(cur, 0)), len(self._split(s, 0))
                n_u = len(self._split(cur + s, 0))
                if gap <= near and n_u >= 2 and n_u == max(n_cur, n_s) and not (sig(s) == first[-1] and len(cur) > len(s)):
                    cur.extend(s)
                    continue
            out.append(list(s))
            first.append(sig(s))
        return out

    def cut(self, blocks: list[Block], depth: int = 0) -> tuple:
        if len(blocks) > 1 and depth < 16:
            for axis in (0, 1):
                groups = self._split(blocks, axis)
                if axis == 1 and len(groups) > 1:
                    groups = self._merge_strips(groups)
                if len(groups) > 1:
                    return ("row" if axis == 0 else "stack", [self.cut(g, depth + 1) for g in groups], groups)
        return ("leaf", sorted(blocks, key=lambda x: (x.raw["bounds"][1], x.raw["bounds"][0])), None)

    async def tree_html(self, node: tuple, ctx: dict) -> str:
        kind, kids, groups = node
        if kind == "leaf":
            return await self.seq_html(kids, ctx)
        parts = [await self.tree_html(k, ctx) for k in kids]
        if kind == "stack":
            return "\n".join(p for p in parts if p)
        widths = [max(1.0, max(b.raw["bounds"][2] for b in g) - min(b.raw["bounds"][0] for b in g)) for g in groups]
        tmpl = " ".join(f"minmax(0,{w / min(widths):.2f}fr)" for w in widths)
        return f'<div class="row" style="--cols:{tmpl}">' + "".join(f'<div class="col">{p}</div>' for p in parts) + "</div>"

    async def content_html(self, page: WebPage, has_h1: bool, skip: set[int], rows: bool = False) -> str:
        blocks = [b for b in page.blocks if b.order not in skip and b.role not in ("furniture", "nav", "footer", "toc", "brand")]
        pairs = self.pair_captions(page.blocks)
        paired = {c.order for c in pairs.values()}
        bodies = [b for b in blocks if b.role in ("body", "lead") and b.kind == "story"]
        main = max(bodies, key=lambda b: b.chars) if bodies else None
        later_figs: list[Block] = []
        if main and len(main.paras) > 4 and not rows:
            pages = sorted(set(main.raw.get("pages") or [main.page]))
            later_figs = [b for b in blocks if b.kind in ("image", "video") and b.order > main.order and b.page > main.page and b.page <= pages[-1]]
        seq = [b for b in blocks if b.order not in paired and not any(b is f for f in later_figs)]
        ctx = {"page": page, "pairs": pairs, "main": main, "later": later_figs, "has_h1": has_h1, "skip": skip}
        if not rows:
            def flat(node: tuple) -> list[Block]:
                return node[1] if node[0] == "leaf" else [b for k in node[1] for b in flat(k)]
            pages_of: dict[int, list[Block]] = {}
            for b in seq:
                pages_of.setdefault(b.page, []).append(b)
            ordered = [b for pg in sorted(pages_of) for b in flat(self.cut(pages_of[pg]))]
            return await self.seq_html(ordered, ctx)
        by_page: dict[int, list[Block]] = {}
        for b in seq:
            by_page.setdefault(b.page, []).append(b)
        out = [await self.tree_html(self.cut(by_page[pg]), ctx) for pg in sorted(by_page)]
        return "\n".join(x for x in out if x)

    async def seq_html(self, seq: list[Block], ctx: dict) -> str:
        page, pairs, main, later_figs, has_h1, skip = ctx["page"], ctx["pairs"], ctx["main"], ctx["later"], ctx["has_h1"], ctx["skip"]
        out: list[str] = []
        stats: list[str] = []
        figs: list[str] = []

        def flush_stats() -> None:
            if stats:
                out.append(f'<div class="stats">{"".join(stats)}</div>')
                stats.clear()

        def flush_figs() -> None:
            if len(figs) > 1:
                out.append(f'<div class="gallery">{"".join(figs)}</div>')
            else:
                out.extend(figs)
            figs.clear()
        for b in seq:
            if b.order in skip:
                continue
            if b.role != "stat":
                flush_stats()
            if b.kind not in ("image", "video", "audio"):
                flush_figs()
            if b.kind in ("image", "video", "audio"):
                figs.append(await self.figure_html(b, pairs.get(b.order), eager=not self._img_seen))
                self._img_seen = True
            elif b.role == "stat":
                ps = [p for p in b.paras if plain(p).strip()]
                if ps:
                    num = self.runs_html(ps[0].get("runs", []))
                    self.used.add(ps[0]["style"])
                    label = self.stat_label(b, page)
                    if label:
                        skip.add(label.order)
                        ps = ps + [p for p in label.paras if plain(p).strip()]
                    rest = "".join(f"<span>{self.runs_html(p.get('runs', []))}</span>" for p in ps[1:])
                    stats.append(f'<div class="stat"><strong class="{css_class(ps[0]["style"])}">{num}</strong>{rest}</div>')
            elif b.role == "quote":
                credit = next((c for c in page.blocks if c.role == "credit" and c.order > b.order and c.page == b.page), None)
                cite = f"<cite>{self.runs_html_join(credit)}</cite>" if credit else ""
                if credit:
                    skip.add(credit.order)
                out.append(self.anim(b, f'<aside class="pullquote"><blockquote>{self.story_html(b, True)}</blockquote>{cite}</aside>'))
            elif b.role == "credit" and b.order in skip:
                continue
            elif b.role == "cta":
                out.append(self.anim(b, self.cta_html(b, page)))
            elif b.role in ("kicker", "title", "dek", "byline", "meta") and b.kind == "story":
                tag = "p"
                if b.role == "title":
                    tag = "h2" if self._h1 else "h1"
                    self._h1 = True
                out.append(self.anim(b, self.heading_html(b, tag, f"r-{b.role}")))
            elif b is main and later_figs:
                n = len(main.paras)
                pages = sorted(set(main.raw.get("pages") or [main.page]))
                p0, p1 = pages[0], pages[-1]
                inserts: dict[int, list[str]] = {}
                for f in later_figs:
                    at = min(n - 1, max(1, round((f.page - p0 + 0.5) / max(1, p1 - p0 + 1) * n)))
                    inserts.setdefault(at, []).append(await self.figure_html(f, pairs.get(f.order)))
                out.append(self.anim(b, f'<div class="story r-{b.role}">{self.story_html(b, has_h1, inserts)}</div>'))
            else:
                out.append(self.anim(b, f'<div class="story r-{b.role}">{self.story_html(b, has_h1)}</div>'))
        flush_stats()
        flush_figs()
        return "\n".join(x for x in out if x)

    def stat_label(self, b: Block, page: WebPage) -> Block | None:
        slot = (b.raw.get("slot") or "").split(".")[0]
        if slot:
            hit = next((x for x in page.blocks if x.role == "statlabel" and (x.raw.get("slot") or "").split(".")[0] == slot), None)
            if hit:
                return hit
        x0, _, x1, y1 = b.raw["bounds"]
        near = 0.04 * float(self.c.get("pageWidth") or 612)
        for x in page.blocks:
            if x is b or x.kind != "story" or x.role in ("title", "stat", "quote", "cta") or x.page != b.page:
                continue
            bx0, by0, bx1, _ = x.raw["bounds"]
            if -2 <= by0 - y1 <= near and min(x1, bx1) - max(x0, bx0) > 0.5 * min(x1 - x0, bx1 - bx0):
                return x
        return None

    def cta_html(self, b: Block, page: WebPage) -> str:
        text = b.text.split("\n")[0]
        href = next((r.get("href") for p in b.paras for r in p.get("runs", []) if r.get("href") is not None), None)
        if href is None:
            url = next((x.text.strip() for x in page.blocks if (x.raw.get("slot") or "").startswith("url") and x.text), "")
            if not url:
                url = next((m.group(1) for x in page.blocks if x.role == "contact" for m in [URL_RE.search(x.text)] if m), "")
            if "@" in url and not url.startswith("http"):
                href = f"mailto:{url}"
            else:
                href = url if url.startswith("http") else (f"https://{url}" if url and "." in url and " " not in url else "#")
                if href == "#":
                    self.site.warnings.append(f"The button \"{text}\" has no link: select its frame and add a hyperlink (Window > Interactive > Hyperlinks)")
        return f'<p class="cta"><a class="btn" href="{escape(self.href(href))}">{escape(text)}</a></p>'

    async def article_html(self, page: WebPage) -> str:
        first = page.pages[0] if page.pages else 0
        title_block = next((b for b in page.blocks if b.role == "title" and b.kind == "story" and b.text), None)
        head_page = title_block.page if title_block else first
        pw, ph = float(self.c.get("pageWidth") or 612), float(self.c.get("pageHeight") or 792)

        def near(b: Block) -> bool:
            if not title_block:
                return True
            ty0, ty1 = title_block.raw["bounds"][1], title_block.raw["bounds"][3]
            return b.raw["bounds"][3] >= ty0 - 0.12 * pw and b.raw["bounds"][1] <= ty1 + 0.2 * pw
        head_blocks, roles = [], set()
        for b in page.blocks:
            if b.role in HEAD_ROLES and b.kind == "story" and b.page == head_page and b.text and near(b) and (b.role not in roles or b.role == "meta"):
                if b.role == "title" and b is not title_block:
                    continue
                roles.add(b.role)
                head_blocks.append(b)
        head_blocks.sort(key=lambda b: HEAD_ROLES.index(b.role))
        skip = {b.order for b in head_blocks}
        parts = []
        for b in head_blocks:
            if b.role == "title":
                parts.append(self.heading_html(b, "h1", "r-title"))
            else:
                parts.append(self.heading_html(b, "p", f"r-{b.role}"))
        if not title_block:
            parts.insert(0, f'<h1 class="r-title {css_class(self.title_style())}">{escape(page.title)}</h1>')
        if page.words > 120:
            parts.append(f'<p class="r-meta readtime">{max(1, round(page.words / 230))} min read</p>')
        hero_html = ""
        def big(b: Block) -> bool:
            x0, y0, x1, y1 = b.raw["bounds"]
            return (x1 - x0) * (y1 - y0) >= 0.18 * pw * ph
        hero = next((b for b in page.blocks if b.kind == "image" and b.role == "hero" and b.page in (first, head_page)), None) or next(
            (b for b in page.blocks if b.kind == "image" and b.role == "image" and b.page in (first, head_page) and big(b)), None)
        if hero:
            pairs = self.pair_captions(page.blocks)
            cap = pairs.get(hero.order)
            hero_html = await self.figure_html(hero, cap, "fig feature", eager=True)
            self._img_seen = True
            skip.add(hero.order)
            if cap:
                skip.add(cap.order)
        self._h1 = True
        body = await self.content_html(page, True, skip)
        return f'<article class="post">\n<header class="post-head">{"".join(parts)}</header>\n{hero_html}\n<div class="post-body">\n{body}\n</div>\n</article>'

    def title_style(self) -> str:
        name = next((n for n in ("Title", "Heading 1", "Headline") if n in self.styles), None)
        name = name or max(self.styles, key=lambda n: float(self.styles[n].get("size") or 0), default="Basic Paragraph")
        self.used.add(name)
        return name

    async def page_html(self, page: WebPage) -> str:
        composed = self.site.split == "single" and len(self.c.get("pages", [])) == 1 and page.words < 400
        if self.layout in ("landing", "card", "email", "docs") or page.kind == "page" or composed:
            body = await self.content_html(page, True, set(), rows=True)
            lead = "" if self._h1 else f'<h1 class="visually-hidden">{escape(page.title)}</h1>\n'
            return f'{lead}<div class="sections" id="main-content">\n{body}\n</div>'
        if page.kind == "home":
            return await self.home_html(page)
        return await self.article_html(page)

    async def home_html(self, page: WebPage) -> str:
        cards = []
        for p in self.site.pages:
            if p.kind != "article":
                continue
            img = ""
            if p.image:
                info = await self.image(p.image)
                if info and info.get("src") and p.image.kind == "image":
                    src = info.get("small") or info["src"]
                    img = f'<img src="{escape(src)}" alt="" loading="lazy" decoding="async">'
            kicker = next((b.text for b in p.blocks if b.role == "kicker" and b.text), "")
            meta = f"{max(1, round(p.words / 230))} min read" if p.words > 120 else ""
            cards.append(
                f'<a class="card" href="{escape(p.href)}"><div class="card-img">{img}</div><div class="card-text">'
                + (f'<p class="card-kicker">{escape(kicker)}</p>' if kicker else "")
                + f"<h2>{escape(p.title)}</h2>"
                + (f'<p class="card-dek">{escape(p.dek[:220])}</p>' if p.dek else "")
                + (f'<p class="card-meta">{escape(" · ".join(x for x in (p.byline, meta) if x))}</p>' if (p.byline or meta) else "")
                + "</div></a>")
        cover = ""
        if page.blocks:
            fig_b = next((b for b in page.blocks if b.kind == "image" and b.role in ("hero", "image")), None)
            fig = await self.figure_html(fig_b, None, "fig cover", eager=True) if fig_b else ""
            head = [self.heading_html(b, "h1" if b.role == "title" else "p", f"r-{b.role}") for b in page.blocks if b.kind == "story" and b.role in ("kicker", "title", "dek")][:4]
            if not any(b.role == "title" for b in page.blocks):
                head.insert(0, f'<h1 class="r-title">{escape(self.site.title)}</h1>')
            cover = f'<section class="hero home-cover{" has-image" if fig else ""}"><div class="hero-text">{"".join(head)}</div>{fig}</section>'
        else:
            desc = self.description()
            cover = f'<section class="hero home-cover"><div class="hero-text"><h1 class="r-title">{escape(self.site.title)}</h1>' + (f'<p class="r-dek">{escape(desc)}</p>' if desc else "") + "</div></section>"
        edition = ""
        if self.o.get("_edition"):
            edition = f'<p class="edition-link"><a class="btn ghost" href="edition.html">Read the {len(self.c.get("pages", []))}-page edition</a></p>'
        return f'{cover}\n<section class="cards">{"".join(cards)}</section>\n{edition}'

    def description(self) -> str:
        d = (self.o.get("description") or "").strip()
        if d:
            return d
        if self.pack.get("subtitle"):
            return str(self.pack["subtitle"]).strip()
        return next((b.text.replace("\n", " ") for b in self.blocks if b.role in ("dek", "lead") and b.text), "")[:200]

    def brand_html(self) -> str:
        logo = next((b for b in self.blocks if b.kind == "image" and b.role == "brand"), None)
        info = self.images.get(logo.raw.get("asset")) if logo else None
        name = escape(self.site.title)
        if info and info.get("src"):
            return f'<a class="brand" href="index.html"><img src="{escape(info["src"])}" alt="{name}" height="32"></a>'
        return f'<a class="brand" href="index.html">{name}</a>'

    def footer_html(self) -> str:
        seen, parts = set(), []
        for b in self.blocks:
            if b.role == "footer" and b.text and b.text not in seen:
                seen.add(b.text)
                parts.append(f"<p>{self.autolink(self.runs_html_join(b))}</p>")
        year = datetime.now().year
        if not parts:
            parts.append(f"<p>{escape(self.site.title)} · {year}</p>")
        return "".join(parts)

    def nav_html(self, current: WebPage | None) -> str:
        here = ' aria-current="page"'
        links = [(p.href, p.title[:48], p is current) for p in self.site.pages if p.slug != "index"][:8]
        if self.o.get("_edition"):
            links.append(("edition.html", "Edition", current is None))
        items_html = "".join(f'<li><a href="{escape(h)}"{here if on else ""}>{escape(t)}</a></li>' for h, t, on in links)
        if not items_html:
            return ""
        return f'<details class="menu"><summary aria-label="Menu"><span></span></summary><ul>{items_html}</ul></details><ul class="nav-links">{items_html}</ul>'

    def pager_html(self, page: WebPage) -> str:
        arts = [p for p in self.site.pages if p.kind == page.kind and p.slug != "index"]
        if page not in arts:
            return ""
        i = arts.index(page)
        if len(arts) < 2:
            return ""
        prev =f'<a class="prev" href="{escape(arts[i - 1].href)}"><span>Previous</span>{escape(arts[i - 1].title)}</a>' if i > 0 else "<span></span>"
        nxt = f'<a class="next" href="{escape(arts[i + 1].href)}"><span>Next</span>{escape(arts[i + 1].title)}</a>' if i + 1 < len(arts) else "<span></span>"
        return f'<nav class="pager" aria-label="More">{prev}{nxt}</nav>'

    def head_html(self, title: str, desc: str, path: str, image: str | None, extra: str = "") -> str:
        base = (self.o.get("baseUrl") or "").rstrip("/")
        full_title = title if title == self.site.title else f"{title} · {self.site.title}"
        canon = f'<link rel="canonical" href="{escape(base)}/{"" if path == "index.html" else escape(path)}">' if base else ""
        img = ""
        if image:
            img = f'<meta property="og:image" content="{escape((base + "/" if base else "") + image)}">'
        feed = '<link rel="alternate" type="application/rss+xml" title="Feed" href="feed.xml">' if base and self.site.split == "articles" else ""
        return (f'<!doctype html>\n<html lang="{escape(self.lang)}">\n<head>\n<meta charset="utf-8">\n<meta name="viewport" content="width=device-width, initial-scale=1">\n'
                f"<title>{escape(full_title)}</title>\n" + (f'<meta name="description" content="{escape(desc[:300])}">\n' if desc else "")
                + f'<meta property="og:title" content="{escape(title)}">\n' + (f'<meta property="og:description" content="{escape(desc[:300])}">\n' if desc else "")
                + f'<meta property="og:type" content="{"article" if extra else "website"}">\n{img}\n{canon}\n{feed}\n'
                f'<meta name="generator" content="FormDesign {__version__}">\n<link rel="stylesheet" href="assets/site.css">\n{extra}</head>\n')

    def chrome(self, page: WebPage | None, title: str, desc: str, path: str, main: str, image: str | None = None, extra: str = "", body_cls: str = "") -> str:
        cls = f"layout-{self.layout} kind-{page.kind if page else 'edition'} {body_cls}".strip()
        return (self.head_html(title, desc, path, image, extra)
                + f'<body class="{cls}">\n<a class="skip" href="#main">Skip to content</a>\n<header class="site-head"><div class="wrap">{self.brand_html()}<nav aria-label="Site">{self.nav_html(page)}</nav></div></header>\n'
                f'<main id="main" class="wrap">\n{main}\n</main>\n' + (self.pager_html(page) if page else "")
                + f'<footer class="site-foot"><div class="wrap">{self.footer_html()}</div></footer>\n</body>\n</html>\n')

    def jsonld(self, page: WebPage, image: str | None) -> str:
        base = (self.o.get("baseUrl") or "").rstrip("/")
        d = {"@context": "https://schema.org", "@type": "Article", "headline": page.title[:110]}
        if page.dek:
            d["description"] = page.dek[:300]
        author = page.byline.removeprefix("By ").removeprefix("by ").strip() or self.o.get("author") or self.pack.get("author")
        if author:
            d["author"] = {"@type": "Person", "name": author}
        if image and base:
            d["image"] = f"{base}/{image}"
        return f'<script type="application/ld+json">{motion.safe_json(d)}</script>\n'

    async def edition(self) -> None:
        pages = self.c.get("pages", [])
        if not pages:
            return
        spreads: dict[int, list[str]] = {}
        names: dict[int, list[str]] = {}
        hot_by_page: dict[int, list[dict]] = {}
        for h in self.c.get("hotspots", []):
            hot_by_page.setdefault(int(h["page"]), []).append(h)
        text_by_page: dict[int, list[str]] = {}
        for b in self.blocks:
            if b.kind == "story" and b.text:
                text_by_page.setdefault(b.page, []).append(b.text)
        motion_cfg = await motion.export_config(self.meta, "page") if self.meta is not None and self.o.get("animations", True) else None
        motion_geo = await motion.geometry() if motion_cfg else {}
        for p in pages:
            i = int(p["index"])
            scale = min(3.0, PAGE_PX / max(1.0, float(p["width"])))
            ids = sorted(int(k) for k, a in (motion_cfg or {}).get("anims", {}).items() if motion_geo.get(int(k), {}).get("page") == i)
            overlays = ""
            try:
                if ids:
                    plate, shots = await motion.page_layers(i, ids, scale)
                    r = type("R", (), {"bin": plate})()
                    for k in ids:
                        g = motion_geo[k]
                        if k in shots:
                            self.site.add(f"assets/pages/page-{i + 1}-a{k}.png", shots[k])
                            overlays += (f'<img class="ed-an" data-fd="{k}" src="assets/pages/page-{i + 1}-a{k}.png" alt="" style="left:{g["x"] / g["pageW"] * 100:.3f}%;top:{g["y"] / g["pageH"] * 100:.3f}%;'
                                         f'width:{g["w"] / g["pageW"] * 100:.3f}%;height:{g["h"] / g["pageH"] * 100:.3f}%">')
                else:
                    r = await core.call("page.png", {"page": i, "scale": scale, "format": "png"})
            except CoreError as exc:
                self.site.warnings.append(f"page {p['name']}: {exc}")
                continue
            im = await asyncio.to_thread(lambda data: Image.open(io.BytesIO(data)).convert("RGB"), r.bin or b"")
            buf = io.BytesIO()
            await asyncio.to_thread(im.save, buf, "WEBP", quality=80, method=4)
            path = f"assets/pages/page-{i + 1}.webp"
            self.site.add(path, buf.getvalue())
            hots = ""
            for h in hot_by_page.get(i, []):
                x0, y0, x1, y1 = h["bounds"]
                px, pw, ph = float(p["x"]), float(p["width"]), float(p["height"])
                left, top = (x0 - px) / pw * 100, y0 / ph * 100
                wd, ht = (x1 - x0) / pw * 100, (y1 - y0) / ph * 100
                hots += f'<a class="ed-hot" href="{escape(self.href(h["href"]))}" style="left:{left:.2f}%;top:{top:.2f}%;width:{wd:.2f}%;height:{ht:.2f}%" aria-label="Link"></a>'
            text = "".join(f"<p>{escape(t)}</p>" for t in text_by_page.get(i, []))
            load = "" if i < 2 else ' loading="lazy"'
            sp = int(p.get("spread", i))
            spreads.setdefault(sp, []).append(f'<figure class="ed-page" id="p{i + 1}"><img src="{path}" width="{im.size[0]}" height="{im.size[1]}" alt="Page {escape(p["name"])}" decoding="async"{load}>{overlays}{hots}<div class="ed-text">{text}</div></figure>')
            names.setdefault(sp, []).append(str(p["name"]))
        facing = " facing" if self.c.get("facing") else ""
        rows = []
        for sp, figs in spreads.items():
            label = f"Page {names[sp][0]}" if len(names[sp]) == 1 else f"Pages {names[sp][0]}–{names[sp][-1]}"
            rows.append(f'<div class="ed-spread{" single" if len(figs) == 1 else ""}" data-label="{escape(label)}">{"".join(figs)}</div>')
        total = len(pages)
        script = ("<script>(()=>{const f=[...document.querySelectorAll('.ed-spread')],o=document.querySelector('.ed-count');let c=0;"
                  f"const io=new IntersectionObserver(e=>{{for(const x of e)if(x.isIntersecting){{c=f.indexOf(x.target);o.textContent=`${{x.target.dataset.label}} of {total}`}}}},{{threshold:.4}});"
                  "f.forEach(x=>io.observe(x));const go=d=>{const n=f[Math.max(0,Math.min(f.length-1,c+d))];n&&n.scrollIntoView({behavior:'smooth',block:'start'})};"
                  "document.querySelector('.ed-prev').onclick=()=>go(-1);document.querySelector('.ed-next').onclick=()=>go(1);"
                  "addEventListener('keydown',e=>{if(e.key==='ArrowRight'||e.key==='PageDown'){e.preventDefault();go(1)}if(e.key==='ArrowLeft'||e.key==='PageUp'){e.preventDefault();go(-1)}})})()</script>")
        first_label = next(iter(names.values()), ["1"])
        first_text = f"Page {first_label[0]}" if len(first_label) == 1 else f"Pages {first_label[0]}–{first_label[-1]}"
        main = (f'<div class="ed-bar"><button class="ed-prev" type="button" aria-label="Previous">&#8592;</button><span class="ed-count" aria-live="polite">{escape(first_text)} of {total}</span>'
                f'<button class="ed-next" type="button" aria-label="Next">&#8594;</button></div>\n<div class="ed-pages{facing}">{"".join(rows)}</div>\n{script}')
        if motion_cfg and "ed-an" in main:
            main += motion.page_script({**motion_cfg, "mode": "flow"})
        self.site.add("edition.html", self.chrome(None, "Edition", self.description(), "edition.html", main, "assets/pages/page-1.webp", body_cls="edition"))

    async def fonts_css(self) -> str:
        wanted: set[tuple[str, str]] = set(self.fonts_used)
        for name, sp in self.styles.items():
            if name not in self.used:
                continue
            wanted.add((sp.get("font") or "", sp.get("fontStyle") or "Regular"))
            fam = sp.get("font") or ""
            wanted.add((fam, "Bold"))
            wanted.add((fam, "Italic"))
        wanted = {w for w in wanted if w[0]}
        lic_dir = config.FONTS_DIR / "licenses"
        ofl_body = ""
        for f in sorted(lic_dir.glob("OFL-*.txt")) if lic_dir.is_dir() else []:
            t = f.read_text(errors="ignore")
            k = t.find("This Font Software is licensed under the SIL Open Font License")
            if k >= 0:
                ofl_body = t[k:]
                break
        faces, seen, skipped, licences = [], {}, set(), {}
        for fam, style in sorted(wanted):
            try:
                r = await core.call("fonts.data", {"family": fam, "style": style})
            except CoreError:
                skipped.add(fam)
                continue
            data = r.bin or b""
            if not data or r.result.get("index", 0) != 0:
                skipped.add(fam)
                continue
            h = hashlib.sha1(data).hexdigest()[:12]
            info = await asyncio.to_thread(font_info, data)
            bundled = lic_dir / f"OFL-{slugify(fam)}.txt"
            if bundled.is_file():
                licence = bundled.read_text(errors="ignore")
            elif info["ofl"] and ofl_body:
                licence = f"{info['copyright']}\n\n{ofl_body}"
            elif info["apache"]:
                licence = f"{info['copyright']}\n\n{info['licence']}\n{info['url']}\n"
            else:
                skipped.add(fam)
                continue
            licences[fam] = licence
            weight = info["weights"] or str(weight_of(style))
            italic = "italic" if "italic" in style.lower() or "oblique" in style.lower() else "normal"
            key = (fam, weight, italic)
            if key in seen or (not info["weights"] and h in {v for k, v in seen.items() if k[0] == fam}):
                continue
            fname = f"assets/fonts/{slugify(fam)}-{h}.ttf"
            if fname not in self.site.files:
                self.site.add(fname, data)
            seen[key] = h
            faces.append(f'@font-face{{font-family:"{fam}";src:url("fonts/{Path(fname).name}") format("truetype");font-weight:{weight};font-style:{italic};font-display:swap}}')
        for fam, text in licences.items():
            self.site.add(f"assets/fonts/LICENSE-{slugify(fam)}.txt", text)
        for fam in sorted(skipped):
            self.site.warnings.append(f"{fam} is not embedded (its licence does not allow it or is unknown); the site falls back to a system font")
        return "\n".join(dict.fromkeys(faces))

    def style_css(self) -> str:
        rules = []
        body = self.styles.get(self.body_style, {})
        bpt = self.body_pt or 10
        for name, sp in self.styles.items():
            if name not in self.used:
                continue
            size = float(sp.get("size") or bpt)
            rel = size / bpt
            decl = [f"font-family:{stack(sp.get('font') or 'serif')}", f"font-weight:{int(sp.get('weight') or 400)}"]
            if sp.get("italic"):
                decl.append("font-style:italic")
            if abs(rel - 1) > 0.04:
                mx = min(5.5, max(0.7, rel)) * 1.0
                if mx > 1.5:
                    mn = max(1.2, mx * 0.6)
                    k = (mx - mn) / 67.5
                    decl.append(f"font-size:clamp({mn:.3f}rem, calc({mn - 22.5 * k:.3f}rem + {k * 100:.3f}vw), {mx:.3f}rem)")
                else:
                    decl.append(f"font-size:{mx:.3f}rem")
            lead = float(sp.get("leading") or size * 1.2) / max(1.0, size)
            decl.append(f"line-height:{min(1.9, max(1.0 if rel > 1.5 else 1.3, lead)):.3f}")
            tr = float(sp.get("tracking") or 0)
            if tr:
                decl.append(f"letter-spacing:{tr / 1000:.3f}em")
            if sp.get("caps") == "all":
                decl.append("text-transform:uppercase")
            elif sp.get("caps") == "small":
                decl.append("font-variant:small-caps")
            col = sp.get("color")
            if col and col.lower() != self.ink.lower() and contrast(col, self.paper) >= 2.2:
                decl.append(f"color:{col}")
            if sp.get("align") in ("center", "right"):
                decl.append(f"text-align:{sp['align']}")
            sa, sb = float(sp.get("spaceAfter") or 0), float(sp.get("spaceBefore") or 0)
            fi = float(sp.get("firstIndent") or 0)
            if name == self.body_style or sp.get("font") == body.get("font") and abs(rel - 1) < 0.1:
                if fi > 0 and sa == 0:
                    decl.append("margin:0")
                    rules.append(f".{css_class(name)}+.{css_class(name)}{{text-indent:{min(3.0, fi / size):.2f}em}}")
                else:
                    decl.append(f"margin:{min(1.5, sb / size):.2f}em 0 {max(0.8, min(1.5, sa / size)):.2f}em")
            else:
                decl.append(f"margin:{min(1.2, sb / size):.2f}em 0 {min(1.0, max(0.25, sa / size)):.2f}em")
            if sp.get("underline"):
                decl.append("text-decoration:underline")
            rules.append(f".{css_class(name)}{{{';'.join(decl)}}}")
            if sp.get("dropCap"):
                lines = int(sp["dropCap"])
                rules.append(f".{css_class(name)}.dropcap::first-letter{{float:left;font-size:{lines * 1.08:.2f}em;line-height:.86;margin:.06em .08em 0 0;font-weight:700;color:var(--accent)}}")
        for name, cs in self.cstyles.items():
            decl = []
            if cs.get("weight") and int(cs["weight"]) >= 600:
                decl.append(f"font-weight:{int(cs['weight'])}")
            if cs.get("italic"):
                decl.append("font-style:italic")
            if cs.get("color") and contrast(cs["color"], self.paper) >= 2.2:
                decl.append(f"color:{cs['color']}")
            if cs.get("caps") == "all":
                decl.append("text-transform:uppercase")
            if cs.get("font"):
                decl.append(f"font-family:{stack(cs['font'])}")
            if decl:
                rules.append(f".cs-{slugify(name, fallback='char')}{{{';'.join(decl)}}}")
        return "\n".join(rules)

    def site_css(self, faces: str) -> str:
        body = self.styles.get(self.body_style, {})
        text_font = stack(body.get("font") or "Source Serif 4")
        ts = self.styles.get(self.title_style(), {})
        display_font = stack(ts.get("font") or body.get("font") or "serif")
        maxw = (self.rt.get("web") or {}).get("maxWidth")
        wide = "72rem" if not maxw else f"{min(90, max(40, int(maxw) / 16)):.0f}rem"
        measure = {"email": "37.5rem", "card": "40rem"}.get(self.layout, "42rem")
        return (f"{faces}\n:root{{--ink:{self.ink};--paper:{self.paper};--accent:{self.accent};--muted:{self.muted};--tint:{self.tint};"
                f"--line:color-mix(in srgb,var(--ink) 14%,transparent);--text:{text_font};--display:{display_font};"
                f"--ui:system-ui,-apple-system,'Segoe UI',sans-serif;--fs:1.125rem;--measure:{measure};--wide:{wide};--gap:clamp(1rem,3vw,2rem)}}\n"
                + SITE_CSS + "\n" + self.style_css() + "\n")

    async def build(self) -> Site:
        self.lang = language(self.c.get("language", ""), self.o.get("language"))
        stem_title = Path(self.c.get("title") or "Untitled").stem
        brand_text = next((re.sub(r"\s+", " ", b.text).strip() for b in self.blocks if b.kind == "story" and b.role == "brand" and b.text), "")
        self.site.title = (self.o.get("title") or "").strip() or brand_text[:80] or (self.pack.get("title") or "").strip() or stem_title
        self.site.layout = self.layout
        if self.meta is not None and self.o.get("animations", True):
            self.mo = await motion.export_config(self.meta, "flow")
            if self.mo:
                self.mo_ids = set(self.mo["anims"]) | {str(a["button"]) for a in self.mo["anims"].values() if a.get("button") is not None}
        pages = self.split()
        self.site.pages = pages
        n_pages = len(self.c.get("pages", []))
        want_edition = self.o.get("edition")
        if want_edition is None:
            want_edition = self.c.get("intent") != "web" and n_pages > 1 and self.layout not in ("landing", "email", "card")
        self.o["_edition"] = bool(want_edition)
        for b in self.blocks:
            if b.kind == "image" and b.role == "brand":
                await self.image(b)
                break
        base = (self.o.get("baseUrl") or "").rstrip("/")
        for p in pages:
            self._h1 = False
            self._img_seen = False
            self.mo_used = set()
            main = await self.page_html(p)
            main += self.page_motion()
            img = None
            if p.image:
                info = await self.image(p.image)
                img = info.get("src") if info else None
            extra = self.jsonld(p, img) if p.kind == "article" else ""
            desc = p.dek or self.description()
            self.site.add(p.href, self.chrome(p, p.title, desc, p.href, main, img, extra))
        if self.o["_edition"]:
            await self.edition()
        faces = await self.fonts_css()
        self.site.add("assets/site.css", self.site_css(faces))
        self.site.add("404.html", self.chrome(None, "Not found", "", "404.html", '<article class="post"><header class="post-head"><h1 class="r-title">Page not found</h1><p class="r-dek"><a href="index.html">Back to the start</a></p></header></article>'))
        if base:
            now = datetime.now(timezone.utc)
            urls = [f"{base}/" if p.slug == "index" else f"{base}/{p.href}" for p in pages]
            if self.o["_edition"]:
                urls.append(f"{base}/edition.html")
            self.site.add("sitemap.xml", '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
                          + "".join(f"<url><loc>{escape(u)}</loc><lastmod>{now.date().isoformat()}</lastmod></url>\n" for u in urls) + "</urlset>\n")
            self.site.add("robots.txt", f"User-agent: *\nAllow: /\nSitemap: {base}/sitemap.xml\n")
            if self.site.split == "articles":
                items = "".join(
                    f"<item><title>{escape(p.title)}</title><link>{escape(base)}/{escape(p.href)}</link><guid>{escape(base)}/{escape(p.href)}</guid>"
                    f"<pubDate>{format_datetime(now)}</pubDate>" + (f"<description>{escape(p.dek[:400])}</description>" if p.dek else "") + "</item>\n"
                    for p in pages if p.kind == "article")
                self.site.add("feed.xml", f'<?xml version="1.0" encoding="UTF-8"?>\n<rss version="2.0"><channel><title>{escape(self.site.title)}</title><link>{escape(base)}/</link>'
                              f"<description>{escape(self.description() or self.site.title)}</description><language>{escape(self.lang)}</language>\n{items}</channel></rss>\n")
        if self.placeholders:
            many = self.placeholders > 1
            self.site.warnings.append(f"{self.placeholders} image frame{'s' if many else ''} still {'show' if many else 'shows'} Make's placeholder art: place your own pictures before publishing")
        self.site.warnings = list(dict.fromkeys(self.site.warnings))
        return self.site


def weight_of(style: str) -> int:
    s = style.lower()
    for k, w in (("black", 900), ("heavy", 900), ("extrabold", 800), ("semibold", 600), ("demibold", 600), ("bold", 700), ("medium", 500), ("light", 300), ("thin", 200)):
        if k in s:
            return w
    return 400


def font_info(data: bytes) -> dict:
    from fontTools.ttLib import TTFont
    f = TTFont(io.BytesIO(data), lazy=True)
    name = f["name"]
    lic = name.getDebugName(13) or ""
    url = name.getDebugName(14) or ""
    weights = ""
    if "fvar" in f:
        ax = next((a for a in f["fvar"].axes if a.axisTag == "wght"), None)
        if ax:
            weights = f"{int(ax.minValue)} {int(ax.maxValue)}"
    low = (lic + " " + url).lower()
    return {"copyright": name.getDebugName(0) or "", "licence": lic, "url": url, "weights": weights,
            "ofl": "open font license" in low or "openfontlicense" in low or "scripts.sil.org/ofl" in low, "apache": "apache" in low}


async def build_site(options: dict | None = None) -> Site:
    options = dict(options or {})
    if options.get("split") and options["split"] not in SPLITS:
        raise CoreError(f"unknown split {options['split']}: one of {', '.join(SPLITS)}")
    r = await core.call("web")
    meta = await documents.meta_for_active()
    rt = registry.get(meta.template["id"]) if meta and meta.template and meta.template.get("id") else None
    pack = packs.get(meta.pack_id) if meta and meta.pack_id else None
    return await Builder(r.result, options, meta, rt, pack).build()


def zip_bytes(site: Site) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for path, data in sorted(site.files.items()):
            stored = path.endswith((".webp", ".jpg", ".png", ".gif", ".mp4", ".mp3", ".webm"))
            z.writestr(path, data, compress_type=zipfile.ZIP_STORED if stored else zipfile.ZIP_DEFLATED)
    return buf.getvalue()


SITE_MARKER = ".formdesign-site"


def write_dir(site: Site, target: Path) -> None:
    target = target.expanduser()
    tmp = target.with_name(f".{target.name}.partial")
    if tmp.exists():
        shutil.rmtree(tmp)
    for path, data in site.files.items():
        p = tmp / path
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)
    tmp.mkdir(parents=True, exist_ok=True)
    (tmp / SITE_MARKER).write_text("\n".join(sorted(site.files)), encoding="utf-8")
    if target.exists():
        old = target.with_name(f".{target.name}.old")
        if old.exists():
            shutil.rmtree(old)
        target.rename(old)
        tmp.rename(target)
        shutil.rmtree(old, ignore_errors=True)
    else:
        target.parent.mkdir(parents=True, exist_ok=True)
        tmp.rename(target)


def _only_export_files(p: Path) -> bool:
    try:
        listed = set((p / SITE_MARKER).read_text(encoding="utf-8").splitlines())
    except (OSError, ValueError):
        return False
    for f in p.rglob("*"):
        rel = f.relative_to(p).as_posix()
        if rel == SITE_MARKER or rel in listed or (f.is_dir() and not f.is_symlink() and any(x.startswith(rel + "/") for x in listed)):
            continue
        return False
    return True


async def export(path: str | None, options: dict | None) -> tuple[dict, bytes | None, str]:
    site = await build_site(options)
    info = site.summary()
    if path is None:
        return info, zip_bytes(site), "application/zip"
    p = Path(path).expanduser()
    if p.suffix.lower() == ".zip":
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(zip_bytes(site))
    else:
        if p.exists() and p.is_dir() and any(p.iterdir()) and not _only_export_files(p):
            raise CoreError(f"{p} holds files a website export did not write; choose an empty folder, the folder of an earlier export, or a .zip path")
        write_dir(site, p)
    info["path"] = str(p)
    return info, None, "application/zip"


class Published:
    def __init__(self) -> None:
        self.index = PUBLISHED_DIR / "index.json"

    def entries(self) -> list[dict]:
        try:
            items = json.loads(self.index.read_text())
        except (OSError, ValueError):
            return []
        return [i for i in items if isinstance(i, dict) and (PUBLISHED_DIR / str(i.get("slug", ""))).is_dir()]

    def _save(self, items: list[dict]) -> None:
        PUBLISHED_DIR.mkdir(parents=True, exist_ok=True)
        self.index.write_text(json.dumps(items, indent=1))

    def find(self, slug: str) -> dict | None:
        return next((i for i in self.entries() if i.get("slug") == slug), None)

    @staticmethod
    def belongs(rec: dict, meta) -> bool:
        if meta is None:
            return False
        if meta.path:
            return rec.get("doc") == meta.path
        return rec.get("session") == SESSION and rec.get("uid") == meta.uid

    async def active_slug(self) -> str | None:
        meta = await documents.meta_for_active()
        return next((i["slug"] for i in self.entries() if self.belongs(i, meta)), None)

    def dir(self, slug: str) -> Path:
        safe = slugify(slug, fallback="site")
        return PUBLISHED_DIR / safe

    async def publish(self, options: dict | None = None, slug: str | None = None) -> dict:
        options = dict(options or {})
        meta = await documents.meta_for_active()
        if meta is None:
            raise CoreError("no document open")
        items = self.entries()
        rec = None
        if slug:
            rec = next((i for i in items if i.get("slug") == slug), None)
        else:
            rec = next((i for i in items if self.belongs(i, meta)), None)
        if rec and not options:
            options = dict(rec.get("options") or {})
        site = await build_site(options)
        if rec is None:
            used = {i.get("slug") for i in items}
            slug = slugify(slug or site.title, set(used), fallback="site")
            rec = {"slug": slug, "created": time.time()}
            items.append(rec)
        write_dir(site, self.dir(rec["slug"]))
        rec.update({"title": site.title, "doc": meta.path or rec.get("doc"), "uid": meta.uid, "session": SESSION, "updated": time.time(), "options": {k: v for k, v in options.items() if not k.startswith("_")},
                    "files": len(site.files), "bytes": sum(len(v) for v in site.files.values()), "pages": len(site.pages), "split": site.split,
                    "warnings": site.warnings})
        self._save(items)
        return {**rec, "path": str(self.dir(rec["slug"]))}

    def unpublish(self, slug: str) -> bool:
        items = self.entries()
        rec = next((i for i in items if i.get("slug") == slug), None)
        if rec is None:
            return False
        shutil.rmtree(self.dir(slug), ignore_errors=True)
        self._save([i for i in items if i is not rec])
        return True

    def zip(self, slug: str) -> bytes:
        root = self.dir(slug)
        if not root.is_dir():
            raise CoreError(f"no published site {slug}")
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
            for f in sorted(root.rglob("*")):
                if f.is_file():
                    z.write(f, f.relative_to(root).as_posix())
        return buf.getvalue()


published = Published()

SITE_CSS = """
*,*::before,*::after{box-sizing:border-box}
html{-webkit-text-size-adjust:100%;text-size-adjust:100%}
body{margin:0;background:var(--paper);color:var(--ink);font-family:var(--text);font-size:var(--fs);line-height:1.6;text-rendering:optimizeLegibility;-webkit-font-smoothing:antialiased}
img,video{display:block;max-width:100%;height:auto}
a{color:inherit;text-decoration-color:color-mix(in srgb,var(--accent) 70%,transparent);text-underline-offset:.18em}
a:hover{color:var(--accent)}
.skip{position:absolute;left:-999px;top:0;background:var(--ink);color:var(--paper);padding:.5rem 1rem;z-index:10}
.skip:focus{left:1rem}
.wrap{width:min(100% - 2*var(--gap),var(--wide));margin-inline:auto}
.site-head{border-bottom:1px solid var(--line);font-family:var(--ui);position:sticky;top:0;background:color-mix(in srgb,var(--paper) 92%,transparent);backdrop-filter:blur(8px);z-index:5}
.site-head .wrap{display:flex;align-items:center;justify-content:space-between;gap:1rem;min-height:3.5rem}
.brand{font-family:var(--display);font-weight:700;font-size:1.15rem;text-decoration:none;letter-spacing:-.01em}
.brand img{height:2rem;width:auto}
.site-head nav{position:relative}
.nav-links{display:flex;gap:1.25rem;list-style:none;margin:0;padding:0;font-size:.9rem}
.nav-links a,.menu a{text-decoration:none;color:var(--muted)}
.nav-links a:hover,.nav-links a[aria-current],.menu a[aria-current]{color:var(--ink)}
.menu{display:none}
.menu summary{list-style:none;cursor:pointer;width:2.5rem;height:2.5rem;display:grid;place-items:center}
.menu summary::-webkit-details-marker{display:none}
.menu summary span,.menu summary span::before,.menu summary span::after{display:block;width:1.2rem;height:2px;background:var(--ink);position:relative;content:""}
.menu summary span::before{position:absolute;top:-6px}.menu summary span::after{position:absolute;top:6px}
.menu ul{position:absolute;right:0;top:3rem;list-style:none;margin:0;padding:.75rem 1rem;background:var(--paper);border:1px solid var(--line);border-radius:.5rem;min-width:14rem;box-shadow:0 12px 32px rgba(0,0,0,.12)}
.menu li{padding:.35rem 0}
@media (max-width:48rem){.nav-links{display:none}.menu{display:block}}
main{padding-block:clamp(1.5rem,5vw,4rem)}
.post{max-width:var(--measure);margin-inline:auto}
.post-head{margin-bottom:clamp(1.5rem,4vw,2.5rem)}
.post .post-head>*{margin-block:0 .5rem}
.post .post-head>.r-title{margin-bottom:.9rem}
.r-kicker{color:var(--accent);font-family:var(--ui);font-weight:700;font-size:.8rem;letter-spacing:.08em;text-transform:uppercase}
.r-title{text-wrap:balance}
.r-dek{color:var(--muted);text-wrap:pretty}
.r-byline,.r-meta{font-family:var(--ui);font-size:.85rem;color:var(--muted)}
.readtime{margin-top:.75rem}
.fig{margin:clamp(1.5rem,4vw,2.5rem) 0}
.fig img,.fig video{width:100%;object-fit:cover;border-radius:2px;background:var(--tint)}
.fig figcaption{font-family:var(--ui);font-size:.8rem;line-height:1.45;color:var(--muted);margin-top:.6rem}
.fig figcaption p{margin:0}
.post .fig.feature{width:min(100vw - 2*var(--gap),var(--wide));margin-inline:calc((var(--measure) - min(100vw - 2*var(--gap),var(--wide)))/2)}
.post .fig.feature img{aspect-ratio:max(var(--ar),1.25)}
@media (min-width:56rem){.post .fig.feature img{aspect-ratio:max(var(--ar),1.9)}}
@media (max-width:48rem){.post .fig.feature{width:100%;margin-inline:0}}
.fig.cover img{aspect-ratio:clamp(.75,var(--ar),2)}
.hero .fig img{aspect-ratio:clamp(.75,var(--ar),2);max-height:80vh}
.post-body .story>:first-child{margin-top:0}
.post-body p,.sections p{hyphens:auto;-webkit-hyphens:auto;overflow-wrap:break-word}
.post-body h2,.post-body h3,.post-body h4,.sections h2,.sections h3{line-height:1.2;margin:1.8em 0 .5em;text-wrap:balance}
blockquote{margin:1.5em 0;padding-left:1.25em;border-left:3px solid var(--accent)}
.pullquote{margin:2.5rem 0;padding:1.5rem 0;border-block:1px solid var(--line)}
.pullquote blockquote{border:0;padding:0;margin:0;font-family:var(--display);font-size:clamp(1.35rem,1rem + 1.5vw,1.9rem);line-height:1.25;text-wrap:balance}
.pullquote blockquote p{font:inherit;margin:0;color:inherit}
.pullquote cite{display:block;margin-top:.75rem;font-family:var(--ui);font-style:normal;font-size:.85rem;color:var(--muted)}
.stats{display:grid;grid-template-columns:repeat(auto-fit,minmax(10rem,1fr));gap:1rem;margin:2rem 0}
.stat{padding:1.25rem;background:var(--tint);border-radius:.5rem}
.stat strong{display:block;font-family:var(--display);font-size:clamp(1.8rem,1.2rem + 2vw,2.6rem);line-height:1.05;color:var(--accent);margin:0}
.stat span{display:block;font-family:var(--ui);font-size:.85rem;color:var(--muted);margin-top:.35rem}
.cta{margin:1.5rem 0}
.btn{display:inline-block;padding:.8rem 1.4rem;border-radius:999px;background:var(--accent);color:var(--paper);text-decoration:none;font-family:var(--ui);font-weight:600;font-size:.95rem}
.btn:hover{color:var(--paper);filter:brightness(1.08)}
.btn.ghost{background:transparent;color:var(--ink);box-shadow:inset 0 0 0 1.5px var(--line)}
.btn.ghost:hover{color:var(--accent)}
.hero{display:grid;gap:clamp(1.5rem,4vw,3rem);align-items:center;margin-bottom:clamp(2rem,6vw,4rem)}
.hero.has-image{grid-template-columns:1fr}
@media (min-width:56rem){.hero.has-image{grid-template-columns:minmax(0,1fr) minmax(0,1.1fr)}}
.hero .fig{margin:0}
.hero .hero-text>*{margin-block:0 .6rem}
.hero-text .r-title{margin-bottom:.8rem}
.sections{max-width:var(--wide);margin-inline:auto}
.sections>.story,.sections>.stats,.sections>.cta,.sections>.r-title,.sections>.r-kicker,.sections>.r-dek{max-width:var(--measure)}
.layout-landing .sections>.story{margin-bottom:2rem}
.row{display:grid;grid-template-columns:var(--cols);gap:clamp(1.25rem,4vw,3rem);align-items:start;margin-bottom:clamp(1.5rem,4vw,3rem)}
.row .col>:first-child,.row .col>.story>:first-child{margin-top:0}
.row .fig{margin:0 0 1rem}
.row .fig,.gallery .fig{max-width:none!important}
@media (max-width:48rem){.row{grid-template-columns:1fr}}
.sections .fig img{max-height:80vh}
.gallery{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,14rem),1fr));gap:1rem;margin:1.5rem 0}
.gallery .fig{margin:0}
.visually-hidden{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
.cards{display:grid;grid-template-columns:repeat(auto-fill,minmax(min(100%,19rem),1fr));gap:clamp(1.25rem,3vw,2rem)}
.card{display:flex;flex-direction:column;text-decoration:none;color:inherit;border-radius:.6rem;overflow:hidden;background:var(--paper);box-shadow:0 0 0 1px var(--line);transition:box-shadow .2s,transform .2s}
.card:hover{color:inherit;box-shadow:0 0 0 1px var(--line),0 14px 36px rgba(0,0,0,.1);transform:translateY(-2px)}
.card-img{aspect-ratio:3/2;background:var(--tint);overflow:hidden}
.card-img img{width:100%;height:100%;object-fit:cover}
.card-text{padding:1rem 1.15rem 1.25rem}
.card-text>*{margin:0 0 .4rem}
.card h2{font-family:var(--display);font-size:1.3rem;line-height:1.2;text-wrap:balance}
.card-kicker{font-family:var(--ui);font-size:.72rem;font-weight:700;letter-spacing:.08em;text-transform:uppercase;color:var(--accent)}
.card-dek{color:var(--muted);font-size:.95rem;line-height:1.45}
.card-meta{font-family:var(--ui);font-size:.78rem;color:var(--muted)}
.edition-link{margin:3rem 0 0;text-align:center}
.pager{width:min(100% - 2*var(--gap),var(--measure));margin:0 auto 3rem;display:grid;grid-template-columns:1fr 1fr;gap:1rem;border-top:1px solid var(--line);padding-top:1.5rem;font-family:var(--ui)}
.pager a{text-decoration:none;font-weight:600;line-height:1.3}
.pager span{display:block;font-weight:400;font-size:.78rem;color:var(--muted);margin-bottom:.2rem}
.pager .next{text-align:right}
.site-foot{border-top:1px solid var(--line);font-family:var(--ui);font-size:.85rem;color:var(--muted);padding-block:2rem}
.site-foot p{margin:.25rem 0}
.layout-card main{display:grid;place-items:center}
.layout-card .sections{max-width:var(--measure);padding:clamp(1.5rem,5vw,3rem);border-radius:1rem;box-shadow:0 0 0 1px var(--line),0 24px 60px rgba(0,0,0,.08);text-align:center}
.layout-email .wrap{--wide:37.5rem}
.layout-docs main.wrap{max-width:var(--wide)}
.edition main{padding-top:1rem}
.ed-bar{position:sticky;top:3.6rem;z-index:4;display:flex;align-items:center;justify-content:center;gap:1rem;margin:0 auto 1.25rem;padding:.4rem .5rem;width:max-content;border-radius:999px;background:color-mix(in srgb,var(--paper) 90%,transparent);box-shadow:0 0 0 1px var(--line);font-family:var(--ui);font-size:.85rem}
.ed-bar button{all:unset;cursor:pointer;width:2rem;height:2rem;display:grid;place-items:center;border-radius:50%}
.ed-bar button:hover{background:var(--tint)}
.ed-bar button:focus-visible{outline:2px solid var(--accent)}
.ed-pages{display:grid;gap:clamp(1rem,3vw,2.5rem);max-width:60rem;margin:0 auto}
.ed-spread{display:flex;flex-direction:column;align-items:center;gap:clamp(1rem,3vw,2rem);scroll-margin-top:7rem}
@media (min-width:64rem){.ed-pages.facing{max-width:none}.facing .ed-spread{flex-direction:row;justify-content:center;align-items:flex-start;gap:0}.facing .ed-page{width:50%;max-width:44rem}}
.ed-page{position:relative;margin:0;width:100%;box-shadow:0 2px 18px rgba(0,0,0,.14);background:#fff}
.ed-page img{width:100%}
.ed-hot{position:absolute;display:block}
.ed-page img.ed-an{position:absolute;display:block;margin:0;max-width:none}
.ed-page:has(.ed-an){overflow:hidden}
.fd-mo{display:block}
.ed-hot:hover{outline:2px solid var(--accent)}
.ed-text{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
@media (prefers-reduced-motion:reduce){*{transition:none!important;scroll-behavior:auto!important}}
@media print{.site-head,.site-foot,.pager,.ed-bar{display:none}}
"""
