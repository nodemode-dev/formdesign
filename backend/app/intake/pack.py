import re
import shutil
import time
import zipfile
from collections import Counter
from pathlib import Path

from app.intake import blocks as B
from app.intake import brief as brief_mod
from app.intake import images as I
from app.intake import markdown as md
from app.intake import office
from app.intake.store import new_pack_id, packs
from app.services import pdfread

TEXT_EXTS = {".md", ".markdown", ".mdx", ".txt", ".text", ".rtf", ".docx", ".html", ".htm", ".pdf"}
SHEET_EXTS = {".csv", ".tsv", ".xlsx", ".xlsm"}
BRIEF_NAMES = re.compile(r"(^|[-_ .])(brief|prompt|instructions|context|notes?)([-_ .]|$)", re.I)
SKIP = re.compile(r"(^\.|__MACOSX|\.DS_Store|Thumbs\.db)")
GENERATOR_NAMES = {"python-docx", "microsoft office user", "author", "admin", "user", "owner", "unknown", "formdesign"}


class Intake:
    def __init__(self, pack_id: str, root: Path):
        self.id = pack_id
        self.root = root
        self.files_dir = root / "files"
        self.img_dir = root / "images"
        self.files_dir.mkdir(parents=True, exist_ok=True)
        self.img_dir.mkdir(parents=True, exist_ok=True)
        self.images: list[dict] = []
        self.by_name: dict[str, str] = {}
        self.articles: list[dict] = []
        self.tables: list[dict] = []
        self.assets: list[dict] = []
        self.files: list[dict] = []
        self.warnings: list[str] = []
        self.brief_parts: list[str] = []
        self.meta_seen: dict = {}

    def add_image_file(self, path: Path, source: str, caption: str = "") -> str | None:
        try:
            norm = I.normalize(path, self.img_dir)
            ref = f"img{len(self.images) + 1}"
            d = I.describe(norm, ref, source)
            if norm != path:
                d["original"] = str(path)
            if caption:
                d["caption"] = caption
            if path.suffix.lower() == ".gif" and d["animated"]:
                d["webSource"] = str(path)
            self.images.append(d)
            self.by_name[path.name.lower()] = ref
            self.by_name[path.stem.lower()] = ref
            return ref
        except Exception as exc:
            self.warnings.append(f"{path.name}: not a readable image ({exc.__class__.__name__})")
            return None

    def add_image_bytes(self, data: bytes, stem: str, source: str) -> str | None:
        try:
            p = I.save_bytes(data, self.img_dir / stem)
        except Exception:
            self.warnings.append(f"{stem}: an embedded image could not be read")
            return None
        return self.add_image_file(p, source)

    def resolver(self, base: Path):
        def resolve(src: str, alt: str) -> str | None:
            if not src or re.match(r"^(https?:|data:)", src):
                if src.startswith("http"):
                    self.warnings.append(f"{src[:80]}: remote images are not fetched; upload the file")
                return None
            name = Path(src.split("?")[0]).name.lower()
            if name in self.by_name:
                ref = self.by_name[name]
            else:
                cand = (base / src).resolve()
                ref = self.add_image_file(cand, f"ref:{src}") if cand.is_file() else None
            if ref and alt:
                img = next(i for i in self.images if i["id"] == ref)
                img.setdefault("alt", alt)
            return ref
        return resolve

    def add_article(self, meta: dict, blocks: list[dict], source: str) -> None:
        if not blocks and not meta.get("title"):
            return
        aid = f"a{len(self.articles) + 1}"
        art = B.article_from_blocks(blocks, source, aid, meta)
        self.articles.append(art)
        for k in ("title", "subtitle", "author", "date"):
            if meta.get(k):
                self.meta_seen.setdefault(k, meta[k])

    def ingest_file(self, path: Path) -> None:
        ext = path.suffix.lower()
        size = path.stat().st_size
        rec = {"name": path.name, "bytes": size, "kind": ext.lstrip(".")}
        self.files.append(rec)
        if ext in I.IMAGE_EXTS:
            rec["kind"] = "image"
            return
        if ext == ".svg":
            rec["kind"] = "vector"
            self.assets.append({"id": f"v{len(self.assets) + 1}", "name": path.name, "path": str(path), "mime": "image/svg+xml"})
            return
        if ext in SHEET_EXTS:
            rec["kind"] = "table"
            for t in office.read_sheet(path):
                self.tables.append({"id": f"t{len(self.tables) + 1}", "source": path.name, **t, "header": True})
            return
        if ext in TEXT_EXTS and BRIEF_NAMES.search(path.stem) and ext in (".txt", ".md", ".markdown"):
            rec["kind"] = "brief"
            self.brief_parts.append(path.read_text(encoding="utf-8", errors="replace"))
            return
        if ext in (".md", ".markdown", ".mdx"):
            meta, blocks = md.read(path, path.name, self.resolver(path.parent))
        elif ext in (".txt", ".text"):
            meta, blocks = office.read_text(path)
        elif ext == ".rtf":
            meta, blocks = office.read_rtf(path)
        elif ext == ".docx":
            meta, blocks = office.read_docx(path, self.img_dir, self.add_image_bytes)
        elif ext in (".html", ".htm"):
            meta, blocks = office.read_html(path, self.resolver(path.parent))
        elif ext == ".pdf":
            info = pdfread.info(str(path))
            rec["pages"] = info["pages"]
            st = pdfread.structure(str(path))
            words = B.block_words(st["blocks"])
            if words < 40 * max(1, info["pages"]) and info["pages"] <= 4:
                rec["kind"] = "graphic"
                self.assets.append({"id": f"v{len(self.assets) + 1}", "name": path.name, "path": str(path), "mime": "application/pdf", "pages": info["pages"]})
                return
            found = pdfread.extract_images(str(path), self.img_dir / path.stem)
            refs_by_page: dict[int, list[str]] = {}
            for f in found:
                ref = self.add_image_file(Path(f["path"]), f"pdf:{path.name}:p{f['page'] + 1}")
                if ref:
                    refs_by_page.setdefault(f["page"], []).append(ref)
            meta = {k: v for k, v in {"title": st["meta"].get("title"), "author": st["meta"].get("author"), "subtitle": st["meta"].get("subject")}.items() if v}
            blocks = st["blocks"]
            for refs in refs_by_page.values():
                for ref in refs:
                    blocks.append(B.image(ref))
        else:
            rec["kind"] = "other"
            self.warnings.append(f"{path.name}: {ext or 'this file type'} is not read; send Markdown, text, Word, RTF, HTML, PDF, CSV, Excel or images")
            return
        self.add_article(meta, blocks, path.name)

    def expand(self, paths: list[Path]) -> list[Path]:
        out: list[Path] = []
        for p in paths:
            if SKIP.search(p.name):
                continue
            if p.is_dir():
                out.extend(self.expand(sorted(p.iterdir())))
            elif p.suffix.lower() == ".zip":
                dest = self.files_dir / p.stem
                try:
                    with zipfile.ZipFile(p) as z:
                        for member in z.infolist():
                            target = (dest / member.filename).resolve()
                            if member.is_dir() or dest.resolve() not in target.parents or member.file_size > 400_000_000:
                                continue
                            target.parent.mkdir(parents=True, exist_ok=True)
                            with z.open(member) as src, open(target, "wb") as dst:
                                shutil.copyfileobj(src, dst)
                except (zipfile.BadZipFile, OSError, RuntimeError) as exc:
                    self.warnings.append(f"{p.name}: not a readable zip ({exc})")
                if dest.is_dir():
                    out.extend(self.expand(sorted(dest.iterdir())))
            elif p.is_file():
                out.append(p)
        return out


