#!/usr/bin/env python3
"""./formdesign agents: layout by conversation with Claude Code, Codex and Cursor.

  sync [--check]            write the harness skill's tool reference from the engine's MCP catalogue and mirror the
                            agent skills from .claude/skills to .agents/skills (Codex, Cursor); --check exits 1 on drift
  install [--dry-run] [claude|codex|cursor ...]
                            set the three up for this user account (all by default): the MCP server (./formdesign mcp at
                            this folder's absolute path) and the skills; files are backed up before they change, and
                            running it again changes nothing
"""

import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SKILLS = ["formdesign-harness", "formdesign-template-creator"]
SRC = ROOT / ".claude" / "skills"
MIRROR = ROOT / ".agents" / "skills"
LAUNCH = str(ROOT / ("formdesign.bat" if os.name == "nt" else "formdesign"))
GROUPS = [("Intake and make", ["intake", "list_templates", "get_template", "make_layout"]), ("Look and fix", ["preview", "get_slots", "set_slots", "get_document", "preflight", "find_change"]),
          ("Documents", ["list_documents", "switch_document", "new_document", "open_document", "save_document", "export_document", "book"]),
          ("Any command", ["run_commands", "find_commands", "undo", "redo"]), ("PDF sources", ["read_pdf", "pdf_page_image"]),
          ("Brands and templates", ["list_brands", "save_brand", "validate_template", "save_template", "library"])]


def tools_reference() -> str:
    sys.path.insert(0, str(ROOT / "backend"))
    from app.routers.mcp import TOOLS
    out = ["# FormDesign MCP tools", "", "Generated from the engine's catalogue by `./formdesign agents sync`; do not edit by hand.", "", "## Contents", ""]
    listed = set()
    for title, names in GROUPS:
        here = [t["name"] for t in TOOLS if t["name"] in names]
        listed.update(here)
        out.append(f"- {title}: " + ", ".join(f"`{n}`" for n in here))
    rest = [t["name"] for t in TOOLS if t["name"] not in listed]
    if rest:
        out.append("- Other: " + ", ".join(f"`{n}`" for n in rest))
    out.append("")
    for t in TOOLS:
        a = t["annotations"]
        marks = [m for m, on in (("read-only", a["readOnlyHint"]), ("writes or replaces files", a["destructiveHint"])) if on]
        out += [f"## {t['name']}", "", t["description"], ""]
        props, required = t["inputSchema"]["properties"], set(t["inputSchema"]["required"])
        for k, v in props.items():
            kind = v.get("type", "any") + (f" ({' | '.join(map(str, v['enum']))})" if "enum" in v else "")
            extra = "; ".join(x for x in (("required" if k in required else ""), (f"default {json.dumps(v['default'])}" if "default" in v else ""), v.get("description", "")) if x)
            out.append(f"- `{k}`: {kind}" + (f"; {extra}" if extra else ""))
        if not props:
            out.append("- no parameters")
        if marks:
            out += ["", "Note: " + ", ".join(marks) + "."]
        out.append("")
    return "\n".join(out)


def templates_reference() -> str:
    sys.path.insert(0, str(ROOT / "backend"))
    from app.templates import spec
    out = ["# FormDesign built-in templates", "", "Generated from the template library by `./formdesign agents sync`; do not edit by hand. The person's own templates are listed live by `list_templates`.", "",
           "Columns: id, size in the template's unit, pages, kinds the Make pass ranks it for, flows (running stories that add continuation pages), slots it fills.", ""]
    cards = [spec.card(t) for t in spec.load_all().values() if not t.get("_user")]
    present = [c for c in ["magazine", "newsletter", "book", "report", "print", "stationery", "ad", "web", "presentation", "other"] if any(x["category"] == c for x in cards)]
    out += ["## Contents", ""] + [f"- {c.title()} ({sum(1 for x in cards if x['category'] == c)})" for c in present] + [""]
    for cat in ["magazine", "newsletter", "book", "report", "print", "stationery", "ad", "web", "presentation", "other"]:
        rows = sorted((c for c in cards if c["category"] == cat), key=lambda c: c["id"])
        if not rows:
            continue
        out += [f"## {cat.title()}", "", "| id | size | pages | kinds | flows | slots |", "| --- | --- | --- | --- | --- | --- |"]
        for c in rows:
            size = f"{c['size'][0]:g} x {c['size'][1]:g} {c['unit']}"
            slots = ", ".join(c["slots"][:14]) + (f" (+{len(c['slots']) - 14})" if len(c["slots"]) > 14 else "")
            out.append(f"| `{c['id']}` | {size} | {c['pages']} | {', '.join(c['kinds'])} | {', '.join(c['flows']) or 'none'} | {slots} |")
        out.append("")
    return "\n".join(out)


def _files(folder: Path) -> dict:
    return {str(p.relative_to(folder)): p.read_bytes() for p in sorted(folder.rglob("*")) if p.is_file()}


