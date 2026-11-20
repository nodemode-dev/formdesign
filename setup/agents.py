#!/usr/bin/env python3
"""./formdesign agents: layout by conversation with Claude Code, Codex, Cursor and Antigravity.

  sync [--check]            write the harness skill's tool reference from the engine's MCP catalogue, agents/manifest.json
                            (what the app offers agents) and mirror the agent skills from .claude/skills to
                            .agents/skills (Codex, Cursor); --check exits 1 on drift, naming the file
  install [--dry-run] [claude|codex|cursor|antigravity ...]
                            set the four up for this user account (all by default): the MCP server (./formdesign mcp at
                            this folder's absolute path) and the skills; files are backed up before they change, and
                            running it again changes nothing. Antigravity gets the MCP server only, in
                            ~/.gemini/config/mcp_config.json; a server named formdesign that FormDesign did not write
                            is left alone
"""

import hashlib
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
MIRRORED = SKILLS + ["formdesign-skillset"]
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


APP = "formdesign"
DOCK = {"present": True, "positions": ["bottom", "right"], "tabs": ["chat", "terminal", "claude", "codex", "cursor", "antigravity"]}
DOCK_FOLDER = ROOT / "frontend" / "src" / "dock"
LOOKUP = ROOT / "backend" / "app" / "services" / "agent_lookup.py"
LOOKUP_HASHES = {"1.0.0": "252ea9bb9419d4990f174f91914f09800497bc69e98f849f5643bbf4217cc3f3", "1.0.1": "62667f9b68b7d101e1896ce617240424e33b7e498c560bc79d45f0ab26c77d51"}


def lookup_problems() -> list[str]:
    rel = LOOKUP.relative_to(ROOT).as_posix()
    try:
        raw = LOOKUP.read_bytes()
    except OSError:
        return [f"{rel}: missing (the shared finder for the agent programs)"]
    said = re.search(rb'^VERSION = "([^"\r\n]+)"\r?$', raw, re.M)
    version = said.group(1).decode("ascii", "replace") if said else "unknown"
    if LOOKUP_HASHES.get(version) != hashlib.sha256(raw).hexdigest():
        return [f"{rel}: does not match the hash recorded for version {version} (the shared finder changes only as a whole new version)"]
    return []


def lookup():
    import importlib.util
    spec = importlib.util.spec_from_file_location("formdesign_agent_lookup", LOOKUP)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def dock_problems(folder: Path = DOCK_FOLDER) -> list[str]:
    name = "frontend/src/dock"
    try:
        kept = json.loads((folder / "MANIFEST.json").read_text(encoding="utf-8"))["files"]
    except (OSError, ValueError, KeyError):
        return [f"{name}/MANIFEST.json: missing or unreadable"]
    now = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in folder.iterdir() if p.is_file() and p.name != "MANIFEST.json"}
    said = [f"{name}/{file}: does not match MANIFEST.json (the shared dock module changes only as a whole new version)" for file in sorted(set(kept) | set(now)) if kept.get(file) != now.get(file)]
    for path in sorted(folder.glob("*.ts")):
        for target in re.findall(r"(?:from|import)\s*\(?'([^']+)'", path.read_text(encoding="utf-8")):
            if target.startswith(".") and (path.parent / target).resolve().parent != folder.resolve():
                said.append(f"{name}/{path.name}: imports {target} from outside the shared folder")
    return said


def manifest() -> str:
    sys.path.insert(0, str(ROOT / "backend"))
    from app.routers import mcp as server
    from app.services import agents as gate
    names = [t["name"] for t in server.TOOLS]
    transports = [t for t, there in (("stdio", (ROOT / "setup" / "mcp_stdio.py").is_file()), ("http", any(getattr(r, "path", "") == "/mcp" for r in server.router.routes))) if there]
    skills = [{"name": d.name, "sub_skills": sorted(s.name for s in (d / "sub-skills").iterdir() if s.is_dir()) if (d / "sub-skills").is_dir() else []} for d in sorted(SRC.iterdir()) if d.is_dir()]
    doc = {"format": "form-agent-manifest", "version": 1, "app": APP,
           "mcp": {"tools": names, "read_only": [t["name"] for t in server.TOOLS if t["annotations"]["readOnlyHint"]], "approval_gated": [n for n in names if n in gate.GATED_TOOLS], "transports": transports},
           "skills": skills, "dock": DOCK, "extensions": None}
    return json.dumps(doc, indent=2) + "\n"


def _files(folder: Path) -> dict:
    return {str(p.relative_to(folder)): p.read_bytes() for p in sorted(folder.rglob("*")) if p.is_file()}


