from fastapi import APIRouter, Body, HTTPException

from app.services import profiles

router = APIRouter()


@router.get("/preflight/profiles")
async def list_profiles():
    return {"profiles": profiles.listing()}


@router.post("/preflight/profiles")
async def save_profile(body: dict = Body(...)):
    try:
        return profiles.save(body)
    except profiles.ProfileError as exc:
        raise HTTPException(422, str(exc)) from exc


@router.delete("/preflight/profiles")
async def delete_profile(name: str):
    try:
        profiles.delete(name)
    except profiles.ProfileError as exc:
        raise HTTPException(404, str(exc)) from exc
    return {"ok": True}
