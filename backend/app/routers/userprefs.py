from fastapi import APIRouter, Body, HTTPException

from app.services import userprefs

router = APIRouter()

RESERVED = {
    "workspaces": ("Essentials", "Advanced", "Book", "Digital Publishing", "Interactive for PDF", "Printing and Proofing", "Typography"),
    "shortcuts": ("Default",),
}


@router.post("/prefs/profiles")
async def add_profile(body: dict = Body(...)):
    try:
        return await userprefs.add_profile(str(body.get("path") or ""))
    except userprefs.PrefsError as exc:
        raise HTTPException(422, str(exc)) from exc


def _kind(kind: str) -> str:
    if kind not in userprefs.KINDS:
        raise HTTPException(404, f"unknown kind {kind}")
    return kind


@router.get("/prefs/{kind}")
async def listing(kind: str):
    return {"items": userprefs.listing(_kind(kind)), "builtin": list(RESERVED[kind])}


@router.post("/prefs/{kind}")
async def save(kind: str, body: dict = Body(...)):
    try:
        return userprefs.save(_kind(kind), body, RESERVED[kind])
    except userprefs.PrefsError as exc:
        raise HTTPException(422, str(exc)) from exc


@router.delete("/prefs/{kind}")
async def delete(kind: str, name: str):
    try:
        userprefs.delete(_kind(kind), name)
    except userprefs.PrefsError as exc:
        raise HTTPException(404, str(exc)) from exc
    return {"ok": True}
