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


def post(engine: str, tool: str, arguments: dict) -> dict:
    payload = {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": tool, "arguments": arguments}}
    req = urllib.request.Request(f"{engine}/mcp", data=json.dumps(payload).encode(), headers={"Content-Type": "application/json", "User-Agent": "formdesign-template-check/1"}, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=300) as r:
            reply = json.loads(r.read())
    except urllib.error.HTTPError as exc:
        return {"ok": False, "error": exc.read().decode(errors="replace")}
    except (urllib.error.URLError, OSError) as exc:
        return {"ok": False, "error": f"the FormDesign engine is not answering ({exc}); start it with ./formdesign backend"}
    if "error" in reply:
        return {"ok": False, "error": str((reply["error"] or {}).get("message") or "refused")}
    result = reply.get("result") or {}
    blocks = result.get("content") or []
    text = " ".join(b.get("text", "") for b in blocks if b.get("type") == "text")
    if result.get("isError"):
        return {"ok": False, "error": text}
    try:
        report = json.loads(text)
    except ValueError:
        report = {"ok": True, "saved": text}
    if isinstance(report, dict):
        images = [b["data"] for b in blocks if b.get("type") == "image"]
        if images:
            report["renders"] = images
    return report


def main() -> int:
    ap = argparse.ArgumentParser(description="Validate and render a FormDesign template")
    ap.add_argument("file")
    ap.add_argument("--renders")
    ap.add_argument("--engine", default="http://127.0.0.1:8196")
    ap.add_argument("--save", action="store_true")
    a = ap.parse_args()
    spec = json.loads(Path(a.file).read_text(encoding="utf-8"))
    rep = post(a.engine, "validate_template", {"spec": spec, "render": bool(a.renders)})
    renders = rep.pop("renders", []) or []
    if a.renders and renders:
        out = Path(a.renders)
        out.mkdir(parents=True, exist_ok=True)
        for i, b64 in enumerate(renders):
            (out / f"{spec.get('id', 'template')}-p{i + 1}.png").write_bytes(base64.b64decode(b64))
        rep["renders"] = str(out)
    clean = bool(rep.get("ok")) and not rep.get("problems") and not rep.get("overset") and not any(str(p).startswith("error") for p in rep.get("preflight") or [])
    if a.save and clean:
        rep["saved"] = post(a.engine, "save_template", {"spec": spec})
    print(json.dumps(rep, indent=1))
    return 0 if clean else 1


if __name__ == "__main__":
    sys.exit(main())
