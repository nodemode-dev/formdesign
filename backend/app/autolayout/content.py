import datetime as dt
import re

from app.intake import blocks as B

SENT = re.compile(r"(?<=[.!?”])\s+(?=[A-Z“\"'])")
STAT = re.compile(r"(?<![\w.$£€])([$£€]?\d[\d,]*(?:\.\d+)?\s?(?:%|x\b|k\b|m\b|bn\b|million\b|billion\b|thousand\b)?\+?)")
PRICE = re.compile(r"([$£€]\s?\d[\d,]*(?:\.\d{2})?|\d[\d,]*(?:\.\d{2})?\s?(?:USD|EUR|GBP))")
SLOT = re.compile(r"^([a-z]+?)(\d+)?(?:\.([a-z]+))?$")
LINE_BREAK = chr(0x2028)
LONG = {"intro", "summary", "body", "note", "legal", "details", "caption", "pullquote", "quote", "contact", "address", "list", "toc"}


def parse_slot(slot: str) -> tuple[str, int | None, str | None]:
    m = SLOT.match(slot or "")
    if not m:
        return slot, None, None
    return m.group(1), int(m.group(2)) if m.group(2) else None, m.group(3)


def sentences(text: str) -> list[str]:
    return [s.strip() for s in SENT.split(text or "") if s.strip()]


def trim_words(text: str, n: int) -> str:
    if B.words(text) <= n:
        return text
    out = ""
    for s in sentences(text):
        if B.words(f"{out} {s}") > n:
            break
        out = f"{out} {s}".strip()
    if out:
        return out
    return " ".join(text.split()[:n]).rstrip(",;:") + "…"


def nice_date(value: str) -> str:
    v = (value or "").strip()
    for fmt in ("%Y-%m-%d", "%Y-%m-%dT%H:%M:%S", "%Y/%m/%d"):
        try:
            d = dt.datetime.strptime(v[:19] if "T" in v else v[:10], fmt)
            return f"{d.day} {d.strftime('%B %Y')}"
        except ValueError:
            continue
    return v


def stats_from(articles: list[dict], n: int = 8) -> list[dict]:
    out, seen = [], set()
    for a in articles:
        for b in a["blocks"]:
            if b["type"] not in ("paragraph", "list"):
                continue
            texts = b["items"] if b["type"] == "list" else sentences(b["text"])
            for s in texts:
                for m in STAT.finditer(s):
                    raw = m.group(1).strip()
                    digits = re.sub(r"[^\d.]", "", raw)
                    try:
                        num = float(digits.rstrip(".") or 0)
                    except ValueError:
                        continue
                    unit = bool(re.search(r"[%xkmb$£€+]|million|billion|thousand", raw, re.I))
                    if not unit and (num < 10 or (1800 <= num <= 2100 and "." not in digits)):
                        continue
                    tail = s[m.end():].strip(" ,")
                    label = re.split(r"[,;:.()\N{EM DASH}\N{EN DASH}]", tail)[0].strip()
                    label = " ".join(label.split()[:7])
                    if B.words(label) < 2 or raw in seen:
                        continue
                    seen.add(raw)
                    out.append({"value": raw.replace(" ", ""), "label": label, "score": 2 if unit else 1})
    out.sort(key=lambda x: -x["score"])
    return out[:n]


def split_entry(text: str) -> dict:
    t = text.strip()
    price = None
    m = PRICE.search(t)
    if m:
        price = m.group(1).replace(" ", "")
        t = (t[:m.start()] + t[m.end():]).strip(" ,.-\N{EN DASH}\N{EM DASH}|")
    m = re.match(r"^(.{2,60}?)\s*(?::|\N{EM DASH}|\N{EN DASH}| - |\|)\s*(.+)$", t)
    if m:
        return {"title": m.group(1).strip(), "body": m.group(2).strip(), "price": price}
    m = re.match(r"^(.{2,60}?)[.!?]\s+(.+)$", t)
    if m and B.words(m.group(1)) <= 8:
        return {"title": m.group(1).strip(), "body": m.group(2).strip(), "price": price}
    return {"title": t, "body": "", "price": price}


