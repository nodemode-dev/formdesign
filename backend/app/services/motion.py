import asyncio
import io
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

from app.engine.core import CoreError, core
from app.engine.documents import documents

PRESETS: dict[str, dict] = {
    "fadeIn": {"label": "Fade In", "enter": True},
    "fadeOut": {"label": "Fade Out", "enter": False},
    "flyInLeft": {"label": "Fly In From Left", "enter": True},
    "flyInRight": {"label": "Fly In From Right", "enter": True},
    "flyInTop": {"label": "Fly In From Top", "enter": True},
    "flyInBottom": {"label": "Fly In From Bottom", "enter": True},
    "grow": {"label": "Grow", "enter": True},
    "shrink": {"label": "Shrink", "enter": False},
    "spin": {"label": "Spin", "enter": False},
    "bounce": {"label": "Bounce", "enter": False},
    "pulse": {"label": "Pulse", "enter": False},
}
EVENTS = ("load", "click", "button", "hover")
SPEEDS = {"preset": None, "none": "linear", "easeIn": "ease-in", "easeOut": "ease-out", "easeInOut": "ease-in-out"}
PRESET_EASE = {"fadeIn": "ease", "fadeOut": "ease", "flyInLeft": "ease-out", "flyInRight": "ease-out", "flyInTop": "ease-out", "flyInBottom": "ease-out",
               "grow": "ease-out", "shrink": "ease-in", "spin": "ease-in-out", "bounce": "ease-in-out", "pulse": "ease-in-out"}


def _num(v, lo: float, hi: float, default):
    try:
        f = float(v)
    except (TypeError, ValueError):
        return default
    return min(hi, max(lo, f))


def clean(a: dict) -> dict:
    preset = a.get("preset")
    if preset not in PRESETS:
        raise CoreError(f"preset is one of {', '.join(PRESETS)}")
    event = a.get("event", "load")
    if event not in EVENTS:
        raise CoreError(f"event is one of {', '.join(EVENTS)}")
    speed = a.get("speed", "preset")
    if speed not in SPEEDS:
        raise CoreError(f"speed is one of {', '.join(SPEEDS)}")
    out = {"preset": preset, "event": event, "duration": _num(a.get("duration"), 0.05, 60, 1.0), "delay": _num(a.get("delay"), 0, 120, 0.0),
           "repeat": int(_num(a.get("repeat"), 1, 99, 1)), "loop": bool(a.get("loop", False)), "speed": speed,
           "opacity": None if a.get("opacity") in (None, "") else _num(a.get("opacity"), 0, 100, None),
           "scale": None if a.get("scale") in (None, "") else _num(a.get("scale"), 0, 1000, None),
           "rotate": None if a.get("rotate") in (None, "") else _num(a.get("rotate"), -3600, 3600, None),
           "hideBefore": bool(a.get("hideBefore", PRESETS[preset]["enter"]))}
    if event == "button":
        try:
            out["button"] = int(a.get("button"))
        except (TypeError, ValueError):
            raise CoreError("an animation on the click of a button needs `button`, the id of the button object") from None
    return out


