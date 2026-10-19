from pathlib import Path

from fastapi import APIRouter, HTTPException
from PIL import Image
from pydantic import BaseModel

from app.intake.images import palette

router = APIRouter()


class FromImage(BaseModel):
    path: str
    count: int = 6


@router.post("/colour/theme")
async def colour_theme(body: FromImage):
    """A colour theme picked from an image: the image's dominant colours, darkest first."""
    p = Path(body.path).expanduser()
    if not p.is_absolute() or not p.is_file():
        raise HTTPException(400, f"no image at {body.path}")
    try:
        with Image.open(p) as im:
            im.seek(0)
            colours = palette(im.convert("RGB"), max(2, min(12, body.count)))
    except (OSError, ValueError) as e:
        raise HTTPException(400, f"cannot read the image: {e}")
    lum = lambda h: 0.2126 * int(h[1:3], 16) + 0.7152 * int(h[3:5], 16) + 0.0722 * int(h[5:7], 16)
    return {"colours": sorted(colours, key=lum)}
