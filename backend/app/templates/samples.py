import hashlib
import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

from app.core import config

PARAGRAPHS = [
    "Every page begins as an open field. The grid arrives first: margins that frame the reading area, columns that set the rhythm, and a baseline that keeps each line in step with its neighbours across the spread.",
    "Good layout is quiet. It lets a reader move through a story without noticing the structure carrying them, the way a well-made chair holds you without asking to be admired.",
    "Type does most of the work. A paragraph composer weighs every possible break at once, trading a slightly loose line here for a much better one three lines later, so the texture of the page stays even.",
    "Pictures give a spread its pulse. A strong photograph bleeds off the edge of the page while smaller ones sit inside the grid and let the text wrap around them, each with a caption close at hand.",
    "Colour holds it together. A restrained palette of two or three swatches, applied consistently to headlines, rules and backgrounds, gives a publication a voice the reader learns to recognise.",
    "Styles are the secret to working fast. Change the body style once and every page follows, from the opening spread to the last folio, without a single paragraph touched by hand.",
    "When the layout is done it has to travel: to a printer as a press-ready file with bleed and marks, to a colleague as an editable package, or to a screen as a page that reflows for any width.",
]

HEADLINES = {"title": "The Quiet Art of Layout", "subtitle": "How grids, type and colour turn words and pictures into pages people want to read",
             "kicker": "Feature", "byline": "By Mara Ellison", "date": "Spring 2026", "pullquote": "The best layouts disappear. What remains is the story.",
             "quote": "The best layouts disappear. What remains is the story.", "cta": "Read the issue", "url": "formdes.app", "brand": "Northlight",
             "tagline": "Stories worth slowing down for", "price": "$24", "caption": "Evening light over layered hills.", "credit": "Photograph: Studio archive",
             "name": "Alex Morgan", "role": "Creative Director", "phone": "+1 555 0142", "email": "hello@example.com", "address": "12 Harbour Street, Portland",
             "location": "Harbour Hall, Portland", "time": "Saturday 7 pm", "intro": PARAGRAPHS[0], "summary": PARAGRAPHS[1], "issue": "No. 01",
             "section": "Design", "label": "New", "stat": "38%", "statLabel": "growth since 2020", "footer": "formdes.app", "nav": "Home    Stories    About    Subscribe"}


def sample_text(slot: str, item_sample) -> str | list[str]:
    if isinstance(item_sample, (str, list)) and item_sample:
        return item_sample
    base = slot.split(".")[-1].rstrip("0123456789")
    if base in ("body", "text", "story", "intro", "summary", "description", "bio", "details"):
        return PARAGRAPHS[:4] if base in ("body", "story", "text") else PARAGRAPHS[0]
    return HEADLINES.get(base, base.replace("_", " ").title())


def _rgb(hx: str) -> tuple[int, int, int]:
    hx = hx.lstrip("#")
    return int(hx[0:2], 16), int(hx[2:4], 16), int(hx[4:6], 16)


def _mix(a: tuple, b: tuple, t: float) -> tuple[int, int, int]:
    return tuple(int(a[k] + (b[k] - a[k]) * t) for k in range(3))


def _art(w: int, h: int, colors: list[str], seed: int) -> Image.Image:
    c = [_rgb(x) for x in colors] + [(240, 200, 150)] * 4
    dark, accent, accent2, muted = c[0], c[1], c[2], c[3]
    sky_top = _mix(dark, accent2, 0.35)
    sky_low = _mix(accent, (255, 236, 214), 0.35)
    grad = Image.linear_gradient("L").resize((w, h))
    img = Image.composite(Image.new("RGB", (w, h), sky_low), Image.new("RGB", (w, h), sky_top), grad)
    glow = Image.new("L", (w, h), 0)
    sx, sy, sr = w * (0.6 + 0.12 * math.sin(seed * 1.7)), h * 0.42, min(w, h) * 0.16
    ImageDraw.Draw(glow).ellipse([sx - sr * 2.4, sy - sr * 2.4, sx + sr * 2.4, sy + sr * 2.4], fill=110)
    glow = glow.filter(ImageFilter.GaussianBlur(sr))
    img = Image.composite(Image.new("RGB", (w, h), (255, 240, 220)), img, glow)
    ss = 2
    layer = Image.new("RGBA", (w * ss, h * ss), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    d.ellipse([(sx - sr) * ss, (sy - sr) * ss, (sx + sr) * ss, (sy + sr) * ss], fill=(*_mix((255, 244, 226), accent, 0.15), 255))
    tones = [_mix(muted, dark, 0.15), _mix(accent2, dark, 0.45), _mix(dark, (0, 0, 0), 0.25)]
    for k, (amp, base, col) in enumerate([(0.05, 0.6, tones[0]), (0.07, 0.71, tones[1]), (0.045, 0.83, tones[2])]):
        pts = [(0, h * ss)]
        for i in range(0, 129):
            fx = i / 128
            y = base + amp * (math.sin(fx * (5 + k * 3) + k + seed) + 0.5 * math.sin(fx * 13 + seed * 0.7))
            pts.append((fx * w * ss, y * h * ss))
        pts.append((w * ss, h * ss))
        d.polygon(pts, fill=(*col, 255))
    layer = layer.resize((w, h), Image.Resampling.LANCZOS)
    img.paste(layer, (0, 0), layer)
    return img


def placeholder(palette: dict, orientation: str = "landscape", n: int = 0, size: tuple[int, int] | None = None) -> Path:
    colors = [palette.get(k, "#888888") for k in ("dark", "accent", "accent2", "muted")]
    w, h = size or {"landscape": (2400, 1500), "portrait": (1500, 2100), "square": (1800, 1800)}.get(orientation, (2400, 1500))
    key = hashlib.sha1(f"{colors}{w}{h}{n}".encode()).hexdigest()[:12]
    out = config.CACHE_DIR / "placeholders" / f"ph-{key}.jpg"
    if not out.is_file():
        out.parent.mkdir(parents=True, exist_ok=True)
        _art(w, h, colors, n).save(out, "JPEG", quality=84)
    return out


def logo_placeholder(palette: dict) -> Path:
    key = hashlib.sha1(f"logo{palette.get('accent')}{palette.get('ink')}".encode()).hexdigest()[:12]
    out = config.CACHE_DIR / "placeholders" / f"logo-{key}.png"
    if not out.is_file():
        out.parent.mkdir(parents=True, exist_ok=True)
        im = Image.new("RGBA", (600, 600), (0, 0, 0, 0))
        d = ImageDraw.Draw(im)
        acc = palette.get("accent", "#f97316")
        ink = palette.get("ink", "#111111")
        d.ellipse([60, 60, 540, 540], fill=acc)
        d.rounded_rectangle([210, 150, 390, 450], radius=40, fill=ink)
        im.save(out, "PNG")
    return out