def keyframes(a: dict, g: dict | None = None) -> list[dict]:
    p = a["preset"]
    op = None if a.get("opacity") is None else a["opacity"] / 100
    sc = None if a.get("scale") is None else a["scale"] / 100
    rot = a.get("rotate")
    w = max(1.0, (g or {}).get("w", 100.0))
    h = max(1.0, (g or {}).get("h", 100.0))
    pw, ph = (g or {}).get("pageW"), (g or {}).get("pageH")
    left, top = (g or {}).get("x", 0.0), (g or {}).get("y", 0.0)

    def rest(extra_scale: float = 1.0, extra_rot: float = 0.0) -> str:
        s = (sc if sc is not None else 1.0) * extra_scale
        r = (rot if rot is not None else 0.0) + extra_rot
        return f"translate(0%,0%) scale({s:g}) rotate({r:g}deg)"

    def away(dx: float, dy: float) -> str:
        return f"translate({dx:.1f}%,{dy:.1f}%)"

    end_op = 1.0 if op is None else op
    if p == "fadeIn":
        return [{"opacity": 0}, {"opacity": end_op, "transform": rest()}]
    if p == "fadeOut":
        return [{"opacity": 1}, {"opacity": 0.0 if op is None else op, "transform": rest()}]
    if p.startswith("flyIn"):
        dx = dy = 0.0
        if p == "flyInLeft":
            dx = -((left + w) / w * 100) if pw else -120.0
        elif p == "flyInRight":
            dx = ((pw - left) / w * 100) if pw else 120.0
        elif p == "flyInTop":
            dy = -((top + h) / h * 100) if ph else -120.0
        else:
            dy = ((ph - top) / h * 100) if ph else 120.0
        return [{"opacity": 1 if op is None else min(1, op), "transform": away(dx, dy)}, {"opacity": end_op, "transform": rest()}]
    if p == "grow":
        return [{"opacity": 0.0, "transform": "scale(0.1)"}, {"opacity": end_op, "transform": rest()}]
    if p == "shrink":
        return [{"opacity": 1, "transform": "scale(1)"}, {"opacity": end_op, "transform": f"scale({sc if sc is not None else 0.25:g}) rotate({rot or 0:g}deg)"}]
    if p == "spin":
        return [{"transform": "rotate(0deg)", "opacity": 1}, {"transform": f"rotate({rot if rot is not None else 360:g}deg)", "opacity": end_op}]
    if p == "bounce":
        return [{"transform": "translateY(0%)", "offset": 0}, {"transform": "translateY(-28%)", "offset": 0.3}, {"transform": "translateY(0%)", "offset": 0.55},
                {"transform": "translateY(-12%)", "offset": 0.78}, {"transform": "translateY(0%)", "offset": 1}]
    peak = sc if sc is not None else 1.15
    return [{"transform": "scale(1)", "offset": 0}, {"transform": f"scale({peak:g})", "offset": 0.5}, {"transform": "scale(1)", "offset": 1}]


def options(a: dict) -> dict:
    ease = SPEEDS[a["speed"]] or PRESET_EASE[a["preset"]]
    iters = "Infinity" if a.get("loop") else a.get("repeat", 1)
    return {"duration": round(a["duration"] * 1000), "delay": round(a["delay"] * 1000), "iterations": iters, "easing": ease, "fill": "both"}


def store(meta) -> dict:
    m = meta.motion
    m.setdefault("anims", {})
    m.setdefault("timing", {})
    m.setdefault("media", {})
    return m


async def geometry() -> dict[int, dict]:
    d = await core.exec("document.inspect")
    out: dict[int, dict] = {}
    for sp in d.get("spreads", []):
        pages = sp.get("pages", [])
        for it in sp.get("items", []):
            b = it["bounds"]
            cx, cy = (b[0] + b[2]) / 2, (b[1] + b[3]) / 2
            best, bd = None, 1e18
            for pg in pages:
                pb = pg["bounds"]
                dx = max(pb[0] - cx, 0, cx - pb[2])
                dy = max(pb[1] - cy, 0, cy - pb[3])
                if dx * dx + dy * dy < bd:
                    best, bd = pg, dx * dx + dy * dy
            if best is None:
                continue
            pb = best["bounds"]
            out[int(it["id"])] = {"id": int(it["id"]), "kind": it.get("kind"), "label": it.get("name") or it.get("label") or "", "story": it.get("story"), "page": best["abs"], "pageName": best["name"], "spread": sp["index"],
                                  "bounds": b, "x": b[0] - pb[0], "y": b[1] - pb[1], "w": b[2] - b[0], "h": b[3] - b[1], "pageW": pb[2] - pb[0], "pageH": pb[3] - pb[1],
                                  "ox": pb[0], "oy": pb[1], "hidden": bool(it.get("hidden")), "asset": it.get("asset")}
    return out


def normalise(meta, geo: dict[int, dict]) -> dict:
    m = store(meta)
    anims = {k: v for k, v in m["anims"].items() if int(k) in geo}
    m["anims"] = anims
    timing: dict[str, list[list[int]]] = {}
    by_page: dict[int, list[int]] = {}
    for k in anims:
        by_page.setdefault(geo[int(k)]["page"], []).append(int(k))
    for page, ids in sorted(by_page.items()):
        keep: list[list[int]] = []
        seen: set[int] = set()
        for grp in m["timing"].get(str(page), []):
            g = [i for i in grp if i in ids and i not in seen]
            seen.update(g)
            if g:
                keep.append(g)
        for i in sorted(ids, key=lambda x: (geo[x]["y"], geo[x]["x"])):
            if i not in seen:
                keep.append([i])
        timing[str(page)] = keep
    m["timing"] = timing
    return m


