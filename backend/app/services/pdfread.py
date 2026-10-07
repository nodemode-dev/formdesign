import io
import re
import statistics
import threading
from collections import Counter
from pathlib import Path

import pypdfium2 as pdfium
import pypdfium2.raw as raw

from app.intake import blocks as B

_lock = threading.Lock()
_cache: dict[str, tuple[float, pdfium.PdfDocument]] = {}

BULLET = re.compile(r"^\s*([•●▪–—\-*·]|\d{1,3}[.)]|[a-z][.)])\s+")


def _doc(path: str) -> pdfium.PdfDocument:
    p = Path(path)
    mtime = p.stat().st_mtime
    hit = _cache.get(path)
    if hit and hit[0] == mtime:
        return hit[1]
    if hit:
        hit[1].close()
    pdf = pdfium.PdfDocument(str(p))
    _cache[path] = (mtime, pdf)
    if len(_cache) > 8:
        oldest = next(iter(_cache))
        _cache.pop(oldest)[1].close()
    return pdf


def info(path: str) -> dict:
    with _lock:
        pdf = _doc(path)
        meta = {k.lower(): v for k, v in pdf.get_metadata_dict(skip_empty=True).items()}
        sizes = [list(pdf.get_page_size(i)) for i in range(len(pdf))]
        toc = []
        try:
            for item in pdf.get_toc():
                toc.append({"level": item.level, "title": item.title, "page": item.page_index})
        except Exception:
            toc = []
        labels = []
        for i in range(len(pdf)):
            try:
                labels.append(pdf.get_page_label(i) or str(i + 1))
            except Exception:
                labels.append(str(i + 1))
        return {"pages": len(pdf), "sizes": sizes, "labels": labels, "toc": toc, "meta": meta, "version": pdf.get_version(),
                "tagged": bool(pdf.is_tagged()), "attachments": pdf.count_attachments()}


def render(path: str, page: int, scale: float | None = None, max_side: float | None = None, fmt: str = "png") -> bytes:
    with _lock:
        pdf = _doc(path)
        pg = pdf[page]
        w, h = pg.get_size()
        s = scale or ((max_side / max(w, h)) if max_side else 1.5)
        s = max(0.05, min(s, 8.0))
        im = pg.render(scale=s, may_draw_forms=True).to_pil()
        pg.close()
    buf = io.BytesIO()
    if fmt == "jpeg":
        im.convert("RGB").save(buf, "JPEG", quality=86)
    else:
        im.save(buf, "PNG")
    return buf.getvalue()


def _spans(pdf: pdfium.PdfDocument, i: int) -> tuple[float, float, list[dict]]:
    pg = pdf[i]
    w, h = pg.get_size()
    tp = pg.get_textpage()
    out = []
    for r in range(tp.count_rects()):
        l, b, rr, t = tp.get_rect(r)
        text = tp.get_text_bounded(l, b, rr, t)
        if not text or not text.strip():
            continue
        idx = tp.get_index(l + 0.6, (b + t) / 2, 2, 2)
        ok = idx is not None and idx >= 0
        size = raw.FPDFText_GetFontSize(tp.raw, idx) if ok else (t - b)
        weight = raw.FPDFText_GetFontWeight(tp.raw, idx) if ok else 400
        out.append({"x": l, "y": h - t, "w": rr - l, "h": t - b, "text": text.replace("\r", " ").replace("\n", " "), "size": round(float(size), 2), "weight": int(weight)})
    tp.close()
    pg.close()
    return w, h, out


def text_layer(path: str, page: int) -> dict:
    with _lock:
        w, h, spans = _spans(_doc(path), page)
    return {"width": w, "height": h, "spans": [{k: (round(v, 2) if isinstance(v, float) else v) for k, v in s.items()} for s in spans]}


