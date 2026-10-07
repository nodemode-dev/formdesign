import argparse
import asyncio
import json
import sys
import time
from pathlib import Path

from app.engine.core import Core, CoreError
from app.templates import registry, spec


async def check_one(core: Core, tid: str, out: Path | None, max_pages: int, size: int) -> dict:
    t0 = time.time()
    rep: dict = {"id": tid, "ok": True}
    try:
        built = await registry.build(core, tid, sample=True, source="check")
        ins = await core.inspect()
        rep["pages"] = ins.get("pageCount")
        rep["commands"] = built["commands"]
        rep["slots"] = sorted({s["slot"] for s in built["slots"] if s.get("slot")})
        over = []
        for sp in ins.get("spreads", []):
            for it in sp.get("items", []):
                if it.get("story") is not None and it.get("overset"):
                    over.append(it["id"])
        stories = ins.get("stories") or []
        rep["overset"] = [s.get("id") for s in stories if s.get("overset")] if stories else over
        pf = await core.exec("preflight.run", {})
        issues = pf if isinstance(pf, list) else pf.get("issues", [])
        rep["preflight"] = [f"{i.get('severity')}: {i.get('message')}" for i in issues[:12]]
        if out:
            out.mkdir(parents=True, exist_ok=True)
            for p in range(min(rep["pages"] or 1, max_pages)):
                r = await core.call("page.png", {"page": p, "maxSide": size})
                (out / f"{tid}-p{p + 1}.png").write_bytes(r.bin or b"")
    except CoreError as exc:
        rep["ok"] = False
        rep["error"] = str(exc)
    finally:
        try:
            await core.call("docs.close", {})
        except CoreError:
            pass
    rep["seconds"] = round(time.time() - t0, 2)
    return rep


async def main() -> int:
    ap = argparse.ArgumentParser(prog="python -m app.templates.check", description="Compile templates with sample content in a private layout core, render their pages and report problems.")
    ap.add_argument("ids", nargs="*", help="template ids (default: every template)")
    ap.add_argument("--out", help="folder for page renders")
    ap.add_argument("--pages", type=int, default=3)
    ap.add_argument("--size", type=int, default=900)
    a = ap.parse_args()
    ids = a.ids or sorted(registry.templates())
    core = Core()
    await core.start()
    reports = []
    try:
        for tid in ids:
            if tid not in registry.templates():
                reports.append({"id": tid, "ok": False, "error": "no such template"})
                continue
            reports.append(await check_one(core, tid, Path(a.out) if a.out else None, a.pages, a.size))
            print(json.dumps(reports[-1]), flush=True)
    finally:
        await core.stop()
    print(json.dumps({"templates": len(reports), "failed": [r["id"] for r in reports if not r["ok"]],
                      "overset": [r["id"] for r in reports if r.get("overset")], "fonts": len(spec.available_fonts())}), flush=True)
    return 1 if any(not r["ok"] for r in reports) else 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
