import argparse
import asyncio
import base64
import json
import os
import sys
from pathlib import Path

from app.autolayout import make as M
from app.autolayout import rank as R
from app.core import config
from app.engine.core import CoreError, core
from app.engine.documents import documents
from app.intake import pack as P
from app.intake.store import packs
from app.templates import brands, registry, spec

CWD = Path(os.environ.get("FORMDES_CWD") or os.getcwd())


def here(p: str) -> Path:
    q = Path(p).expanduser()
    return q if q.is_absolute() else (CWD / q)


def out(data, as_json: bool) -> None:
    if as_json:
        print(json.dumps(data, indent=1, ensure_ascii=False))
    elif isinstance(data, str):
        print(data)
    else:
        print(json.dumps(data, ensure_ascii=False))


def cmd_templates(a) -> int:
    if a.id:
        t = registry.get(a.id)
        if t is None:
            print(f"no template {a.id}", file=sys.stderr)
            return 1
        out({"card": spec.card(t), "spec": {k: v for k, v in t.items() if not k.startswith("_")}} if a.json else spec.card(t), a.json)
        return 0
    cards = registry.cards(a.category, a.kind, a.q, a.medium)
    if a.json:
        out(cards, True)
        return 0
    for c in cards:
        size = f"{c['size'][0]:g}x{c['size'][1]:g}{c['unit']}"
        print(f"{c['id']:<34} {c['category']:<12} {size:<14} {c['pages']}p  {', '.join(c['kinds'][:3])}")
    print(f"{len(cards)} templates")
    return 0


async def ingest(a) -> dict:
    brief = a.brief or ""
    if a.brief_file:
        brief = (brief + "\n" + here(a.brief_file).read_text(encoding="utf-8")).strip()
    opts = json.loads(a.options) if a.options else {}
    return P.ingest([here(f) for f in a.files], brief, opts)


async def save_outputs(paths: list[str], stem: str) -> list[dict]:
    saved = []
    for o in paths:
        p = here(o)
        if p.is_dir() or o.endswith(("/", os.sep)):
            p.mkdir(parents=True, exist_ok=True)
            p = p / f"{stem}.fdes"
        ext = p.suffix.lower().lstrip(".")
        p.parent.mkdir(parents=True, exist_ok=True)
        if ext == "fdes":
            saved.append(await documents.save(str(p), source="cli"))
        elif ext in ("png", "jpg", "jpeg"):
            ins = await core.inspect()
            n = ins.get("pageCount") or 1
            for i in range(n):
                target = p if n == 1 else p.with_name(f"{p.stem}-{i + 1}{p.suffix}")
                res, _, _ = await documents.export(ext, str(target), {"page": i, "scale": 2.0})
                saved.append(res)
        else:
            res, _, _ = await documents.export({"htm": "html"}.get(ext, ext), str(p))
            saved.append({"path": str(p), **({k: v for k, v in (res or {}).items() if k in ("pages", "bytes")} if isinstance(res, dict) else {})})
    return saved


async def cmd_make(a) -> int:
    pack = await ingest(a)
    await core.start()
    try:
        brand = brands.resolve(a.brand) if a.brand else None
        picks = [a.template] if a.template else []
        if a.variations > 1 or not picks:
            picks += [r["id"] for r in R.rank(pack, limit=a.variations + 1, category=a.category) if r["id"] not in picks]
        picks = picks[:max(1, a.variations)]
        reports = []
        for i, tid in enumerate(picks):
            rep = await M.make(core, pack, tid, brand=brand, fills=json.loads(a.fills) if a.fills else None,
                               options={"title": a.title, "placeholders": not a.no_placeholders}, source="cli")
            rep.pop("slots", None)
            await documents.adopt(template={"id": tid}, brand=brand, brief=(pack.get("brief") or {}).get("text", ""), pack_id=pack["id"])
            if a.out:
                stem = f"{Path(pack.get('title') or tid).stem[:40] or tid}-{tid}" if len(picks) > 1 else (pack.get("title") or tid)[:60]
                outs = a.out if len(picks) == 1 else [str(here(o).with_name(f"{here(o).stem}-{i + 1}{here(o).suffix}")) if Path(o).suffix else o for o in a.out]
                rep["saved"] = await save_outputs(outs, "".join(c if c.isalnum() or c in " -_" else "_" for c in stem).strip())
            reports.append(rep)
        out(reports[0] if len(reports) == 1 else {"variations": reports}, a.json)
        return 0
    except CoreError as exc:
        print(f"formdes: {exc}", file=sys.stderr)
        return 1
    finally:
        await core.stop()


