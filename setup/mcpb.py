#!/usr/bin/env python3
"""./formdesign mcpb: the Claude Desktop extension, formdesign-<version>.mcpb, a zip of setup/mcpb/manifest.json (its
version filled in from frontend/src/version.ts), the stdio bridge setup/mcp_stdio.py (run with --attach) and the icon.

  check        validate the manifest; exits 1 on a problem
  build DIR    write DIR/formdesign-<version>.mcpb (open it in Claude Desktop to install)
"""

import argparse
import json
import os
import re
import sys
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST = os.path.join(ROOT, "setup", "mcpb", "manifest.json")
SERVER_SOURCE = os.path.join(ROOT, "setup", "mcp_stdio.py")
ICON = os.path.join(ROOT, "assets", "formdesign_icon.png")
VERSION_TS = os.path.join(ROOT, "frontend", "src", "version.ts")
REQUIRED = ("manifest_version", "name", "version", "description", "author", "server")
STAMP = (1980, 1, 1, 0, 0, 0)


def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def version():
    try:
        with open(VERSION_TS, encoding="utf-8") as f:
            found = re.search(r"VERSION = '([^']+)'", f.read())
    except OSError:
        return None
    return found.group(1) if found else None


def problems_found():
    problems = []
    try:
        manifest = load(MANIFEST)
    except (OSError, ValueError) as exc:
        return [f"setup/mcpb/manifest.json cannot be read: {exc}"]
    for key in REQUIRED:
        if key not in manifest or manifest[key] in ("", None, {}):
            problems.append(f"manifest needs {key}")
    if manifest.get("manifest_version") != "0.2":
        problems.append("manifest_version must be 0.2")
    if not isinstance(manifest.get("name"), str) or not re.fullmatch(r"[a-z0-9][a-z0-9-]*", manifest.get("name") or ""):
        problems.append("manifest name must be lower-case letters, digits and hyphens")
    if not version():
        problems.append("frontend/src/version.ts has no VERSION")
    author = manifest.get("author")
    if not isinstance(author, dict) or not author.get("name"):
        problems.append("manifest author needs a name")
    if manifest.get("icon") and not os.path.isfile(ICON):
        problems.append("assets/formdesign_icon.png is missing")
    server = manifest.get("server")
    if not isinstance(server, dict):
        problems.append("manifest server must be an object")
    else:
        if server.get("type") != "python":
            problems.append("server type must be python")
        entry = server.get("entry_point")
        if not isinstance(entry, str) or not entry.startswith("server/"):
            problems.append("server entry_point must be a file under server/")
        elif not os.path.isfile(os.path.join(ROOT, "setup", entry[len("server/"):])):
            problems.append(f"entry point {entry} has no source in setup/")
        config = server.get("mcp_config")
        if not isinstance(config, dict) or not config.get("command") or not isinstance(config.get("args"), list):
            problems.append("server mcp_config needs a command and args")
        elif "--attach" not in config["args"]:
            problems.append("server mcp_config args need --attach (the extension never starts the engine)")
    return problems


def report(problems):
    for line in problems:
        print(f"[mcpb] {line}", file=sys.stderr)
    return 1 if problems else 0


def check():
    problems = problems_found()
    if problems:
        return report(problems)
    print("[mcpb] manifest is valid")
    return 0


def build(folder):
    problems = problems_found()
    if problems:
        return report(problems)
    manifest = load(MANIFEST)
    manifest["version"] = version()
    files = [("manifest.json", (json.dumps(manifest, indent=2) + "\n").encode()), (manifest["server"]["entry_point"], open(SERVER_SOURCE, "rb").read())]
    if manifest.get("icon"):
        files.append((manifest["icon"], open(ICON, "rb").read()))
    folder = os.path.join(os.environ.get("FORMDESIGN_CWD") or os.getcwd(), folder)
    os.makedirs(folder, exist_ok=True)
    target = os.path.join(folder, f"formdesign-{manifest['version']}.mcpb")
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for name, data in sorted(files):
            info = zipfile.ZipInfo(name, STAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = 0o644 << 16
            z.writestr(info, data, compresslevel=9)
    print(f"[mcpb] wrote {target}")
    return 0


def main():
    parser = argparse.ArgumentParser(prog="formdesign mcpb", description="the Claude Desktop extension")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--check", action="store_true", help="validate the manifest")
    group.add_argument("--build", metavar="DIR", help="write formdesign-<version>.mcpb into DIR")
    args = parser.parse_args()
    return check() if args.check else build(args.build)


if __name__ == "__main__":
    sys.exit(main())
