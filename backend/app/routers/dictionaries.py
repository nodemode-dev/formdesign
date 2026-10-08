from fastapi import APIRouter, Body, HTTPException

from app.services import dictionaries

router = APIRouter()


@router.get("/dictionaries")
async def listing():
    try:
        return await dictionaries.listing()
    except dictionaries.DictionaryError as exc:
        raise HTTPException(503, str(exc)) from exc


@router.post("/dictionaries/install")
async def install(body: dict = Body(...)):
    try:
        await dictionaries.install(str(body.get("code") or ""))
        return await dictionaries.listing()
    except dictionaries.DictionaryError as exc:
        raise HTTPException(422, str(exc)) from exc


@router.post("/dictionaries/add")
async def add(body: dict = Body(...)):
    try:
        await dictionaries.add_files(str(body.get("code") or ""), str(body.get("aff") or ""), str(body.get("dic") or ""))
        return await dictionaries.listing()
    except dictionaries.DictionaryError as exc:
        raise HTTPException(422, str(exc)) from exc


@router.delete("/dictionaries")
async def remove(code: str):
    try:
        await dictionaries.remove(code)
        return await dictionaries.listing()
    except dictionaries.DictionaryError as exc:
        raise HTTPException(404, str(exc)) from exc