def pull_quotes(articles: list[dict], n: int = 5) -> list[dict]:
    cands = []
    for a in articles:
        paras = [b["text"] for b in a["blocks"] if b["type"] == "paragraph"]
        for pi, para in enumerate(paras):
            for s in re.split(r"(?<=[.!?”])\s+(?=[A-Z“])", para):
                w = B.words(s)
                if not 8 <= w <= 30:
                    continue
                score = 2.0 - abs(w - 16) / 16
                if pi == 0:
                    score -= 0.6
                if re.search(r"\b(never|always|every|best|most|only|first|future|why|how)\b", s, re.I):
                    score += 0.5
                if s.startswith("“") or '"' in s:
                    score += 0.6
                if re.search(r"\d", s):
                    score -= 0.3
                cands.append({"text": s.strip(), "article": a["id"], "score": round(score, 2)})
    cands.sort(key=lambda c: -c["score"])
    seen, out = set(), []
    for c in cands:
        key = c["text"][:40]
        if key not in seen:
            seen.add(key)
            out.append(c)
        if len(out) >= n:
            break
    return out


def merged_palette(images: list[dict], brief_colors: list[str]) -> list[str]:
    weights: Counter = Counter()
    for img in images:
        area = img["width"] * img["height"]
        for rank, c in enumerate(img.get("colors", [])):
            weights[c] += area / (rank + 1)
    out = list(brief_colors)
    for c, _ in weights.most_common(12):
        if c not in out:
            out.append(c)
    return out[:8]


