from fastapi import APIRouter, Body, HTTPException

from app.services import queries

router = APIRouter()


@router.get("/find/queries")
async def list_queries():
    return {"queries": queries.listing()}


@router.post("/find/queries")
async def save_query(body: dict = Body(...)):
    try:
        return queries.save(body)
    except queries.QueryError as exc:
        raise HTTPException(422, str(exc)) from exc


@router.delete("/find/queries")
async def delete_query(name: str):
    try:
        queries.delete(name)
    except queries.QueryError as exc:
        raise HTTPException(404, str(exc)) from exc
    return {"ok": True}
