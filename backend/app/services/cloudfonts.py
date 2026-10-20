"""Cloud Fonts: the Google Fonts library, read from the google/fonts repository on GitHub.

fonts.googleapis.com and the GitHub API tree are not reachable from every network, but raw.githubusercontent.com is, so the library is
read from files in the repository itself: tags/all/families.csv names every tagged family with its classification tags (the index),
and each family's folder (ofl/, apache/ or ufl/ by licence) holds METADATA.pb, the font files and the licence text. A family's folder is
found the first time it is needed, by asking for its METADATA.pb in each of the three licence folders.
"""
import asyncio
import csv
import json
import re
import shutil
import time
import urllib.parse
import urllib.request
from pathlib import Path

from app.core import config
from app.engine.core import CoreError, core

RAW = "https://raw.githubusercontent.com/google/fonts/main/"
INDEX_URL = RAW + "tags/all/families.csv"
FOLDERS = ("ofl", "apache", "ufl")
LICENCES = {"ofl": ("SIL Open Font License 1.1", "OFL.txt"), "apache": ("Apache License 2.0", "LICENSE.txt"), "ufl": ("Ubuntu Font Licence 1.0", "UFL.txt")}
CATEGORY_TAGS = {"Sans": "sans-serif", "Serif": "serif", "Slab": "serif", "Monospace": "monospace", "Script": "handwriting"}
CLOUD_DIR = config.DATA_DIR / "Cloud Fonts"
STATE = CLOUD_DIR / "activated.json"
DOCS = CLOUD_DIR / "documents.json"
CACHE = config.CACHE_DIR / "cloudfonts"
INDEX = CACHE / "index.json"
SLUG = re.compile(r"^[a-z0-9]{2,60}$")


class CloudFontError(ValueError):
    pass


def slug_of(name: str) -> str:
    return re.sub(r"[^a-z0-9]", "", name.lower())


def _get(url: str, timeout: int = 60) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "FormDesign"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.read()
    except urllib.error.HTTPError as exc:
        raise CloudFontError("not found" if exc.code == 404 else f"GitHub answered {exc.code}") from exc
    except OSError as exc:
        raise CloudFontError(f"could not reach GitHub: {exc}") from exc


def _read(path: Path, default):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return default


def _write(path: Path, data) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=1))


def build_index() -> dict:
    text = _get(INDEX_URL, 90).decode("utf-8", "replace")
    fams: dict[str, dict] = {}
    for parts in csv.reader(text.splitlines()):
        if len(parts) != 4:
            continue
        name, _axis, tag, _score = parts
        name = name.strip().strip('"')
        if not name or not tag.startswith("/"):
            continue
        f = fams.setdefault(name, {"name": name, "slug": slug_of(name), "category": "", "tags": []})
        seg = tag.strip("/").split("/")
        if seg[0] in CATEGORY_TAGS and not f["category"]:
            f["category"] = CATEGORY_TAGS[seg[0]]
        if seg[0] in ("Sans", "Serif", "Slab", "Script", "Monospace", "Theme", "Seasonal", "Purpose") and len(seg) > 1 and seg[1] not in f["tags"]:
            f["tags"].append(seg[1])
    out = []
    for f in fams.values():
        if not f["category"]:
            f["category"] = "display"
        out.append(f)
    out.sort(key=lambda f: f["name"].lower())
    data = {"fetched": time.time(), "source": INDEX_URL, "families": out}
    _write(INDEX, data)
    return data


def index(refresh: bool = False) -> dict:
    data = None if refresh else _read(INDEX, None)
    if not data or not data.get("families"):
        data = build_index()
    return data


def categories(fams: list[dict]) -> list[str]:
    return sorted({f["category"] for f in fams})


def listing(q: str = "", category: str = "", tag: str = "", state: str = "", limit: int = 80, offset: int = 0) -> dict:
    data = index()
    act = activated()
    q = q.strip().lower()
    rows = []
    for f in data["families"]:
        if category and f["category"] != category:
            continue
        if tag and tag not in f["tags"]:
            continue
        if q and q not in f["name"].lower() and not any(q in t.lower() for t in f["tags"]):
            continue
        on = f["slug"] in act
        if state == "active" and not on:
            continue
        if state == "inactive" and on:
            continue
        rows.append({**f, "active": on})
    tags = sorted({t for f in data["families"] for t in f["tags"]})
    return {"total": len(rows), "families": rows[offset:offset + limit], "fetched": data["fetched"], "categories": categories(data["families"]), "tags": tags,
            "count": len(data["families"])}


def parse_metadata(text: str) -> dict:
    def top(key: str) -> str:
        m = re.search(rf'^{key}: "(.*)"', text, re.M)
        return m.group(1) if m else ""
    files = []
    for blk in re.findall(r"^fonts \{(.*?)^\}", text, re.M | re.S):
        g = lambda k: (re.search(rf'{k}: "?([^"\n]*)"?', blk) or [None, ""])[1]
        files.append({"file": g("filename"), "style": g("style"), "weight": int(g("weight") or 400), "name": g("full_name")})
    axes = [{"tag": t, "min": float(a), "max": float(b)} for t, a, b in re.findall(r'axes \{\s*tag: "(\w+)"\s*min_value: ([\d.\-]+)\s*max_value: ([\d.\-]+)', text)]
    return {"name": top("name"), "designer": top("designer"), "category": top("category").lower().replace("_", "-"), "added": top("date_added"),
            "licenceCode": top("license"), "files": [f for f in files if f["file"]], "axes": axes, "subsets": re.findall(r'^subsets: "(.*)"', text, re.M)}


