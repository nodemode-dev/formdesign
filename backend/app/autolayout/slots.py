from pathlib import Path

from app.autolayout.content import Content
from app.autolayout.fill import image_fill, text_fill
from app.engine.core import Core, CoreError
from app.intake import blocks as B
from app.intake import markdown as MD


async def markdown_paras(core: Core, md: str, source: str) -> list[dict]:
    _, blocks = MD.parse(md, "slot", lambda src, alt: None)
    ins = await core.inspect()
    names = [s if isinstance(s, str) else s.get("name") for s in ins.get("paragraphStyles") or []]
    art = B.article_from_blocks(blocks, "slot", "a1", {"title": "-"})
    art["blocks"] = blocks
    c = Content({"articles": [art], "brief": {}})
    return c.flow({"styleNames": names}, "Body", "print")


async def list_slots(core: Core, source: str = "engine") -> list[dict]:
    r = await core.call("slots", {}, source=source)
    out = (r.result or {}).get("slots", [])
    f = await core.call("slots", {"prefix": "flow:"}, source=source)
    return out + [{**s, "slot": f"flow:{s['slot']}"} for s in (f.result or {}).get("slots", [])]


def compact(slots: list[dict]) -> list[dict]:
    seen: dict[str, dict] = {}
    for s in slots:
        name = s["slot"]
        if name in seen:
            seen[name]["frames"] = seen[name].get("frames", 1) + 1
            continue
        out = {"slot": name, "kind": s.get("kind"), "page": (s.get("page") or 0) + 1}
        if s.get("kind") == "text":
            out["text"] = (s.get("text") or "")[:80]
        elif s.get("image"):
            out["image"] = Path(s["image"]).name
        seen[name] = out
    return list(seen.values())


def image_path(value: str, pack: dict | None) -> str | None:
    if pack:
        for img in pack.get("images") or []:
            if value in (img["id"], img.get("name")):
                return img["path"]
    p = Path(value).expanduser()
    return str(p) if p.is_file() else None


async def set_slots(core: Core, values: dict, pack: dict | None = None, source: str = "engine") -> dict:
    slots = await list_slots(core, source)
    by: dict[str, list[dict]] = {}
    for s in slots:
        by.setdefault(s["slot"], []).append(s)
    done, missing, errors, overset = [], [], [], []
    for name, val in values.items():
        recs = by.get(name)
        if not recs:
            missing.append(name)
            continue
        for rec in recs:
            try:
                if rec.get("kind") == "image" or (rec.get("kind") == "frame" and isinstance(val, str) and image_path(val, pack)):
                    path = image_path(str(val), pack)
                    if not path:
                        errors.append(f"{name}: no image {val}")
                        break
                    await core.batch(image_fill(rec["id"], path, "fit" if "logo" in name else "fill"), source=source)
                elif rec.get("kind") == "text" and name.startswith("flow:") and isinstance(val, str):
                    if rec is recs[0]:
                        await core.batch(text_fill(rec["id"], await markdown_paras(core, val, source), name[5:]), source=source)
                elif rec.get("kind") == "text":
                    text = "\n".join(str(v) for v in val) if isinstance(val, list) else str(val)
                    await core.call("exec", {"command": "story.setText", "params": {"story": rec["story"], "text": text}}, source=source)
                    info = (await core.call("exec", {"command": "story.get", "params": {"frame": rec["id"]}}, source=source)).result or {}
                    if info.get("overset") and rec["slot"] not in overset:
                        overset.append(rec["slot"])
            except CoreError as exc:
                errors.append(f"{name}: {str(exc)[:120]}")
                break
        else:
            done.append(name)
    out = {"set": done}
    if missing:
        out["missing"] = missing
        out["available"] = sorted(by)[:60]
    if errors:
        out["errors"] = errors
    if overset:
        out["overset"] = overset
    return out