def search(path: str, query: str, limit: int = 200) -> list[dict]:
    hits = []
    with _lock:
        pdf = _doc(path)
        for i in range(len(pdf)):
            pg = pdf[i]
            _, h = pg.get_size()
            tp = pg.get_textpage()
            s = tp.search(query, match_case=False, match_whole_word=False)
            while len(hits) < limit:
                m = s.get_next()
                if not m:
                    break
                start, count = m
                rects = []
                for k in range(tp.count_rects(start, count)):
                    l, b, r, t = tp.get_rect(k)
                    rects.append([round(l, 2), round(h - t, 2), round(r - l, 2), round(t - b, 2)])
                context = tp.get_text_range(max(0, start - 40), count + 80).replace("\r", " ").replace("\n", " ")
                hits.append({"page": i, "rects": rects, "context": context.strip()})
            tp.close()
            pg.close()
            if len(hits) >= limit:
                break
    return hits


def page_text(path: str, page: int) -> str:
    with _lock:
        pg = _doc(path)[page]
        tp = pg.get_textpage()
        t = tp.get_text_range()
        tp.close()
        pg.close()
    return t


def extract_images(path: str, dest: Path, min_side: int = 160, limit: int = 120) -> list[dict]:
    dest.mkdir(parents=True, exist_ok=True)
    found = []
    with _lock:
        pdf = _doc(path)
        for i in range(len(pdf)):
            pg = pdf[i]
            for n, obj in enumerate(pg.get_objects(filter=[raw.FPDF_PAGEOBJ_IMAGE], max_depth=3)):
                if len(found) >= limit:
                    break
                try:
                    pw, ph = obj.get_px_size()
                    if max(pw, ph) < min_side:
                        continue
                    im = obj.get_bitmap(render=False).to_pil()
                    out = dest / f"p{i + 1}-img{n + 1}.png"
                    if im.mode not in ("RGB", "RGBA", "L"):
                        im = im.convert("RGB")
                    if im.mode == "RGB" and max(im.size) > 600:
                        out = out.with_suffix(".jpg")
                        im.save(out, "JPEG", quality=90)
                    else:
                        im.save(out, "PNG")
                    found.append({"path": str(out), "page": i, "width": im.size[0], "height": im.size[1]})
                except Exception:
                    continue
            pg.close()
    return found


def _lines(spans: list[dict]) -> list[dict]:
    spans = sorted(spans, key=lambda s: (round(s["y"] + s["h"], 0), s["x"]))
    lines: list[dict] = []
    for s in spans:
        base = s["y"] + s["h"]
        near = lines and s["x"] - lines[-1]["x1"] <= max(3.0, 0.9 * s["size"])
        if lines and near and abs(lines[-1]["base"] - base) <= max(1.5, 0.35 * s["size"]) and s["x"] >= lines[-1]["x1"] - 2:
            ln = lines[-1]
            gap = s["x"] - ln["x1"]
            ln["text"] += (" " if gap > 0.15 * s["size"] and not ln["text"].endswith(" ") else "") + s["text"]
            ln["x1"] = s["x"] + s["w"]
            ln["chars"] += len(s["text"])
            ln["sizes"][s["size"]] = ln["sizes"].get(s["size"], 0) + len(s["text"])
            ln["weights"][s["weight"]] = ln["weights"].get(s["weight"], 0) + len(s["text"])
        else:
            lines.append({"x0": s["x"], "x1": s["x"] + s["w"], "y": s["y"], "base": base, "text": s["text"], "chars": len(s["text"]),
                          "sizes": {s["size"]: len(s["text"])}, "weights": {s["weight"]: len(s["text"])}})
    for ln in lines:
        ln["size"] = max(ln.pop("sizes").items(), key=lambda kv: kv[1])[0]
        ln["weight"] = max(ln.pop("weights").items(), key=lambda kv: kv[1])[0]
        t = re.sub(r"\s+", " ", ln["text"]).strip()
        toks = t.split(" ")
        if len(toks) >= 5 and sum(1 for k in toks if len(k) == 1) >= 0.6 * len(toks):
            t = re.sub(r"(?<=\S) (?=\S(?:\s|$))", "", t)
        ln["text"] = t
    return [ln for ln in lines if ln["text"]]


