import re
from pathlib import Path

from markdown_it import MarkdownIt
from mdit_py_plugins.front_matter import front_matter_plugin

from app.intake import blocks as B

MD = MarkdownIt("commonmark", {"html": False, "typographer": True}).enable(["table", "strikethrough", "replacements", "smartquotes"]).use(front_matter_plugin)


def parse_front_matter(text: str) -> dict:
    meta: dict = {}
    for line in text.splitlines():
        m = re.match(r"^([A-Za-z_][\w-]*)\s*:\s*(.*)$", line)
        if not m:
            continue
        key, val = m.group(1).lower(), m.group(2).strip().strip('"').strip("'")
        if val.startswith("[") and val.endswith("]"):
            meta[key] = [v.strip().strip('"').strip("'") for v in val[1:-1].split(",") if v.strip()]
        else:
            meta[key] = val
    return meta


def _inline(tok) -> tuple[str, list[dict], list[tuple[str, str]]]:
    runs: list[dict] = []
    text = []
    bold = ital = 0
    href = None
    images: list[tuple[str, str]] = []
    for c in tok.children or []:
        t = c.type
        if t == "strong_open":
            bold += 1
        elif t == "strong_close":
            bold -= 1
        elif t == "em_open":
            ital += 1
        elif t == "em_close":
            ital -= 1
        elif t == "link_open":
            href = c.attrs.get("href")
        elif t == "link_close":
            href = None
        elif t in ("text", "code_inline"):
            r = {"text": c.content}
            if bold:
                r["b"] = True
            if ital:
                r["i"] = True
            if t == "code_inline":
                r["code"] = True
            if href:
                r["href"] = href
            runs.append(r)
            text.append(c.content)
        elif t in ("softbreak", "hardbreak"):
            runs.append({"text": " "})
            text.append(" ")
        elif t == "image":
            images.append((c.attrs.get("src", ""), c.content or ""))
    return "".join(text), runs, images


def parse(text: str, source: str, resolve_image) -> tuple[dict, list[dict]]:
    tokens = MD.parse(text)
    meta: dict = {}
    out: list[dict] = []
    i = 0
    list_stack: list[dict] = []
    in_quote = 0
    quote_parts: list[str] = []
    table_rows: list[list[str]] = []
    row: list[str] | None = None
    while i < len(tokens):
        tok = tokens[i]
        t = tok.type
        if t == "front_matter":
            meta = parse_front_matter(tok.content)
        elif t == "heading_open":
            inline = tokens[i + 1]
            content, _, imgs = _inline(inline)
            out.append(B.heading(int(tok.tag[1]), content))
            i += 2
        elif t == "paragraph_open":
            inline = tokens[i + 1]
            content, runs, imgs = _inline(inline)
            for src, alt in imgs:
                ref = resolve_image(src, alt)
                if ref:
                    out.append(B.image(ref, caption=alt, alt=alt))
            if content.strip():
                if list_stack:
                    list_stack[-1]["items"].append(content)
                elif in_quote:
                    quote_parts.append(content)
                else:
                    out.append(B.paragraph(content, runs))
            i += 2
        elif t in ("bullet_list_open", "ordered_list_open"):
            list_stack.append({"ordered": t.startswith("ordered"), "items": []})
        elif t in ("bullet_list_close", "ordered_list_close"):
            lst = list_stack.pop()
            if list_stack:
                list_stack[-1]["items"].extend(lst["items"])
            else:
                out.append(B.listing(lst["items"], lst["ordered"]))
        elif t == "blockquote_open":
            in_quote += 1
            quote_parts = []
        elif t == "blockquote_close":
            in_quote -= 1
            if not in_quote and quote_parts:
                txt = " ".join(quote_parts)
                m = re.match(r"^(.*?)\s*[—–-]{1,2}\s*([^—–-]{2,80})$", txt)
                out.append(B.quote(m.group(1), m.group(2)) if m and len(m.group(1)) > 20 else B.quote(txt))
        elif t in ("fence", "code_block"):
            out.append(B.code(tok.content, (tok.info or "").strip()))
        elif t == "hr":
            out.append({"type": "rule"})
        elif t == "table_open":
            table_rows = []
        elif t == "tr_open":
            row = []
        elif t in ("th_open", "td_open"):
            content, _, _ = _inline(tokens[i + 1])
            if row is not None:
                row.append(content)
            i += 2
            continue
        elif t == "tr_close":
            if row is not None:
                table_rows.append(row)
            row = None
        elif t == "table_close":
            out.append(B.table(table_rows, header=True))
        i += 1
    return meta, out


def read(path: Path, source: str, resolve_image) -> tuple[dict, list[dict]]:
    return parse(path.read_text(encoding="utf-8", errors="replace"), source, resolve_image)