def sync(check: bool) -> int:
    drift = []
    for ref, want in ((ROOT / "agents" / "manifest.json", manifest()), (SRC / "formdesign-harness" / "references" / "tools.md", tools_reference()),
                      (SRC / "formdesign-harness" / "references" / "templates.md", templates_reference())):
        if not ref.exists() or ref.read_text(encoding="utf-8") != want:
            drift.append(str(ref.relative_to(ROOT)))
            if not check:
                ref.parent.mkdir(parents=True, exist_ok=True)
                ref.write_text(want, encoding="utf-8")
    for name in MIRRORED:
        src, dst = SRC / name, MIRROR / name
        if not dst.exists() or _files(dst) != _files(src):
            drift.append(str(dst.relative_to(ROOT)))
            if not check:
                shutil.rmtree(dst, ignore_errors=True)
                shutil.copytree(src, dst)
    problems = dock_problems() + lookup_problems()
    for line in problems:
        print("agents: " + line)
    if check:
        if drift or not problems:
            print("agents: in step" if not drift else "agents: drifted: " + ", ".join(drift) + " (run ./formdesign agents sync)")
        return 1 if drift or problems else 0
    print("agents: " + ("updated " + ", ".join(drift) if drift else "already in step"))
    return 1 if problems else 0


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
    finder = lookup()
    found = finder.check(finder.find("claude", override_vars=("FORMDESIGN_CLAUDE_BIN",), home=home))
    claude = found.path if found.state == "runs" else None
    if claude:
        env = finder.child_env(claude, home=home)
        have = finder.run([claude, "mcp", "get", "formdesign"], env, 60)
        if have.returncode == 0 and LAUNCH in have.stdout:
            said.append("  MCP server formdesign already set (user scope)")
        else:
            said.append(f"  claude mcp add --scope user formdesign -- {LAUNCH} mcp")
            if not dry:
                if have.returncode == 0:
                    finder.run([claude, "mcp", "remove", "formdesign", "--scope", "user"], env, 60)
                finder.run([claude, "mcp", "add", "--scope", "user", "formdesign", "--", LAUNCH, "mcp"], env, 60).check_returncode()
    else:
        said.append(f"  {finder.sentence(found, finder.install('claude'))} Once it runs: claude mcp add --scope user formdesign -- {LAUNCH} mcp")
    _copy_skills(home / ".claude" / "skills", dry, said)


def install_codex(home: Path, dry: bool, said: list) -> None:
    said.append("Codex:")
    cfg = Path(os.environ.get("CODEX_HOME", home / ".codex")) / "config.toml"
    block = f'[mcp_servers.formdesign]\ncommand = {json.dumps(LAUNCH, ensure_ascii=False)}\nargs = ["mcp"]\nstartup_timeout_sec = 90\ntool_timeout_sec = 600\n'
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


def _ours(entry: object) -> bool:
    return isinstance(entry, dict) and entry.get("args") == ["mcp"] and Path(str(entry.get("command", ""))).name == Path(LAUNCH).name


def install_antigravity(home: Path, dry: bool, said: list) -> None:
    said.append("Antigravity:")
    cfg = home / ".gemini" / "config" / "mcp_config.json"
    older = home / ".gemini" / "antigravity-cli" / "mcp_config.json"
    if older.exists() and not cfg.exists():
        said.append(f"  an older file was seen at {older} and left alone; the documented file is {cfg}")
    try:
        data = json.loads(cfg.read_text(encoding="utf-8") or "{}") if cfg.exists() else {}
    except ValueError:
        data = None
    if not isinstance(data, dict) or not isinstance(data.get("mcpServers", {}), dict):
        said.append(f"  {cfg} is not an MCP servers file FormDesign can read; left alone")
        return
    entry = {"command": LAUNCH, "args": ["mcp"]}
    have = data.get("mcpServers", {}).get("formdesign")
    if have == entry:
        said.append(f"  MCP server formdesign already in {cfg}")
    elif have is not None and not _ours(have):
        said.append(f"  {cfg} has a server named formdesign that FormDesign did not write; left alone")
    else:
        said.append(f"  MCP server formdesign -> {cfg}")
        if not dry:
            _backup(cfg, dry)
            data.setdefault("mcpServers", {})["formdesign"] = entry
            cfg.parent.mkdir(parents=True, exist_ok=True)
            part = cfg.with_name(cfg.name + f".part-{os.getpid()}")
            part.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
            os.replace(part, cfg)


def main(argv: list[str]) -> int:
    cmd = argv[0] if argv else "install"
    rest = argv[1:]
    if cmd == "sync":
        return sync("--check" in rest)
    if cmd == "install":
        dry = "--dry-run" in rest
        which = [a for a in rest if not a.startswith("--")] or ["claude", "codex", "cursor", "antigravity"]
        bad = [w for w in which if w not in ("claude", "codex", "cursor", "antigravity")]
        if bad:
            print(f"agents: unknown client {', '.join(bad)} (claude, codex, cursor or antigravity)", file=sys.stderr)
            return 2
        home, said = Path(os.environ.get("HOME", str(Path.home()))), []
        for w in which:
            {"claude": install_claude, "codex": install_codex, "cursor": install_cursor, "antigravity": install_antigravity}[w](home, dry, said)
        print(("Would do (dry run):\n" if dry else "") + "\n".join(said))
        if not dry:
            print("\nRestart the clients, then ask one of them to lay something out: it starts the FormDesign engine when it needs it.")
        return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
