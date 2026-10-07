import io
from pathlib import Path

from PIL import Image, ImageFilter, ImageOps, ImageSequence, ImageStat

CORE_FORMATS = {"PNG": "image/png", "JPEG": "image/jpeg", "WEBP": "image/webp", "GIF": "image/gif", "TIFF": "image/tiff", "BMP": "image/bmp"}
IMAGE_EXTS = {".png", ".jpg", ".jpeg", ".webp", ".gif", ".tif", ".tiff", ".bmp", ".avif", ".heic", ".heif"}

Image.MAX_IMAGE_PIXELS = 400_000_000


def hex_of(rgb) -> str:
    return "#%02x%02x%02x" % tuple(int(c) for c in rgb[:3])


def palette(im: Image.Image, n: int = 5) -> list[str]:
    small = im.convert("RGB")
    small.thumbnail((96, 96))
    q = small.quantize(colors=n, method=Image.Quantize.MEDIANCUT)
    pal = q.getpalette() or []
    counts = sorted(q.getcolors() or [], reverse=True)
    out = []
    for count, idx in counts[:n]:
        rgb = pal[idx * 3: idx * 3 + 3]
        if len(rgb) == 3:
            out.append(hex_of(rgb))
    return out


def focal_point(im: Image.Image) -> list[float]:
    g = ImageOps.grayscale(im)
    g.thumbnail((128, 128))
    e = g.filter(ImageFilter.FIND_EDGES)
    w, h = e.size
    px = e.load()
    tx = ty = tw = 0.0
    for y in range(h):
        for x in range(w):
            v = px[x, y]
            if v > 24:
                tx += x * v
                ty += y * v
                tw += v
    if tw <= 0:
        return [0.5, 0.45]
    fx, fy = tx / tw / max(1, w - 1), ty / tw / max(1, h - 1)
    return [round(0.5 + (fx - 0.5) * 0.8, 3), round(0.45 + (fy - 0.45) * 0.8, 3)]


def role_hint(name: str, w: int, h: int, alpha: bool, colors: list[str]) -> str:
    n = name.lower()
    for key, role in (("logo", "logo"), ("icon", "logo"), ("headshot", "portrait"), ("portrait", "portrait"), ("avatar", "portrait"),
                      ("author", "portrait"), ("chart", "chart"), ("graph", "chart"), ("diagram", "chart"), ("product", "product"),
                      ("hero", "hero"), ("cover", "hero"), ("banner", "hero"), ("texture", "texture"), ("pattern", "texture"), ("bg", "texture")):
        if key in n:
            return role
    if alpha and max(w, h) <= 1600:
        return "logo"
    if max(w, h) >= 1600 and w >= h:
        return "hero"
    return "photo"


def describe(path: Path, ref: str, source: str) -> dict:
    with Image.open(path) as im:
        fmt = im.format or ""
        animated = bool(getattr(im, "is_animated", False))
        frames = getattr(im, "n_frames", 1)
        first = next(ImageSequence.Iterator(im)).copy() if animated else im.copy()
    w, h = first.size
    alpha = first.mode in ("RGBA", "LA", "PA") or (first.mode == "P" and "transparency" in first.info)
    rgb = first.convert("RGB")
    stat = ImageStat.Stat(ImageOps.grayscale(rgb))
    colors = palette(rgb)
    aspect = round(w / h, 4) if h else 1.0
    orientation = "square" if 0.9 <= aspect <= 1.1 else ("landscape" if aspect > 1 else "portrait")
    return {
        "id": ref, "name": path.name, "path": str(path), "width": w, "height": h, "aspect": aspect, "orientation": orientation,
        "mime": CORE_FORMATS.get(fmt, "image/png"), "format": fmt.lower(), "animated": animated, "frames": frames if animated else 1,
        "alpha": alpha, "colors": colors, "brightness": round(stat.mean[0] / 255, 3), "focal": focal_point(rgb),
        "megapixels": round(w * h / 1e6, 2), "source": source, "role": role_hint(path.name, w, h, alpha, colors),
    }


def normalize(path: Path, dest_dir: Path) -> Path:
    with Image.open(path) as im:
        fmt = im.format or ""
        if fmt in CORE_FORMATS and fmt not in ("TIFF", "BMP"):
            return path
        out = dest_dir / (path.stem + ".png")
        frame = next(ImageSequence.Iterator(im)).copy()
        frame.save(out, "PNG")
        return out


def save_bytes(data: bytes, dest: Path) -> Path:
    with Image.open(io.BytesIO(data)) as im:
        fmt = im.format or "PNG"
        ext = {"JPEG": ".jpg", "PNG": ".png", "WEBP": ".webp", "GIF": ".gif"}.get(fmt, ".png")
        target = dest.with_suffix(ext)
        if fmt in ("JPEG", "PNG", "WEBP", "GIF"):
            target.write_bytes(data)
        else:
            im.convert("RGBA").save(target.with_suffix(".png"), "PNG")
            target = target.with_suffix(".png")
    return target