def sync(check: bool) -> int:
    drift = []
    for ref, want in ((SRC / "formdesign-harness" / "references" / "tools.md", tools_reference()),
                      (SRC / "formdesign-harness" / "references" / "templates.md", templates_reference())):
        if not ref.exists() or ref.read_text(encoding="utf-8") != want:
            drift.append(str(ref.relative_to(ROOT)))
            if not check:
                ref.parent.mkdir(parents=True, exist_ok=True)
                ref.write_text(want, encoding="utf-8")
    for name in SKILLS:
        src, dst = SRC / name, MIRROR / name
        if not dst.exists() or _files(dst) != _files(src):
            drift.append(str(dst.relative_to(ROOT)))
            if not check:
                shutil.rmtree(dst, ignore_errors=True)
                shutil.copytree(src, dst)
    if check:
        print("agents: in step" if not drift else "agents: drifted: " + ", ".join(drift) + " (run ./formdesign agents sync)")
        return 1 if drift else 0
    print("agents: " + ("updated " + ", ".join(drift) if drift else "already in step"))
    return 0


def _backup(path: Path, dry: bool) -> None:
    if path.exists() and not dry:
        shutil.copy2(path, path.with_name(path.name + f".bak-{time.strftime('%Y%m%d-%H%M%S')}"))


def _copy_skills(base: Path, dry: bool, said: list) -> None:
    for name in SKILLS:
        to = base / name
        if to.exists() and _files(to) == _files(SRC / name):
            said.append(f"  skill already at {to}")
            continue
        said.append(f"  skill -> {to}")
        if not dry:
            shutil.rmtree(to, ignore_errors=True)
            to.parent.mkdir(parents=True, exist_ok=True)
            shutil.copytree(SRC / name, to)


def install_claude(home: Path, dry: bool, said: list) -> None:
    said.append("Claude Code:")
    claude = shutil.which("claude")
    if claude:
        have = subprocess.run([claude, "mcp", "get", "formdesign"], capture_output=True, text=True)
        if have.returncode == 0 and LAUNCH in have.stdout:
            said.append("  MCP server formdesign already set (user scope)")
        else:
            said.append(f"  claude mcp add --scope user formdesign -- {LAUNCH} mcp")
            if not dry:
                if have.returncode == 0:
                    subprocess.run([claude, "mcp", "remove", "formdesign", "--scope", "user"], capture_output=True)
                subprocess.run([claude, "mcp", "add", "--scope", "user", "formdesign", "--", LAUNCH, "mcp"], check=True, capture_output=True)
    else:
        said.append(f"  the claude command is not installed here; once it is, run: claude mcp add --scope user formdesign -- {LAUNCH} mcp")
    _copy_skills(home / ".claude" / "skills", dry, said)


def install_codex(home: Path, dry: bool, said: list) -> None:
    said.append("Codex:")
    cfg = Path(os.environ.get("CODEX_HOME", home / ".codex")) / "config.toml"
    block = f'[mcp_servers.formdesign]\ncommand = "{LAUNCH}"\nargs = ["mcp"]\nstartup_timeout_sec = 90\ntool_timeout_sec = 600\n'
    text = cfg.read_text(encoding="utf-8") if cfg.exists() else ""
    found = re.search(r"(?ms)^\[mcp_servers\.formdesign\]\n.*?(?=^\[|\Z)", text)
    if found and found.group(0).strip() == block.strip():
        said.append(f"  MCP server formdesign already in {cfg}")
    else:
        said.append(f"  MCP server formdesign -> {cfg}")
        if not dry:
            _backup(cfg, dry)
            cfg.parent.mkdir(parents=True, exist_ok=True)
            new = text[:found.start()] + block + "\n" + text[found.end():] if found else (text.rstrip("\n") + "\n\n" if text.strip() else "") + block
            cfg.write_text(new, encoding="utf-8")
    _copy_skills(home / ".agents" / "skills", dry, said)


def install_cursor(home: Path, dry: bool, said: list) -> None:
    said.append("Cursor:")
    cfg = home / ".cursor" / "mcp.json"
    raw = cfg.read_text(encoding="utf-8") if cfg.exists() else ""
    data = json.loads(raw) if raw.strip() else {}
    entry = {"command": LAUNCH, "args": ["mcp"]}
    if data.get("mcpServers", {}).get("formdesign") == entry:
        said.append(f"  MCP server formdesign already in {cfg}")
    else:
        said.append(f"  MCP server formdesign -> {cfg}")
        if not dry:
            _backup(cfg, dry)
            data.setdefault("mcpServers", {})["formdesign"] = entry
            cfg.parent.mkdir(parents=True, exist_ok=True)
            cfg.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    _copy_skills(home / ".agents" / "skills", dry, said)


def main(argv: list[str]) -> int:
    cmd = argv[0] if argv else "install"
    rest = argv[1:]
    if cmd == "sync":
        return sync("--check" in rest)
    if cmd == "install":
        dry = "--dry-run" in rest
        which = [a for a in rest if not a.startswith("--")] or ["claude", "codex", "cursor"]
        bad = [w for w in which if w not in ("claude", "codex", "cursor")]
        if bad:
            print(f"agents: unknown client {', '.join(bad)} (claude, codex or cursor)", file=sys.stderr)
            return 2
        home, said = Path(os.environ.get("HOME", str(Path.home()))), []
        for w in which:
            {"claude": install_claude, "codex": install_codex, "cursor": install_cursor}[w](home, dry, said)
        print(("Would do (dry run):\n" if dry else "") + "\n".join(said))
        if not dry:
            print("\nRestart the clients, then ask one of them to lay something out: it starts the FormDesign engine when it needs it.")
        return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
