from fastapi import APIRouter, Body, HTTPException

from app.engine.core import CoreError
from app.services import libraries

router = APIRouter()


def _fail(exc: Exception) -> HTTPException:
    return HTTPException(422 if isinstance(exc, CoreError) else 404, str(exc))


@router.get("/libraries")
async def list_libraries():
    return libraries.listing()


@router.post("/libraries")
async def create_library(body: dict = Body(...)):
    try:
        return libraries.create(str(body.get("name") or ""))
    except libraries.LibraryError as exc:
        raise HTTPException(422, str(exc)) from exc


@router.delete("/libraries")
async def delete_library(name: str):
    try:
        libraries.delete(name)
    except libraries.LibraryError as exc:
        raise _fail(exc) from exc
    return {"ok": True}


@router.get("/libraries/items")
async def library_items(name: str):
    try:
        return {"name": name, "items": libraries.items(name)}
    except libraries.LibraryError as exc:
        raise _fail(exc) from exc


@router.post("/libraries/items")
async def add_item(body: dict = Body(...)):
    try:
        return await libraries.add(str(body.get("name") or ""), body.get("title"), str(body.get("description") or ""), body.get("ids"))
    except (libraries.LibraryError, CoreError) as exc:
        raise _fail(exc) from exc


@router.patch("/libraries/items")
async def update_item(body: dict = Body(...)):
    try:
        return libraries.update(str(body.get("name") or ""), int(body.get("index", -1)), body.get("title"), body.get("description"))
    except libraries.LibraryError as exc:
        raise _fail(exc) from exc


@router.delete("/libraries/items")
async def remove_item(name: str, index: int):
    try:
        libraries.remove(name, index)
    except libraries.LibraryError as exc:
        raise _fail(exc) from exc
    return {"ok": True}


@router.post("/libraries/place")
async def place_item(body: dict = Body(...)):
    try:
        return await libraries.place(str(body.get("name") or ""), int(body.get("index", -1)), body.get("x"), body.get("y"), body.get("spread"))
    except (libraries.LibraryError, CoreError) as exc:
        raise _fail(exc) from exc