async def snapshot(item: int, scale: float = 2.0) -> bytes:
    from PIL import Image
    geo = await geometry()
    g = geo.get(item)
    if not g:
        raise CoreError(f"no object with id {item}")
    r = await core.call("page.png", {"page": g["page"], "scale": scale})
    im = Image.open(io.BytesIO(r.bin or b"")).convert("RGBA")
    box = (round(g["x"] * scale), round(g["y"] * scale), round((g["x"] + g["w"]) * scale), round((g["y"] + g["h"]) * scale))
    box = (max(0, box[0]), max(0, box[1]), min(im.width, max(box[0] + 1, box[2])), min(im.height, max(box[1] + 1, box[3])))
    buf = io.BytesIO()
    im.crop(box).save(buf, "PNG")
    return buf.getvalue()


async def page_layers(page: int, ids: list[int], scale: float) -> tuple[bytes, dict[int, bytes]]:
    """The page drawn with the animated objects hidden (the plate) and a picture of each object, so a fixed page can play them."""
    from PIL import Image
    geo = await geometry()
    shots: dict[int, bytes] = {}
    async with core.exclusive() as c:
        full = (await c.call("page.png", {"page": page, "scale": scale})).bin or b""
        im = Image.open(io.BytesIO(full)).convert("RGBA")
        for i in ids:
            g = geo[i]
            box = (max(0, round(g["x"] * scale)), max(0, round(g["y"] * scale)), min(im.width, round((g["x"] + g["w"]) * scale)), min(im.height, round((g["y"] + g["h"]) * scale)))
            if box[2] <= box[0] or box[3] <= box[1]:
                continue
            buf = io.BytesIO()
            im.crop(box).save(buf, "PNG")
            shots[i] = buf.getvalue()
        shown = [i for i in ids if not geo[i]["hidden"]]
        plate = full
        if shown:
            await c.call("exec", {"command": "object.setFlags", "params": {"hidden": True, "ids": shown}})
            try:
                plate = (await c.call("page.png", {"page": page, "scale": scale})).bin or full
            finally:
                await c.call("exec", {"command": "edit.undo", "params": {}})
    return plate, shots


async def export_config(meta, mode: str, page: int | None = None) -> dict | None:
    """What the page runtime needs: the keyframes of every animation, the groups in order, the trigger of each."""
    if not meta or not meta.motion.get("anims"):
        return None
    geo = await geometry()
    m = normalise(meta, geo)
    anims, groups = {}, []
    for k, a in m["anims"].items():
        g = geo[int(k)]
        if page is not None and g["page"] != page:
            continue
        anims[k] = {"kf": keyframes(a, g if mode == "page" else None), "opt": options(a), "event": a["event"], "button": a.get("button"), "hide": a.get("hideBefore", True)}
    for pg, grps in sorted(m["timing"].items(), key=lambda x: int(x[0])):
        if page is not None and int(pg) != page:
            continue
        groups.extend([g for g in grps if any(str(i) in anims for i in g)])
    if not anims:
        return None
    return {"mode": mode, "anims": anims, "groups": groups}


