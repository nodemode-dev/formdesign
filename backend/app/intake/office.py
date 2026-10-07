import csv
import io
import re
from html.parser import HTMLParser
from pathlib import Path

from app.intake import blocks as B


def read_docx(path: Path, image_dir: Path, add_image) -> tuple[dict, list[dict]]:
    import docx
    from docx.oxml.ns import qn

    d = docx.Document(str(path))
    cp = d.core_properties
    meta = {k: v for k, v in {"title": cp.title, "subtitle": cp.subject, "author": cp.author, "keywords": cp.keywords,
                              "date": cp.created.isoformat() if cp.created else ""}.items() if v}
    rels = {}
    for rid, rel in d.part.rels.items():
        if "image" in rel.reltype:
            try:
                rels[rid] = rel.target_part.blob
            except Exception:
                continue
    out: list[dict] = []
    items: list[str] = []
    ordered = False
    n_img = 0

    def flush_list():
        nonlocal items
        if items:
            out.append(B.listing(items, ordered))
            items = []

    body = d.element.body
    for child in body.iterchildren():
        tag = child.tag.rsplit("}", 1)[-1]
        if tag == "tbl":
            flush_list()
            rows = []
            for tr in child.iter(qn("w:tr")):
                rows.append(["".join(t.text or "" for t in tc.iter(qn("w:t"))) for tc in tr.iter(qn("w:tc"))])
            if rows:
                out.append(B.table(rows, header=True))
            continue
        if tag != "p":
            continue
        p = docx.text.paragraph.Paragraph(child, d)
        for blip in child.iter(qn("a:blip")):
            rid = blip.get(qn("r:embed"))
            if rid in rels:
                n_img += 1
                ref = add_image(rels[rid], f"{path.stem}-image{n_img}", f"docx:{path.name}")
                if ref:
                    flush_list()
                    out.append(B.image(ref))
        text = p.text.strip()
        if not text:
            continue
        style = (p.style.name if p.style is not None else "") or ""
        sl = style.lower()
        num = child.find(qn("w:pPr"))
        is_list = num is not None and num.find(qn("w:numPr")) is not None or sl.startswith("list")
        runs = [{"text": r.text, **({"b": True} if r.bold else {}), **({"i": True} if r.italic else {})} for r in p.runs if r.text]
        if sl == "title":
            flush_list()
            out.append(B.heading(1, text))
            meta.setdefault("title", text)
        elif sl == "subtitle":
            flush_list()
            meta.setdefault("subtitle", text)
        elif sl.startswith("heading"):
            flush_list()
            m = re.search(r"(\d)", sl)
            out.append(B.heading(int(m.group(1)) if m else 2, text))
        elif "quote" in sl:
            flush_list()
            out.append(B.quote(text))
        elif is_list:
            ordered = "number" in sl
            items.append(text)
        else:
            flush_list()
            out.append(B.paragraph(text, runs))
    flush_list()
    return meta, out


def read_rtf(path: Path) -> tuple[dict, list[dict]]:
    from striprtf.striprtf import rtf_to_text
    return read_text_content(rtf_to_text(path.read_text(encoding="utf-8", errors="replace")))


def read_text_content(text: str) -> tuple[dict, list[dict]]:
    chunks = [c.strip() for c in re.split(r"\n\s*\n", text.replace("\r\n", "\n")) if c.strip()]
    out: list[dict] = []
    for i, c in enumerate(chunks):
        lines = [l.strip() for l in c.split("\n") if l.strip()]
        if all(re.match(r"^([•*\-]|\d+[.)])\s+", l) for l in lines) and len(lines) > 1:
            out.append(B.listing([re.sub(r"^([•*\-]|\d+[.)])\s+", "", l) for l in lines]))
        elif len(lines) == 1 and B.words(lines[0]) <= 12 and not lines[0].endswith((".", "!", "?", ":", ";", ",")):
            out.append(B.heading(1 if i == 0 else 2, lines[0]))
        else:
            out.append(B.paragraph(" ".join(lines)))
    return {}, out