def detail(family: str) -> dict:
    slug = slug_of(family)
    if not SLUG.match(slug):
        raise CloudFontError("not a font family name")
    cached = _read(CACHE / "meta" / f"{slug}.json", None)
    if cached:
        return cached
    for folder in FOLDERS:
        try:
            raw = _get(f"{RAW}{folder}/{slug}/METADATA.pb", 30)
        except CloudFontError as exc:
            if str(exc) == "not found":
                continue
            raise
        d = parse_metadata(raw.decode("utf-8", "replace"))
        name, file = LICENCES[folder]
        d.update({"slug": slug, "folder": folder, "licence": name, "licenceFile": file, "licenceUrl": f"https://github.com/google/fonts/tree/main/{folder}/{slug}",
                  "family": d["name"] or family})
        _write(CACHE / "meta" / f"{slug}.json", d)
        return d
    raise CloudFontError(f"{family} is not in the Google Fonts repository")


def _url(d: dict, file: str) -> str:
    return f"{RAW}{d['folder']}/{d['slug']}/{urllib.parse.quote(file)}"


def preview_file(family: str) -> Path:
    d = detail(family)
    reg = [f for f in d["files"] if f["style"] == "normal"] or d["files"]
    if not reg:
        raise CloudFontError("this family lists no font files")
    pick = min(reg, key=lambda f: abs(f["weight"] - 400))
    dest = CACHE / "preview" / d["slug"] / pick["file"]
    if not dest.is_file():
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(_get(_url(d, pick["file"]), 120))
    return dest


def activated() -> dict:
    return _read(STATE, {})


async def activate(family: str, doc_key: str | None = None) -> dict:
    d = await asyncio.to_thread(detail, family)
    folder = CLOUD_DIR / d["slug"]
    folder.mkdir(parents=True, exist_ok=True)
    got = []
    for f in d["files"]:
        dest = folder / f["file"]
        if not dest.is_file():
            dest.write_bytes(await asyncio.to_thread(_get, _url(d, f["file"]), 180))
        got.append(f["file"])
    try:
        (folder / d["licenceFile"]).write_bytes(await asyncio.to_thread(_get, _url(d, d["licenceFile"]), 30))
    except CloudFontError:
        pass
    r = await core.call("fonts.add", {"dir": str(folder)})
    state = activated()
    state[d["slug"]] = {"family": d["family"], "slug": d["slug"], "licence": d["licence"], "licenceUrl": d["licenceUrl"], "designer": d["designer"], "files": got, "at": time.time()}
    _write(STATE, state)
    if doc_key:
        remember(doc_key, d["slug"])
    return {**state[d["slug"]], "faces": (r.result or {}).get("faces", 0)}


def deactivate(slug: str) -> dict:
    if not SLUG.match(slug):
        raise CloudFontError("not a cloud font")
    state = activated()
    if slug not in state:
        raise CloudFontError("that font is not active")
    shutil.rmtree(CLOUD_DIR / slug, ignore_errors=True)
    state.pop(slug)
    _write(STATE, state)
    # The layout core has no command to drop a font it has loaded, so the family stays in its list until the engine next starts.
    return {"removed": slug, "note": "The layout core keeps the font in this session; it is gone after the engine restarts."}


def remember(doc_key: str, slug: str) -> None:
    d = _read(DOCS, {})
    lst = d.setdefault(doc_key, [])
    if slug not in lst:
        lst.append(slug)
        _write(DOCS, d)


async def missing(doc_key: str) -> dict:
    """Families a document uses that the core cannot find, and the Google Fonts families that would supply them (or that the document remembers)."""
    r = await core.exec("font.list", {})
    used = [f["family"] for f in (r or {}).get("fonts", [])]
    avail = set((r or {}).get("families", []))
    lacking = [f for f in used if f not in avail]
    act = activated()
    if doc_key:
        for f in used:
            if slug_of(f) in act and f in avail:
                remember(doc_key, slug_of(f))
    if not lacking and not (doc_key and _read(DOCS, {}).get(doc_key)):
        return {"missing": [], "unavailable": []}
    fams = {f["slug"]: f for f in (await asyncio.to_thread(index))["families"]}
    out, seen = [], set()
    for name in lacking:
        s = slug_of(name)
        if s in fams and s not in seen:
            seen.add(s)
            out.append({"family": fams[s]["name"], "slug": s, "reason": "used"})
    for s in _read(DOCS, {}).get(doc_key, []):
        if s not in act and s not in seen and s in fams:
            seen.add(s)
            out.append({"family": fams[s]["name"], "slug": s, "reason": "remembered"})
    return {"missing": out, "unavailable": [f for f in lacking if slug_of(f) not in fams]}


def font_files(slug: str) -> Path:
    return CLOUD_DIR / slug