def _columns(lines: list[dict], width: float) -> list[dict]:
    if len(lines) < 8:
        return sorted(lines, key=lambda l: l["base"])
    narrow = [l for l in lines if (l["x1"] - l["x0"]) < width * 0.55]
    starts: list[list[float]] = []
    for x in sorted(l["x0"] for l in narrow):
        if starts and x - starts[-1][-1] <= 24:
            starts[-1].append(x)
        else:
            starts.append([x])
    cols = [min(c) for c in starts if len(c) >= 3]
    if len(cols) < 2:
        return sorted(lines, key=lambda l: l["base"])

    def col_of(l: dict) -> int:
        best = 0
        for i, c in enumerate(cols):
            if l["x0"] >= c - 12:
                best = i
        return best

    spanning = sorted([l for l in lines if l not in narrow], key=lambda l: l["base"])
    ordered: list[dict] = []
    remaining = sorted(narrow, key=lambda l: l["base"])
    cut = -1e9
    for sp in spanning + [None]:
        limit = sp["base"] if sp else 1e9
        band = [l for l in remaining if cut < l["base"] <= limit]
        for i in range(len(cols)):
            ordered.extend(l for l in band if col_of(l) == i)
        if sp:
            ordered.append(sp)
            cut = limit
    return ordered


def structure(path: str, max_pages: int = 400) -> dict:
    with _lock:
        pdf = _doc(path)
        pages = []
        for i in range(min(len(pdf), max_pages)):
            w, h, spans = _spans(pdf, i)
            pages.append((w, h, _lines(spans)))
        meta = {k.lower(): v for k, v in pdf.get_metadata_dict(skip_empty=True).items()}
    edge_texts: Counter = Counter()
    for w, h, lines in pages:
        for l in lines:
            if l["y"] < h * 0.07 or l["base"] > h * 0.93:
                edge_texts[re.sub(r"\d+", "#", l["text"])] += 1
    repeated = {t for t, c in edge_texts.items() if c >= max(2, len(pages) // 2)}
    sizes = Counter()
    for _, _, lines in pages:
        for l in lines:
            sizes[round(l["size"] * 2) / 2] += l["chars"]
    body = sizes.most_common(1)[0][0] if sizes else 10.0
    heading_sizes = sorted({s for s in sizes if s >= body * 1.18}, reverse=True)
    level_of = {s: min(3, k + 1) for k, s in enumerate(heading_sizes)}
    out: list[dict] = []
    para: list[str] = []
    items: list[str] = []
    prev = None

    def flush():
        nonlocal para, items
        if para:
            text = ""
            for part in para:
                if text.endswith("-") and part[:1].islower():
                    text = text[:-1] + part
                else:
                    text = (text + " " + part).strip()
            out.append(B.paragraph(text))
            para = []
        if items:
            out.append(B.listing(items))
            items = []

    for w, h, lines in pages:
        for l in _columns(lines, w):
            if re.sub(r"\d+", "#", l["text"]) in repeated or re.fullmatch(r"[\divxlc]+", l["text"].lower()):
                continue
            size = round(l["size"] * 2) / 2
            is_head = size in level_of and len(B.WORD.findall(l["text"])) <= 24
            if not is_head and l["weight"] >= 650 and len(B.WORD.findall(l["text"])) <= 12 and not l["text"].endswith((".", ",", ";")) and size >= body:
                is_head = True
                level_of.setdefault(size, 3)
            if is_head:
                flush()
                if out and out[-1]["type"] == "heading" and prev and prev["size"] == l["size"] and l["base"] - prev["base"] < l["size"] * 1.6:
                    out[-1]["text"] += " " + l["text"]
                else:
                    out.append(B.heading(level_of.get(size, 3), l["text"]))
                prev = l
                continue
            m = BULLET.match(l["text"])
            if m:
                if para:
                    flush()
                items.append(l["text"][m.end():])
                prev = l
                continue
            gap = (l["base"] - prev["base"]) if prev else 0
            new_par = prev is None or gap > max(l["size"], prev["size"]) * 1.75 or gap < 0 or abs(l["size"] - prev["size"]) > 0.6
            if items and not new_par:
                items[-1] += " " + l["text"]
                prev = l
                continue
            if new_par:
                flush()
            para.append(l["text"])
            prev = l
        prev = None
    flush()
    for i, b in enumerate(out):
        if b["type"] == "heading" and b["text"][:1] in "“\"‘" and b["level"] > 1:
            out[i] = B.quote(b["text"].strip("“”\"‘’ "))
    return {"meta": meta, "blocks": out, "bodySize": body, "pages": len(pages)}