def ingest(paths: list[Path], brief_text: str = "", options: dict | None = None, pack_id: str | None = None) -> dict:
    pack_id = pack_id or new_pack_id()
    root = packs.dir(pack_id)
    it = Intake(pack_id, root)
    files = it.expand(paths)
    for f in files:
        if f.suffix.lower() in I.IMAGE_EXTS:
            it.add_image_file(f, "upload")
    for f in files:
        try:
            it.ingest_file(f)
        except Exception as exc:
            it.warnings.append(f"{f.name}: could not be read ({exc.__class__.__name__}: {str(exc)[:120]})")
    text = "\n\n".join([brief_text or "", *it.brief_parts]).strip()
    brief = brief_mod.parse(text, options)
    it.articles.sort(key=lambda a: -a["words"])
    for n, a in enumerate(it.articles, 1):
        a["id"] = f"a{n}"
        if a["byline"].lower() in GENERATOR_NAMES:
            a["byline"] = ""
    used = {b["ref"] for a in it.articles for b in a["blocks"] if b["type"] == "image"}
    for img in it.images:
        img["used"] = img["id"] in used
    first = it.articles[0] if it.articles else {}
    words = sum(a["words"] for a in it.articles)
    pack = {
        "id": pack_id, "created": time.time(), "version": 1,
        "title": brief.get("headline") or first.get("title") or it.meta_seen.get("title") or "",
        "subtitle": first.get("deck") or "",
        "author": first.get("byline") or "",
        "date": first.get("date") or it.meta_seen.get("date") or "",
        "brief": brief, "articles": it.articles, "images": it.images, "tables": it.tables, "assets": it.assets,
        "quotes": pull_quotes(it.articles), "palette": merged_palette(it.images, brief.get("colors", [])),
        "files": it.files, "warnings": it.warnings,
        "facts": {"words": words, "articles": len(it.articles), "images": len(it.images), "tables": len(it.tables),
                  "paragraphs": sum(1 for a in it.articles for b in a["blocks"] if b["type"] == "paragraph"),
                  "headings": sum(1 for a in it.articles for b in a["blocks"] if b["type"] == "heading"),
                  "readingMinutes": max(1, round(words / 230)) if words else 0},
    }
    return packs.put(pack)


def summary(pack: dict, detail: str = "brief") -> dict:
    arts = []
    for a in pack.get("articles", []):
        heads = [b["text"][:70] for b in a["blocks"] if b["type"] == "heading"]
        arts.append({"id": a["id"], "title": a["title"][:100], "deck": a["deck"][:140], "byline": a["byline"], "words": a["words"],
                     "headings": heads[:8] if detail == "brief" else heads,
                     "blocks": Counter(b["type"] for b in a["blocks"])})
    imgs = [{"id": i["id"], "name": i["name"][:40], "size": f"{i['width']}x{i['height']}", "orientation": i["orientation"], "role": i["role"],
             **({"animated": True} if i.get("animated") else {}), **({"caption": i["caption"][:80]} if i.get("caption") else {}),
             "colors": i["colors"][:3]} for i in pack.get("images", [])]
    out = {"pack": pack["id"], "title": pack.get("title"), "subtitle": pack.get("subtitle"), "author": pack.get("author"),
           "facts": pack.get("facts"), "brief": {k: v for k, v in pack.get("brief", {}).items() if v and k != "text"},
           "articles": arts, "images": imgs, "tables": [{"id": t["id"], "name": t["name"], "rows": len(t["rows"]), "cols": max((len(r) for r in t["rows"]), default=0)} for t in pack.get("tables", [])],
           "assets": [{"id": a["id"], "name": a["name"], "mime": a["mime"]} for a in pack.get("assets", [])],
           "quotes": [q["text"] for q in pack.get("quotes", [])[:3]], "palette": pack.get("palette"), "warnings": pack.get("warnings")}
    return out
