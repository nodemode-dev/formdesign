from pathlib import Path

from fastapi import APIRouter, Body, HTTPException, Query
from fastapi.responses import Response

from app.services import books

router = APIRouter()


def _fail(exc: Exception) -> HTTPException:
    return HTTPException(422, str(exc))


@router.get("/books")
async def list_books():
    return {"books": books.all_books(), "documents": books.project_documents()}


@router.post("/books")
async def create_book(body: dict = Body(...)):
    path = books.create(str(body.get("name") or "Book"))
    return await books.info(path)


@router.get("/books/info")
async def book_info(path: str = Query(...)):
    try:
        return await books.info(path)
    except books.BookError as exc:
        raise _fail(exc) from exc


@router.post("/books/edit")
async def edit_book(body: dict = Body(...)):
    path = str(body.get("path") or "")
    op = body.get("op")
    try:
        if op == "add":
            books.add(path, str(body.get("doc") or ""), body.get("at"))
        elif op == "remove":
            books.remove(path, int(body.get("index", -1)))
        elif op == "move":
            books.move(path, int(body.get("index", -1)), int(body.get("to", 0)))
        elif op == "source":
            books.set_source(path, int(body.get("index", -1)))
        else:
            raise books.BookError(f"unknown book operation {op}")
        return await books.info(path)
    except books.BookError as exc:
        raise _fail(exc) from exc


@router.post("/books/paginate")
async def paginate(body: dict = Body(...)):
    try:
        return await books.run(str(body.get("path") or ""), [("book.paginate", {})], write_back=True)
    except books.BookError as exc:
        raise _fail(exc) from exc


@router.post("/books/sync")
async def sync(body: dict = Body(...)):
    try:
        return await books.run(str(body.get("path") or ""), [("book.syncStyles", {})], write_back=True)
    except books.BookError as exc:
        raise _fail(exc) from exc


@router.get("/books/export.pdf")
async def export(path: str = Query(...)):
    try:
        _, out = await books.export_pdf(path)
    except books.BookError as exc:
        raise _fail(exc) from exc
    return Response(out.read_bytes(), media_type="application/pdf",
                    headers={"Content-Disposition": f'attachment; filename="{Path(out).name}"', "Cache-Control": "no-store"})