RUNTIME = """(()=>{const d=document.getElementById('fd-motion-data');if(!d)return;const C=JSON.parse(d.textContent);
const still=matchMedia('(prefers-reduced-motion: reduce)').matches;const els={};
document.querySelectorAll('[data-fd]').forEach(e=>{els[e.dataset.fd]=e});
const A={};
for(const id in C.anims){const e=els[id];if(!e)continue;const a=C.anims[id];const o=Object.assign({},a.opt);if(o.iterations==='Infinity')o.iterations=Infinity;
const an=e.animate(a.kf,o);an.pause();if(still){try{an.finish()}catch(x){an.cancel()}}A[id]={e,a,an,end:(o.delay||0)+(o.duration||0)*(isFinite(o.iterations)?o.iterations:1)};}
const play=id=>{const x=A[id];if(!x||still)return Promise.resolve();x.an.currentTime=0;x.an.play();return new Promise(r=>setTimeout(r,x.end))};
const seen=e=>new Promise(r=>{if(C.mode==='page'){r();return}const abs=getComputedStyle(e).position==='absolute';let s=e.parentNode;if(!abs){s=document.createElement('i');s.style.cssText='display:block;height:0;width:0';e.parentNode.insertBefore(s,e)}
const io=new IntersectionObserver(l=>{if(l.some(i=>i.isIntersecting)){io.disconnect();if(!abs)s.remove();r()}},{rootMargin:'0px 0px -10% 0px',threshold:abs?.2:0});io.observe(s)});
let chain=Promise.resolve();
for(const g of C.groups){const first=A[g[0]]||A[g.find(i=>A[i])];if(!first)continue;const ev=first.a.event;
if(ev==='load'){chain=chain.then(()=>seen(first.e)).then(()=>Promise.all(g.map(play)))}
else for(const id of g){const x=A[id];if(!x)continue;const t=x.a.event==='button'?els[x.a.button]:x.e;if(!t)continue;
if(x.a.event==='hover')t.addEventListener('mouseenter',()=>g.forEach(play));else{t.style.cursor='pointer';t.addEventListener('click',()=>g.forEach(play))}}}
})();"""


def safe_json(cfg: dict) -> str:
    return json.dumps(cfg, separators=(",", ":")).replace("<", "\\u003c").replace(">", "\\u003e").replace("&", "\\u0026")


def page_script(cfg: dict) -> str:
    data = safe_json(cfg)
    return f'<style>html{{overflow-x:clip}}</style><script type="application/json" id="fd-motion-data">{data}</script><script>{RUNTIME}</script>'


def has_ffmpeg() -> str | None:
    return shutil.which("ffmpeg")


async def poster_frame(data: bytes, ext: str, at: float) -> bytes | None:
    exe = has_ffmpeg()
    if not exe:
        return None

    def run() -> bytes | None:
        with tempfile.TemporaryDirectory() as td:
            src = Path(td) / f"in{ext or '.mp4'}"
            src.write_bytes(data)
            out = Path(td) / "poster.jpg"
            for t in (at, 0.0):
                subprocess.run([exe, "-loglevel", "error", "-y", "-ss", f"{t:g}", "-i", str(src), "-frames:v", "1", "-vf", "scale='min(1280,iw)':-2", "-q:v", "4", str(out)], capture_output=True, timeout=60)
                if out.is_file() and out.stat().st_size:
                    return out.read_bytes()
        return None
    return await asyncio.to_thread(run)


def media_extra(meta, item: int) -> dict:
    return dict(((meta.motion.get("media") if meta else None) or {}).get(str(item), {}))


async def media_for(meta, item: int) -> dict:
    """Options from the Interactive panel (play on page load, loop, controls) plus the poster settings."""
    try:
        o = await core.exec("media.get", {"id": item}) or {}
    except CoreError:
        o = {}
    x = media_extra(meta, item)
    return {"autoplay": bool(o.get("playOnPageLoad")), "loop": bool(o.get("loop")), "controls": bool(o.get("controls", True)), "poster": x.get("poster", "auto"), "posterTime": float(x.get("time", 1.0)),
            "posterFile": x.get("file")}


async def poster_bytes(opts: dict, data: bytes, ext: str) -> tuple[bytes, str] | None:
    mode = opts.get("poster", "auto")
    if mode == "none":
        return None
    if mode == "file" and opts.get("posterFile"):
        p = Path(str(opts["posterFile"])).expanduser()
        if p.is_file() and p.suffix.lower() in (".jpg", ".jpeg", ".png", ".webp"):
            from PIL import Image
            try:
                with Image.open(p) as im:
                    im.verify()
                return p.read_bytes(), p.suffix.lower()
            except (OSError, ValueError, SyntaxError):
                pass
    got = await poster_frame(data, ext, float(opts.get("posterTime") or 1.0))
    return (got, ".jpg") if got else None


def notify(what: str = "motion") -> None:
    core.announce({"type": "motion", "what": what})


async def active_meta():
    return await documents.meta_for_active()
