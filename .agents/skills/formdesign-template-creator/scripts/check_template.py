#!/usr/bin/env python3
"""Validate and render a FormDesign template JSON file through a running FormDesign engine.

usage: python3 check_template.py TEMPLATE.json [--renders DIR] [--engine http://127.0.0.1:8196] [--save]

Prints the engine's report (problems, pages, slots, overset outside flows, preflight) as JSON, writes page PNGs to DIR
when given, and with --save stores the template in the person's library once it is clean. Exit code 0 when clean."""

import argparse
import base64
import json
import sys
import urllib.error
import urllib.request
from pathlib import Path


def post(url: str, body: dict) -> dict:
    req = urllib.request.Request(url, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"}, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=300) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as exc:
        return {"ok": False, "error": exc.read().decode(errors="replace")}
    except (urllib.error.URLError, OSError) as exc:
        return {"ok": False, "error": f"the FormDesign engine is not answering ({exc}); start it with ./formdesign backend"}


def main() -> int:
    ap = argparse.ArgumentParser(description="Validate and render a FormDesign template")
    ap.add_argument("file")
    ap.add_argument("--renders")
    ap.add_argument("--engine", default="http://127.0.0.1:8196")
    ap.add_argument("--save", action="store_true")
    a = ap.parse_args()
    spec = json.loads(Path(a.file).read_text(encoding="utf-8"))
    rep = post(f"{a.engine}/api/templates/validate", {"spec": spec, "render": bool(a.renders), "size": 900})
    renders = rep.pop("renders", []) or []
    if a.renders and renders:
        out = Path(a.renders)
        out.mkdir(parents=True, exist_ok=True)
        for i, b64 in enumerate(renders):
            (out / f"{spec.get('id', 'template')}-p{i + 1}.png").write_bytes(base64.b64decode(b64))
        rep["renders"] = str(out)
    clean = bool(rep.get("ok")) and not rep.get("problems") and not rep.get("overset") and not any(str(p).startswith("error") for p in rep.get("preflight") or [])
    if a.save and clean:
        rep["saved"] = post(f"{a.engine}/api/templates", {"spec": spec})
    print(json.dumps(rep, indent=1))
    return 0 if clean else 1


if __name__ == "__main__":
    sys.exit(main())
