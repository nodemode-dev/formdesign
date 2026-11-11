"""Import PDF Comments: the annotations a reviewer added in Acrobat or Preview become Notes in the document.

The PDF is one this document exported, so page n is page n and a point of the PDF is a point of the page (scaled when the
export changed the size, flipped because PDF counts up from the bottom). The notes are placed by services.notes.
"""
import asyncio
import re
from pathlib import Path

from pypdf import PdfReader
from pypdf.generic import ArrayObject, DictionaryObject, IndirectObject

from app.engine.core import CoreError, core
from app.services import notes

MARKUP = {"Highlight": "Highlight", "Underline": "Underline", "StrikeOut": "Strike-out", "Squiggly": "Squiggly"}
SKIP = {"Link", "Popup", "Widget", "Watermark", "Screen", "TrapNet", "PrinterMark", "3D", "Movie", "Sound", "RichMedia", "Redact"}
LABEL = {"Text": "Comment", "FreeText": "Text box", "Caret": "Insert text", "Ink": "Drawing", "Line": "Line", "Square": "Rectangle", "Circle": "Oval",
         "Polygon": "Polygon", "PolyLine": "Polyline", "Stamp": "Stamp", "FileAttachment": "Attachment", **MARKUP}
DATE = re.compile(r"D?:?(\d{4})(\d\d)?(\d\d)?(\d\d)?(\d\d)?(\d\d)?")


def pdf_date(v) -> str:
    m = DATE.match(str(v or "").strip())
    if not m:
        return ""
    y, mo, d, h, mi, _ = (int(g) if g else 0 for g in m.groups())
    return f"{y:04d}-{max(mo, 1):02d}-{max(d, 1):02d} {h:02d}:{mi:02d}"


def _text(v) -> str:
    s = str(v or "").replace("\r\n", "\n").replace("\r", "\n").strip()
    return s.replace("﻿", "")


def _nums(arr) -> list[float]:
    return [float(x) for x in (arr or [])]


def read_annotations(path: str) -> tuple[list[dict], list[tuple[float, float, float, float]]]:
    """Every annotation worth importing, in page order, and each page's box (x0, y0, w, h)."""
    reader = PdfReader(path)
    if reader.is_encrypted:
        try:
            reader.decrypt("")
        except Exception as exc:
            raise CoreError("the PDF is password protected") from exc
    boxes, out, ids = [], [], {}
    for pi, page in enumerate(reader.pages):
        mb = _nums(page.get("/CropBox") or page.mediabox)
        mb = mb if len(mb) == 4 else _nums(page.mediabox)
        x0, y0, x1, y1 = min(mb[0], mb[2]), min(mb[1], mb[3]), max(mb[0], mb[2]), max(mb[1], mb[3])
        boxes.append((x0, y0, x1 - x0, y1 - y0))
        rot = int(page.get("/Rotate", 0) or 0) % 360
        for ref in page.get("/Annots") or []:
            try:
                obj = ref.get_object() if isinstance(ref, IndirectObject) else ref
            except Exception:
                continue
            if not isinstance(obj, DictionaryObject):
                continue
            sub = str(obj.get("/Subtype", "")).lstrip("/")
            if sub in SKIP or not sub:
                continue
            irt = obj.get("/IRT")
            rec = {
                "page": pi, "subtype": sub, "rotate": rot,
                "author": _text(obj.get("/T")), "contents": _text(obj.get("/Contents")),
                "date": pdf_date(obj.get("/M") or obj.get("/CreationDate")),
                "rect": _nums(obj.get("/Rect")), "quads": _nums(obj.get("/QuadPoints")),
                "subject": _text(obj.get("/Subj")), "state": _text(obj.get("/State")),
                "irt": irt.idnum if isinstance(irt, IndirectObject) else None,
                "id": ref.idnum if isinstance(ref, IndirectObject) else None,
            }
            if rec["contents"] == "" and isinstance(obj.get("/RC"), str):
                rec["contents"] = _text(re.sub(r"<[^>]+>", " ", obj["/RC"]))
            out.append(rec)
            if rec["id"] is not None:
                ids[rec["id"]] = rec
    for r in out:
        if r["irt"] is not None and r["irt"] in ids and ids[r["irt"]] is not r:
            r.setdefault("parent", r["irt"])
    return out, boxes


def _to_page(box: tuple[float, float, float, float], page_pt: tuple[float, float], x: float, y: float) -> tuple[float, float]:
    """A PDF point to a point of the document's page: from the top left, in the document's units."""
    bx, by, bw, bh = box
    sx, sy = (page_pt[0] / bw if bw else 1.0), (page_pt[1] / bh if bh else 1.0)
    return (x - bx) * sx, (bh - (y - by)) * sy


def _quote_pdf(path: str, page: int, rect: list[float]) -> str:
    try:
        import pypdfium2 as pdfium

        from app.services.pdfread import _lock as pdfium_lock
        with pdfium_lock:
            pdf = pdfium.PdfDocument(path)
            try:
                tp = pdf[page].get_textpage()
                return " ".join(tp.get_text_bounded(left=rect[0], bottom=rect[1], right=rect[2], top=rect[3]).split())
            finally:
                pdf.close()
    except Exception:
        return ""


def _quads_rect(quads: list[float]) -> tuple[list[float], list[list[float]]]:
    lines = []
    for i in range(0, len(quads) - 7, 8):
        q = quads[i:i + 8]
        xs, ys = q[0::2], q[1::2]
        lines.append([min(xs), min(ys), max(xs), max(ys)])
    lines.sort(key=lambda r: (-(r[1] + r[3]) / 2, r[0]))
    if not lines:
        return [], []
    return [min(r[0] for r in lines), min(r[1] for r in lines), max(r[2] for r in lines), max(r[3] for r in lines)], lines


