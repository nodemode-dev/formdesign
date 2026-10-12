from pathlib import Path

from fastapi import APIRouter, Body, HTTPException, Request
from pydantic import BaseModel, Field

from app.engine.core import CoreError
from app.services import pdfcomments, wordimport
from app.services.review import MAX_NAME, MAX_TEXT, clean, reviews, same_origin, throttle

router = APIRouter()


def _local(request: Request) -> None:
    origin = request.headers.get("origin")
    if origin:
        from urllib.parse import urlparse
        host = urlparse(origin).hostname or ""
        if host not in ("localhost", "127.0.0.1", "::1", "tauri.localhost") and not origin.startswith("tauri://"):
            raise HTTPException(403, "reviews are managed only from this computer's FormDesign window")


def _base(request: Request) -> str:
    return str(request.base_url).rstrip("/")


def _owner_view(request: Request, rec: dict | None) -> dict:
    if rec is None:
        return {"shared": False, "comments": []}
    return {"shared": bool(rec.get("open")), "token": rec["token"], "slug": rec["slug"], "url": reviews.url(_base(request), rec), "title": rec.get("title"),
            "pages": rec.get("pages"), "updated": rec.get("updated"),
            "comments": [{**reviews.public(c), "placed": bool(c.get("placed")), "note": c.get("note")} for c in rec["comments"]]}


# ---------------------------------------------------------------- PDF comments

@router.post("/review/pdf-comments")
async def pdf_comments(request: Request, body: dict = Body(...)):
    _local(request)
    path = str(body.get("path") or "")
    if not path:
        raise HTTPException(400, "send {path: the annotated PDF}")
    try:
        return await pdfcomments.import_comments(path)
    except CoreError as exc:
        raise HTTPException(422, str(exc)) from exc


# ------------------------------------------------------------- owner: sharing

@router.get("/review")
async def status(request: Request):
    _local(request)
    return _owner_view(request, await reviews.for_active())


@router.post("/review/share")
async def share(request: Request):
    _local(request)
    try:
        rec = await reviews.share()
    except CoreError as exc:
        raise HTTPException(422, str(exc)) from exc
    return _owner_view(request, rec)


@router.post("/review/stop")
async def stop(request: Request):
    _local(request)
    if not await reviews.stop():
        raise HTTPException(404, "this document is not shared for review")
    return {"ok": True}


class Reply(BaseModel):
    text: str = Field(max_length=MAX_TEXT)
    name: str = Field(default="", max_length=MAX_NAME)


@router.post("/review/comments/{cid}/reply")
async def owner_reply(cid: int, request: Request, body: Reply):
    _local(request)
    rec = await reviews.for_active()
    text = clean(body.text, MAX_TEXT)
    if rec is None or not text:
        raise HTTPException(404 if rec is None else 400, "no review is open for this document" if rec is None else "write a reply")
    try:
        c = await reviews.reply(rec, cid, clean(body.name, MAX_NAME), text, True)
    except CoreError as exc:
        raise HTTPException(404, str(exc)) from exc
    return reviews.public(c)


@router.post("/review/comments/{cid}/resolve")
async def owner_resolve(cid: int, request: Request, body: dict = Body(default={})):
    _local(request)
    rec = await reviews.for_active()
    if rec is None:
        raise HTTPException(404, "no review is open for this document")
    try:
        c = await reviews.resolve(rec, cid, bool(body.get("resolved", True)))
    except CoreError as exc:
        raise HTTPException(404, str(exc)) from exc
    return reviews.public(c)


@router.post("/review/comments/{cid}/delete")
async def owner_delete(cid: int, request: Request):
    _local(request)
    rec = await reviews.for_active()
    if rec is None or not await reviews.remove(rec, cid):
        raise HTTPException(404, "no such comment")
    return {"ok": True}


# ------------------------------------------------------------ public: reviewers

def _public(request: Request, token: str) -> dict:
    if not same_origin(request.headers.get("origin"), request.headers.get("host")):
        raise HTTPException(403, "comments are accepted only from the review page itself")
    rec = reviews.get(token)
    if rec is None or not rec.get("open"):
        raise HTTPException(404, "this review is not available")
    return rec


class NewComment(BaseModel):
    name: str = Field(max_length=MAX_NAME)
    text: str = Field(max_length=MAX_TEXT)
    page: int = Field(ge=0, le=5000)
    x: float = Field(ge=0, le=1)
    y: float = Field(ge=0, le=1)


@router.get("/review/public/{token}/comments")
async def public_list(token: str, request: Request):
    rec = _public(request, token)
    return {"comments": [reviews.public(c) for c in rec["comments"]]}


@router.post("/review/public/{token}/comments")
async def public_post(token: str, request: Request, body: NewComment):
    rec = _public(request, token)
    if not throttle(token):
        raise HTTPException(429, "too many comments at once; wait a minute")
    name, text = clean(body.name, MAX_NAME), clean(body.text, MAX_TEXT)
    if not name or not text:
        raise HTTPException(400, "a name and a comment are needed")
    try:
        c = await reviews.post(rec, name, text, body.page, body.x, body.y)
    except CoreError as exc:
        raise HTTPException(422, str(exc)) from exc
    return reviews.public(c)


@router.post("/review/public/{token}/comments/{cid}/replies")
async def public_reply(token: str, cid: int, request: Request, body: Reply):
    rec = _public(request, token)
    if not throttle(token):
        raise HTTPException(429, "too many comments at once; wait a minute")
    name, text = clean(body.name, MAX_NAME), clean(body.text, MAX_TEXT)
    if not name or not text:
        raise HTTPException(400, "a name and a reply are needed")
    try:
        c = await reviews.reply(rec, cid, name, text, False)
    except CoreError as exc:
        raise HTTPException(404, str(exc)) from exc
    return reviews.public(c)


# --------------------------------------------------------- Word import options

@router.post("/place/word/scan")
async def word_scan(request: Request, body: dict = Body(...)):
    _local(request)
    try:
        found = wordimport.scan(str(body.get("path") or ""))
        known = await wordimport.known_styles()
        memory = await wordimport.recall()
    except CoreError as exc:
        raise HTTPException(422, str(exc)) from exc
    return {**found, "document": {k: sorted(v, key=str.lower) for k, v in known.items()}, "remembered": memory,
            "defaults": wordimport.DEFAULTS}


@router.post("/place/word")
async def word_place(request: Request, body: dict = Body(...)):
    _local(request)
    path = str(body.get("path") or "")
    try:
        return await wordimport.place(path, body.get("styleMap") or {}, body.get("options") or {}, body.get("frame"))
    except CoreError as exc:
        raise HTTPException(422, str(exc)) from exc


@router.post("/place/word/finish")
async def word_finish(request: Request):
    _local(request)
    try:
        return await wordimport.finish()
    except CoreError as exc:
        raise HTTPException(422, str(exc)) from exc
