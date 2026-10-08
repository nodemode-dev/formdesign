import re

WORD = re.compile(r"[\w'\u2019-]+", re.UNICODE)
BYLINE = re.compile(r"^\s*(?:by|words by|written by|text by|story by)\s+([A-Z][^\n]{1,60}?)\s*$", re.I)


def words(text: str) -> int:
    return len(WORD.findall(text or ""))


def heading(level: int, text: str) -> dict:
    return {"type": "heading", "level": max(1, min(6, int(level))), "text": clean(text)}


def paragraph(text: str, runs: list[dict] | None = None) -> dict:
    b = {"type": "paragraph", "text": clean(text)}
    if runs and any(r.get("b") or r.get("i") or r.get("code") or r.get("href") for r in runs):
        b["runs"] = runs
    return b


def listing(items: list[str], ordered: bool = False) -> dict:
    return {"type": "list", "ordered": ordered, "items": [clean(i) for i in items if clean(i)]}


def quote(text: str, cite: str = "") -> dict:
    b = {"type": "quote", "text": clean(text)}
    if cite:
        b["cite"] = clean(cite)
    return b


def image(ref: str, caption: str = "", alt: str = "") -> dict:
    b = {"type": "image", "ref": ref}
    if caption:
        b["caption"] = clean(caption)
    if alt:
        b["alt"] = clean(alt)
    return b


def table(rows: list[list[str]], header: bool = True) -> dict:
    return {"type": "table", "header": header, "rows": [[clean(c) for c in r] for r in rows if any(clean(c) for c in r)]}


def code(text: str, lang: str = "") -> dict:
    b = {"type": "code", "text": text.rstrip("\n")}
    if lang:
        b["lang"] = lang
    return b


def clean(s: str) -> str:
    s = (s or "").replace("\u00a0", " ").replace("\r", "")
    s = re.sub(r"[ \t]+", " ", s)
    return s.strip()


def smart(text: str) -> str:
    t = re.sub(r"(^|[\s(\[{])\"", "\\1\u201c", text)
    t = t.replace('"', "\u201d")
    t = re.sub(r"(^|[\s(\[{])'", "\\1\u2018", t)
    t = t.replace("'", "\u2019")
    return t.replace(" -- ", " \u2013 ").replace("...", "\u2026")


def block_words(blocks: list[dict]) -> int:
    n = 0
    for b in blocks:
        if b["type"] in ("paragraph", "heading", "quote", "code"):
            n += words(b.get("text", ""))
        elif b["type"] == "list":
            n += sum(words(i) for i in b["items"])
        elif b["type"] == "table":
            n += sum(words(c) for r in b["rows"] for c in r)
    return n


def article_from_blocks(blocks: list[dict], source: str, aid: str, meta: dict | None = None) -> dict:
    meta = meta or {}
    title = meta.get("title") or ""
    deck = meta.get("subtitle") or meta.get("description") or meta.get("deck") or ""
    rest = list(blocks)
    if not title and rest and rest[0]["type"] == "heading":
        title = rest.pop(0)["text"]
    if not deck and rest and rest[0]["type"] == "paragraph" and words(rest[0]["text"]) <= 45 and len(rest) > 2:
        if rest[0].get("runs") and all(r.get("i") for r in rest[0]["runs"] if r.get("text", "").strip()):
            deck = rest.pop(0)["text"]
    if not deck and title and len(rest) > 2 and rest[0]["type"] == "heading" and rest[0]["level"] >= 2 and 5 <= words(rest[0]["text"]) <= 30:
        deck = rest.pop(0)["text"]
    byline = meta.get("author") or meta.get("byline") or ""
    for i, b in enumerate(rest[:2]):
        m = BYLINE.match(b.get("text", "")) if b["type"] == "paragraph" else None
        if m and not byline and words(b["text"]) <= 10:
            byline = m.group(1)
            rest.pop(i)
            break
    if not title:
        first = next((b for b in rest if b["type"] == "paragraph"), None)
        if first and words(first["text"]) <= 12:
            title = first["text"]
            rest.remove(first)
    return {"id": aid, "title": clean(title), "deck": clean(deck), "byline": clean(byline),
            "date": str(meta.get("date") or ""), "tags": meta.get("tags") or [], "source": source, "blocks": rest,
            "words": block_words(rest), "meta": {k: v for k, v in meta.items() if k not in ("title", "subtitle", "description", "deck", "author", "byline", "date", "tags")}}