def compose(rec: dict, quote: str, replies: list[dict]) -> str:
    head = LABEL.get(rec["subtype"], rec["subtype"])
    lines = [f"{head}: “{quote}”" if quote else head]
    if rec["contents"]:
        lines.append(rec["contents"])
    for r in replies:
        who = r["author"] or "Reviewer"
        when = f" {r['date']}" if r["date"] else ""
        lines.append(f"Reply, {who}{when}: {r['contents']}" if r["contents"] else f"{who}{when}: {r['state'] or 'marked'}")
    if rec["date"]:
        lines.append(f"({rec['date']}, from a PDF comment)")
    else:
        lines.append("(from a PDF comment)")
    return "\n".join(lines)


def fingerprint(author: str, text: str) -> tuple:
    """Who, what kind of mark and the comment itself, without the quoted text (which moves with the layout), so a PDF imported twice adds nothing."""
    lines = text.split("\n")
    return (author or "PDF reviewer", lines[0].split(":")[0], tuple(lines[1:]))


async def import_comments(path: str) -> dict:
    p = Path(path).expanduser()
    if not p.is_file():
        raise CoreError(f"no file at {p}")
    try:
        annots, boxes = await asyncio.to_thread(read_annotations, str(p))
    except CoreError:
        raise
    except Exception as exc:
        raise CoreError(f"{p.name} could not be read as a PDF: {exc}") from exc
    st = await core.state()
    if st.get("active") is None:
        raise CoreError("no document open")
    lay = (await core.call("layout", source="review")).result
    pages = {q["abs"]: q for q in lay.get("pages") or []}
    existing = {fingerprint(n.get("author", ""), n["text"]) for n in await core.exec("note.list", {})}
    imported: list[dict] = []
    skipped: list[dict] = []
    plans: list[dict] = []

    by_parent: dict[int, list[dict]] = {}
    top: list[dict] = []
    for r in annots:
        if r.get("parent") is not None:
            by_parent.setdefault(r["parent"], []).append(r)
        else:
            top.append(r)

    def skip(r: dict, why: str) -> None:
        skipped.append({"page": r["page"] + 1, "kind": LABEL.get(r["subtype"], r["subtype"]), "author": r["author"], "reason": why,
                        "text": (r["contents"] or "")[:120]})

    for r in top:
        replies = by_parent.get(r["id"], [])
        if r["page"] not in pages:
            skip(r, f"the document has no page {r['page'] + 1}")
            continue
        pw = pages[r["page"]]["rect"]
        page_pt = (pw[2] - pw[0], pw[3] - pw[1])
        box = boxes[r["page"]]
        if r["rotate"] in (90, 270):
            box = (box[0], box[1], box[3], box[2])
        sub = r["subtype"]
        if not r["rect"] or len(r["rect"]) != 4:
            skip(r, "it has no position")
            continue
        rx = [min(r["rect"][0], r["rect"][2]), min(r["rect"][1], r["rect"][3]), max(r["rect"][0], r["rect"][2]), max(r["rect"][1], r["rect"][3])]
        span, quote = None, ""
        if sub in MARKUP:
            whole, lines = _quads_rect(r["quads"])
            if not lines:
                lines, whole = [rx], rx
            first, last = lines[0], lines[-1]
            a = _to_page(box, page_pt, first[0] + 0.8, (first[1] + first[3]) / 2)
            b = _to_page(box, page_pt, last[2] - 0.8, (last[1] + last[3]) / 2)
            span = (a[0], a[1], b[0], b[1])
            at_x, at_y = a
            pdf_quote = await asyncio.to_thread(_quote_pdf, str(p), r["page"], whole)
        else:
            if not r["contents"] and not replies and sub not in ("Caret",):
                skip(r, "it has no comment text")
                continue
            if sub == "Caret":
                at_x, at_y = _to_page(box, page_pt, rx[0], (rx[1] + rx[3]) / 2)
            else:
                at_x, at_y = _to_page(box, page_pt, rx[0] + 1, rx[3] - 1)
            pdf_quote = ""
        if sub in MARKUP and not r["contents"] and not replies and not pdf_quote:
            skip(r, "a mark with no comment and no readable text under it")
            continue
        where = await notes.locate(r["page"], at_x, at_y, span)
        if where is None:
            skip(r, "there is no text near it on that page")
            continue
        quote = where["quote"].replace("￼", "").strip() or pdf_quote
        text = compose(r, quote, replies)
        fp = fingerprint(r["author"], text)
        if fp in existing:
            skip(r, "already imported")
            continue
        existing.add(fp)
        plans.append({"r": r, "where": where, "text": text, "quote": quote, "replies": len(replies)})
    # A Note takes room on its line, which moves the text after it, so every position is found first, on the text as the PDF had it,
    # and the notes go in from the end of each story backwards so that no insertion shifts a position still to be used.
    for pl in sorted(plans, key=lambda x: (x["where"]["story"], -x["where"]["at"])):
        r, where = pl["r"], pl["where"]
        try:
            made = await notes.add_note(where["story"], where["at"], pl["text"], r["author"] or "PDF reviewer")
        except CoreError as exc:
            skip(r, str(exc))
            continue
        imported.append({"page": r["page"] + 1, "kind": LABEL.get(r["subtype"], r["subtype"]), "author": r["author"], "date": r["date"], "text": pl["text"],
                         "quote": pl["quote"], "story": made["story"], "id": made["id"], "at": made["at"], "snapped": where["snapped"], "replies": pl["replies"]})
    imported.sort(key=lambda x: (x["page"], x["id"]))
    return {"file": p.name, "pages": len(boxes), "found": len(top), "imported": imported, "skipped": skipped}
