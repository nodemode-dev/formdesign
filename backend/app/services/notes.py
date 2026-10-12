"""Putting Notes at a page position.

The core has no command that turns a page point into a story offset, but its Type tool does exactly that on a click, so a page
point is mapped by clicking with the Type tool in a scratch pass and reading the caret that results. The person's tool and
selection are put back afterwards, so an import or a reviewer's comment never moves their cursor.
"""
import asyncio
from datetime import datetime

from app.engine.core import CoreError, core

_lock = asyncio.Lock()


def stamp(ts: float | None = None) -> str:
    return datetime.fromtimestamp(ts or datetime.now().timestamp()).strftime("%Y-%m-%d %H:%M")


async def _exec(cmd: str, params: dict | None = None):
    return await core.exec(cmd, params or {}, source="review")


async def page_geometry() -> dict:
    """Page rectangles on the canvas, and the spread-local bounds of every text item, by page."""
    lay = (await core.call("layout", source="review")).result
    ins = await core.inspect()
    pages = {p["abs"]: p for p in lay.get("pages") or []}
    items: dict[int, list[dict]] = {}
    for sp in ins.get("spreads") or []:
        plist = sp.get("pages") or []
        for it in sp.get("items") or []:
            if it.get("story") is None or it.get("hidden"):
                continue
            b = it["bounds"]
            cx = (b[0] + b[2]) / 2
            for p in plist:
                pb = p["bounds"]
                if pb[0] - 1 <= cx <= pb[2] + 1:
                    items.setdefault(p["abs"], []).append({"id": it["id"], "story": it["story"], "rect": [b[0] - pb[0], b[1] - pb[1], b[2] - pb[0], b[3] - pb[1]]})
                    break
    return {"pages": pages, "items": items}


async def story_text(story: int) -> str:
    return (await _exec("story.get", {"story": story})).get("text", "")


def _byte(text: str, n: int) -> int:
    return len(text[:n].encode())


async def snapshot() -> dict:
    st = await core.state()
    sel = st.get("selection") or {}
    return {"tool": st.get("tool"), "items": sel.get("items") or [], "text": sel.get("text")}


async def restore(snap: dict, inserted: tuple[int, int] | None = None) -> None:
    """Put back the tool and selection. `inserted` is (story, char offset) of a note added since, which shifts later offsets by one."""
    try:
        t = snap.get("text")
        if t:
            story = t["story"]
            a, f = t["anchor"], t["focus"]
            if inserted and inserted[0] == story:
                a += 1 if a >= inserted[1] else 0
                f += 1 if f >= inserted[1] else 0
            text = await story_text(story)
            await _exec("text.select", {"story": story, "anchor": _byte(text, a), "focus": _byte(text, f)})
        elif snap.get("items"):
            await _exec("selection.set", {"ids": snap["items"]})
        else:
            await _exec("edit.deselectAll")
        if snap.get("tool"):
            await core.call("tool", {"id": snap["tool"]}, source="review")
    except CoreError:
        pass


async def _click(x: float, y: float) -> dict | None:
    await _exec("edit.deselectAll")
    r = await core.call("pointer", {"tool": "type", "events": [{"kind": "down", "x": x, "y": y}, {"kind": "up", "x": x, "y": y}]}, source="review")
    t = (r.result.get("selection") or {}).get("text")
    return t if t and t.get("story") is not None else None


def _nearest(items: list[dict], x: float, y: float) -> tuple[dict, float, float] | None:
    best = None
    for it in items:
        r = it["rect"]
        cx = min(max(x, r[0] + 3), r[2] - 3) if r[2] - r[0] > 6 else (r[0] + r[2]) / 2
        cy = min(max(y, r[1] + 3), r[3] - 3) if r[3] - r[1] > 6 else (r[1] + r[3]) / 2
        d = (cx - x) ** 2 + (cy - y) ** 2
        if best is None or d < best[0]:
            best = (d, it, cx, cy)
    return (best[1], best[2], best[3]) if best else None


async def caret_at(geo: dict, page: int, x: float, y: float) -> dict | None:
    """The story and character offset under a point of a page (points from its top left), or the nearest text on that page."""
    p = geo["pages"].get(page)
    if p is None:
        return None
    ox, oy = p["rect"][0], p["rect"][1]
    hit = await _click(ox + x, oy + y)
    snapped = False
    if hit is None:
        near = _nearest(geo["items"].get(page, []), x, y)
        if near is None:
            return None
        hit = await _click(ox + near[1], oy + near[2])
        snapped = True
        if hit is None:
            return None
    return {"story": hit["story"], "frame": hit.get("frame"), "at": hit["focus"], "snapped": snapped}


async def locate(page: int, x: float, y: float, span: tuple[float, float, float, float] | None = None) -> dict | None:
    """Where a note for a comment at (x, y), or on the text between span's first point and second point, belongs.

    Returns {story, at, quote}: `at` is the character offset the note goes at (the end of the marked text).
    """
    async with _lock:
        snap = await snapshot()
        try:
            geo = await page_geometry()
            first = await caret_at(geo, page, span[0], span[1]) if span else await caret_at(geo, page, x, y)
            if first is None:
                return None
            at, quote = first["at"], ""
            if span:
                last = await caret_at(geo, page, span[2], span[3])
                if last and last["story"] == first["story"] and last["at"] != first["at"]:
                    a, b = sorted((first["at"], last["at"]))
                    text = await story_text(first["story"])
                    quote, at = text[a:b], b
            return {"story": first["story"], "at": at, "quote": quote.strip(), "snapped": first["snapped"]}
        finally:
            await restore(snap)


async def add_note(story: int, at: int, text: str, author: str) -> dict:
    """Insert a Note at a character offset of a story, keeping the person's selection where it was."""
    async with _lock:
        snap = await snapshot()
        try:
            body = await story_text(story)
            await _exec("text.select", {"story": story, "anchor": _byte(body, at), "focus": _byte(body, at)})
            r = await _exec("note.new", {"text": text, "author": author})
            return {"story": r["story"], "id": r["id"], "at": at}
        finally:
            await restore(snap, (story, at))