def read_text(path: Path) -> tuple[dict, list[dict]]:
    return read_text_content(path.read_text(encoding="utf-8", errors="replace"))


class _Html(HTMLParser):
    BLOCK = {"p", "h1", "h2", "h3", "h4", "h5", "h6", "li", "blockquote", "pre", "figcaption", "td", "th", "title"}

    def __init__(self, add_url_image):
        super().__init__(convert_charrefs=True)
        self.out: list[dict] = []
        self.meta: dict = {}
        self.stack: list[str] = []
        self.buf: list[str] = []
        self.items: list[str] = []
        self.ordered = False
        self.rows: list[list[str]] = []
        self.row: list[str] | None = None
        self.skip = 0
        self.add_url_image = add_url_image

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag in ("script", "style", "nav", "footer", "noscript", "svg"):
            self.skip += 1
        if tag == "meta" and a.get("name") in ("description", "author"):
            self.meta[a["name"]] = a.get("content", "")
        if tag == "meta" and a.get("property") == "og:title":
            self.meta.setdefault("title", a.get("content", ""))
        if tag in ("ul", "ol"):
            self.ordered = tag == "ol"
        if tag == "tr":
            self.row = []
        if tag == "img" and not self.skip:
            ref = self.add_url_image(a.get("src", ""), a.get("alt", ""))
            if ref:
                self.out.append(B.image(ref, alt=a.get("alt", ""), caption=a.get("title", "") or ""))
        if tag in self.BLOCK:
            self.buf = []
        self.stack.append(tag)

    def handle_endtag(self, tag):
        if tag in ("script", "style", "nav", "footer", "noscript", "svg") and self.skip:
            self.skip -= 1
        text = B.clean("".join(self.buf))
        if tag == "title":
            self.meta.setdefault("title", text)
        elif tag in ("h1", "h2", "h3", "h4", "h5", "h6") and text and not self.skip:
            self.out.append(B.heading(int(tag[1]), text))
        elif tag == "p" and text and not self.skip:
            self.out.append(B.paragraph(text))
        elif tag == "li" and text and not self.skip:
            self.items.append(text)
        elif tag in ("ul", "ol") and self.items:
            self.out.append(B.listing(self.items, self.ordered))
            self.items = []
        elif tag == "blockquote" and text and not self.skip:
            self.out.append(B.quote(text))
        elif tag == "pre" and text and not self.skip:
            self.out.append(B.code("".join(self.buf)))
        elif tag in ("td", "th") and self.row is not None:
            self.row.append(text)
        elif tag == "tr" and self.row is not None:
            self.rows.append(self.row)
            self.row = None
        elif tag == "table" and self.rows:
            self.out.append(B.table(self.rows))
            self.rows = []
        if tag in self.BLOCK:
            self.buf = []
        while self.stack and self.stack.pop() != tag:
            pass

    def handle_data(self, data):
        if not self.skip:
            self.buf.append(data)


def read_html(path: Path, add_url_image) -> tuple[dict, list[dict]]:
    p = _Html(add_url_image)
    p.feed(path.read_text(encoding="utf-8", errors="replace"))
    return p.meta, p.out


def read_sheet(path: Path) -> list[dict]:
    ext = path.suffix.lower()
    tables = []
    if ext in (".csv", ".tsv"):
        text = path.read_text(encoding="utf-8-sig", errors="replace")
        dialect = csv.excel_tab if ext == ".tsv" else csv.Sniffer().sniff(text[:4096], delimiters=",;\t|") if text.strip() else csv.excel
        rows = [r for r in csv.reader(io.StringIO(text), dialect)]
        tables.append({"name": path.stem, "rows": rows[:2000]})
    else:
        import openpyxl
        wb = openpyxl.load_workbook(str(path), read_only=True, data_only=True)
        for ws in wb.worksheets[:12]:
            rows = []
            for r in ws.iter_rows(values_only=True):
                if len(rows) >= 2000:
                    break
                rows.append(["" if v is None else str(v) for v in r])
            while rows and not any(c.strip() for c in rows[-1]):
                rows.pop()
            if rows:
                tables.append({"name": ws.title, "rows": rows})
    return tables