async def cmd_intake(a) -> int:
    pack = await ingest(a)
    s = P.summary(pack)
    s["ranked"] = R.rank(pack, limit=5)
    out(s, a.json)
    return 0


async def cmd_export(a) -> int:
    await core.start()
    try:
        await documents.open_path(str(here(a.file)), source="cli")
        saved = await save_outputs(a.out, here(a.file).stem)
        out({"saved": saved}, a.json)
        return 0
    except CoreError as exc:
        print(f"formdes: {exc}", file=sys.stderr)
        return 1
    finally:
        await core.stop()


async def cmd_check(a) -> int:
    t = json.loads(here(a.file).read_text(encoding="utf-8"))
    probs = spec.problems(t)
    if probs:
        out({"ok": False, "problems": probs}, a.json)
        return 1
    try:
        rep = await registry.check_spec(t, render=bool(a.renders), size=a.size)
    finally:
        if registry._aux is not None:
            await registry._aux.stop()
    if a.renders:
        d = here(a.renders)
        d.mkdir(parents=True, exist_ok=True)
        for i, b64 in enumerate(rep.pop("renders", [])):
            (d / f"{t['id']}-p{i + 1}.png").write_bytes(base64.b64decode(b64))
        rep["renders"] = str(d)
    out(rep, a.json)
    return 0 if rep.get("ok") and not rep.get("overset") else 1


def main() -> int:
    ap = argparse.ArgumentParser(prog="formdes", description="formdes from the command line: lay out content with templates, export documents, check templates.")
    sub = ap.add_subparsers(dest="cmd", required=True)
    t = sub.add_parser("templates", help="list templates, or show one")
    t.add_argument("id", nargs="?")
    t.add_argument("--category")
    t.add_argument("--kind")
    t.add_argument("--q")
    t.add_argument("--medium")
    t.add_argument("--json", action="store_true")
    for name, hlp in (("make", "lay out files and a brief with a template"), ("intake", "read files and a brief into a content pack and rank templates")):
        m = sub.add_parser(name, help=hlp)
        m.add_argument("files", nargs="*")
        m.add_argument("--brief", default="")
        m.add_argument("--brief-file")
        m.add_argument("--options", help="JSON options for intake, e.g. {\"kind\": \"magazine\"}")
        m.add_argument("--json", action="store_true")
        if name == "make":
            m.add_argument("--template")
            m.add_argument("--category")
            m.add_argument("--brand")
            m.add_argument("--title")
            m.add_argument("--fills", help="JSON slot values, e.g. {\"cta\": \"Book now\"}")
            m.add_argument("--variations", type=int, default=1)
            m.add_argument("--no-placeholders", action="store_true")
            m.add_argument("--out", action="append", default=[], help="file to write: .fdes .pdf .idml .png .jpg .html .epub (repeatable)")
    e = sub.add_parser("export", help="export a .fdes, .fdoc or .idml document")
    e.add_argument("file")
    e.add_argument("--out", action="append", required=True)
    e.add_argument("--json", action="store_true")
    c = sub.add_parser("check-template", help="validate and render a template JSON file")
    c.add_argument("file")
    c.add_argument("--renders")
    c.add_argument("--size", type=int, default=800)
    c.add_argument("--json", action="store_true")
    a = ap.parse_args()
    config.ensure_dirs()
    if a.cmd == "templates":
        return cmd_templates(a)
    fn = {"make": cmd_make, "intake": cmd_intake, "export": cmd_export, "check-template": cmd_check}[a.cmd]
    return asyncio.run(fn(a))


if __name__ == "__main__":
    sys.exit(main())