def sections_of(article: dict) -> list[dict]:
    out, cur = [], None
    for b in article.get("blocks", []):
        if b["type"] == "heading":
            cur = {"title": b["text"], "body": "", "paras": []}
            out.append(cur)
        elif b["type"] == "paragraph" and cur is not None:
            cur["paras"].append(b["text"])
            if not cur["body"]:
                cur["body"] = b["text"]
    return out


class Content:
    def __init__(self, pack: dict, has_flow: bool = False, overrides: dict | None = None):
        self.pack = pack
        self.brief = pack.get("brief") or {}
        self.fields = {**(self.brief.get("fields") or {}), **{k: v for k, v in (overrides or {}).items() if v is not None}}
        self.arts = pack.get("articles") or []
        self.main = self.arts[0] if self.arts else {"title": "", "deck": "", "byline": "", "date": "", "blocks": [], "meta": {}, "tags": []}
        self.has_flow = has_flow
        self.quotes = [q["text"] for q in pack.get("quotes") or []]
        self.used_quotes = 0
        self.consumed_first = False
        self._stats = None
        self._features = None
        self._items = None

    def first_para(self) -> str:
        return next((b["text"] for b in self.main["blocks"] if b["type"] == "paragraph"), "")

    def headings(self) -> list[str]:
        return [b["text"] for b in self.main["blocks"] if b["type"] == "heading"]

    def stats(self) -> list[dict]:
        if self._stats is None:
            self._stats = stats_from(self.arts)
        return self._stats

    def features(self) -> list[dict]:
        if self._features is None:
            secs = sections_of(self.main)
            others = [{"title": a["title"], "body": a["deck"] or next((b["text"] for b in a["blocks"] if b["type"] == "paragraph"), "")} for a in self.arts[1:]]
            lists = [split_entry(i) for b in self.main["blocks"] if b["type"] == "list" for i in b["items"]]
            if self.has_flow:
                pool = others + lists + ([] if others or lists else secs)
            else:
                pool = (secs if len(secs) >= 2 else []) + others + lists
            self._features = pool
        return self._features

    def items(self) -> list[dict]:
        if self._items is None:
            out = []
            for t in self.pack.get("tables") or []:
                rows = t.get("rows") or []
                for r in rows[1:] if len(rows) > 1 else rows:
                    cells = [c for c in r if c]
                    if not cells:
                        continue
                    price = next((c for c in cells if PRICE.fullmatch(c.strip())), None)
                    rest = [c for c in cells if c is not price]
                    out.append({"title": rest[0] if rest else "", "body": max(rest[1:], key=len) if len(rest) > 1 else "", "price": price,
                                "cells": cells})
            for a in self.arts:
                for b in a["blocks"]:
                    if b["type"] == "list":
                        out += [split_entry(i) for i in b["items"]]
            self._items = out or self.features()
        return self._items

    def value(self, slot: str) -> list[str] | None:
        f = self.fields
        base, n, sub = parse_slot(slot)
        m = self.main
        if slot in f:
            return [str(f[slot])]
        if base == "title":
            v = self.brief.get("headline") or m["title"] or self.pack.get("title") or self.brief.get("subject")
            return [v] if v else None
        if base == "subtitle":
            v = m["deck"] or self.pack.get("subtitle") or (f.get("tagline") if not self.arts else None)
            return [v] if v else None
        if base == "kicker" and n is None:
            meta = m.get("meta") or {}
            v = meta.get("section") or meta.get("category") or (m.get("tags") or [None])[0]
            return [str(v)] if v else None
        if base == "byline":
            v = m["byline"] or self.pack.get("author")
            return [v if v.lower().startswith(("by ", "words ")) else f"By {v}"] if v else None
        if base == "date":
            v = m.get("date") or self.pack.get("date") or f.get("eventdate")
            return [nice_date(v)] if v else None
        if base == "runninghead":
            v = m["title"] or self.pack.get("title")
            return [v] if v else None
        if base == "intro":
            p = self.first_para()
            if p and self.has_flow:
                self.consumed_first = True
            return [p] if p else None
        if base == "summary":
            v = f.get("summary") or m["deck"] or trim_words(self.first_para(), 60)
            return [v] if v else None
        if base == "section":
            hs = self.headings()
            k = (n or 1) - 1
            return [hs[k]] if k < len(hs) else None
        if base in ("pullquote", "quote"):
            if self.used_quotes < len(self.quotes):
                self.used_quotes += 1
                return [self.quotes[self.used_quotes - 1]]
            return None
        if base == "quotecredit":
            v = m["byline"] or self.brief.get("brand")
            return [v] if v else None
        if base == "brand":
            v = self.brief.get("brand") or f.get("brand")
            return [v] if v else None
        if base in ("contact", "details"):
            lines = [f[k] for k in ("address", "phone", "email", "url") if f.get(k)]
            return lines or None
        if base == "footer":
            b = self.brief.get("brand")
            return [f"© {dt.date.today().year} {b}"] if b else None
        if base == "list":
            lst = next((b for b in m["blocks"] if b["type"] == "list"), None)
            return lst["items"] if lst else None
        if base == "toc":
            titles = [a["title"] for a in self.arts if a["title"]] if len(self.arts) > 1 else self.headings()
            return titles or None
        if base == "stat" and n:
            st = self.stats()
            if n - 1 < len(st):
                return [st[n - 1]["label" if sub == "label" else "value"]]
            return None
        if base == "feature" and n:
            ft = self.features()
            if n - 1 < len(ft):
                v = ft[n - 1].get(sub or "title")
                return [trim_words(v, 45) if sub == "body" else v] if v else []
            return None
        if base == "item" and n:
            it = self.items()
            if n - 1 < len(it):
                v = it[n - 1].get(sub or "title")
                return [trim_words(v, 30) if sub == "body" else v] if v else []
            return None
        if base == "cta":
            v = self.brief.get("cta")
            return [v] if v and n is None else None
        if base == "tagline":
            v = f.get("tagline") or (m["deck"] if not self.has_flow else None)
            return [v] if v else None
        return None

    def flow(self, rt: dict, frame_style: str | None, medium: str) -> list[dict]:
        names = set(rt.get("styleNames") or [])
        body = frame_style if frame_style and frame_style in names else "Body"
        custom = body != "Body"

        def pick(name: str) -> str:
            return body if custom or name not in names else name

        levels = sorted({b["level"] for b in self.main["blocks"] if b["type"] == "heading"})
        top = "Heading 1" if medium == "web" else "Heading 2"
        heads = {lv: (top if i == 0 else "Heading 3") for i, lv in enumerate(levels)}
        out: list[dict] = []
        first = True
        after_head = False
        skipped = False
        for b in self.main["blocks"]:
            t = b["type"]
            if t == "paragraph":
                if self.consumed_first and not skipped:
                    skipped = True
                    continue
                style = pick("Body First") if first else pick("Body No Indent") if after_head else body
                p = {"style": style, "text": b["text"]}
                if b.get("runs"):
                    p["runs"] = [{"text": r.get("text", ""), **({"cs": "Strong"} if r.get("b") else {"cs": "Emphasis"} if r.get("i") else
                                                                   {"cs": "Code"} if r.get("code") else {"cs": "Link"} if r.get("href") else {})}
                                 for r in b["runs"]]
                out.append(p)
                first = False
                after_head = False
            elif t == "heading":
                out.append({"style": pick(heads.get(b["level"], "Heading 3")), "text": b["text"]})
                after_head = True
            elif t == "quote":
                out.append({"style": pick("Blockquote"), "text": b["text"]})
                if b.get("cite"):
                    out.append({"style": pick("Credit"), "text": b["cite"]})
                after_head = True
            elif t == "list":
                st = pick("Numbered" if b.get("ordered") else "List")
                out += [{"style": st, "text": i} for i in b["items"]]
                after_head = True
            elif t == "code":
                out.append({"style": pick("Code"), "text": b["text"].replace("\n", LINE_BREAK)})
                after_head = True
            elif t == "table":
                rows = b.get("rows") or []
                for k, r in enumerate(rows):
                    out.append({"style": pick("Table Head" if k == 0 and b.get("header") else "Table Body"), "text": "\t".join(r)})
                after_head = True
        return out
