import zipfile
from pathlib import Path

from fastapi import APIRouter, HTTPException
from fastapi.responses import Response

from app.engine.documents import documents

router = APIRouter()


def _thumb(path: str) -> bytes | None:
    try:
        with zipfile.ZipFile(path) as z:
            return z.read("thumbs/page-1.png")
    except (OSError, KeyError, zipfile.BadZipFile):
        return None


@router.get("/home/thumb")
async def recent_thumb(path: str):
    if Path(path).suffix.lower() != ".fdes" or not any(i.get("path") == path for i in documents.recent()):
        raise HTTPException(404, "not a recent document")
    png = _thumb(path)
    if png is None:
        raise HTTPException(404, "no thumbnail")
    return Response(png, media_type="image/png", headers={"Cache-Control": "no-cache"})
