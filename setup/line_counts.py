import re
import sys
from pathlib import Path

LAYERS = [
    ("Layout core (Rust)", "native/formdesign-core/src", (".rs",)),
    ("Engine (Python)", "backend/app", (".py",)),
    ("Window (TypeScript)", "frontend/src", (".ts",)),
    ("Window styles (CSS)", "frontend/src/styles", (".css",)),
    ("Desktop shell (Rust)", "src-tauri/src", (".rs",)),
    ("Setup scripts", "setup", ("", ".sh", ".py", ".bat")),
]


def lines(root: Path, sub: str, exts: tuple[str, ...]) -> tuple[int, int]:
    files = n = 0
    base = root / sub
    if not base.is_dir():
        return 0, 0
    for p in base.rglob("*"):
        if not p.is_file() or "__pycache__" in p.parts or p.suffix not in exts:
            continue
        try:
            n += sum(1 for line in p.read_text(encoding="utf-8").splitlines() if line.strip())
        except (UnicodeDecodeError, OSError):
            continue
        files += 1
    return files, n


def command_words(root: Path, check: bool) -> int:
    import json
    ids: dict[str, bool] = {}
    for f in (root / "native/formdesign-engine/src").rglob("*.rs"):
        text = f.read_text(encoding="utf-8")
        for m in re.finditer(r'cmd\(\s*"([^"]+)"\s*,\s*"[^"]*"\s*\)((?:\s*\.\w+\((?:"(?:[^"\\]|\\.)*"|[^()])*\))*)', text):
            ids[m.group(1)] = ".help(" in m.group(2)
    try:
        meta = json.loads((root / "backend/app/command_meta.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        meta = {}
    lacking = sorted(i for i, has_help in ids.items() if not has_help and not (meta.get(i) or {}).get("description"))
    no_when = sorted(i for i in ids if not (meta.get(i) or {}).get("when"))
    print(f"{len(ids)} engine commands: {len(ids) - len(lacking)} with a description, {len(ids) - len(no_when)} with a when sentence")
    for i in lacking[:20]:
        print(f"  no description: {i}")
    return 1 if check and lacking else 0


def main() -> int:
    root = Path(sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith("--") else ".").resolve()
    check = "--check" in sys.argv
    if "--commands" in sys.argv:
        return command_words(root, check)
    print(f"{'Layer':<24}{'files':>7}{'lines':>9}")
    total = 0
    for label, sub, exts in LAYERS:
        f, n = lines(root, sub, exts)
        total += n
        print(f"{label:<24}{f:>7}{n:>9}")
    print(f"{'Total':<24}{'':>7}{total:>9}")
    templates = len(list((root / "backend/app/templates/library").glob("*/*.json")))
    mcp = (root / "backend/app/routers/mcp.py").read_text(encoding="utf-8")
    tools = len(re.findall(r'^\s*_tool\("', mcp, re.M))
    print(f"\n{templates} templates, {tools} MCP tools")
    if not check:
        return 0
    readme = (root / "README.md").read_text(encoding="utf-8")
    problems = []
    if f"{templates} templates" not in readme:
        problems.append(f"README does not say {templates} templates")
    for m in re.finditer(r"(?:server with|exposes) (\d+) tools", readme):
        if int(m.group(1)) != tools:
            problems.append(f"README says {m.group(1)} tools, the engine has {tools}")
    for p in problems:
        print(f"counts: {p}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
