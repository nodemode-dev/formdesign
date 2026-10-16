#!/usr/bin/env python3
"""./formdes i18n keys|check|export LANG FILE.csv|import LANG FILE.csv

keys    collect every English interface string into frontend/src/ui/i18n/keys.json
check   list how many keys each language table is missing (exit 1 when any are)
export  write one language's table to a CSV (key, translation) for review
import  read a reviewed CSV back into that language's table"""

import csv
import html
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "frontend" / "src"
I18N = SRC / "ui" / "i18n"
TABLES = I18N / "tables"
KEYS = I18N / "keys.json"
ENGINE = ROOT / "native" / "formdes-engine" / "src"
LANGS = ["es", "es-MX", "pt-BR", "fr", "fr-CA", "de", "it", "nl", "da", "fi", "nb", "sv", "ru", "pl", "cs", "hu", "uk", "tr", "zh-CN", "zh-TW", "ja", "ko", "id", "th", "vi"]
VARIANTS = {"es-MX", "fr-CA"}

TEXT_NODE = re.compile(r">([^<>{}$`]*[A-Za-z][^<>{}$`]*)<")
ATTR = re.compile(r"""\b(?:title|aria-label|placeholder|alt)=["']([^"'$`{}]+)["']""")
PROP = re.compile(r"""\b(?:label|title|note|placeholder|description|hint|text|heading|empty)\s*:\s*'((?:[^'\\]|\\.)+)'""")
CALL = re.compile(r"""\b(?:toast|setText|setTitle|t|confirmDialog|alertDialog|say|status|fail)\(\s*(?:[\w.]+\s*,\s*)?'((?:[^'\\]|\\.)+)'""")
OPTION = re.compile(r"""new Option\(\s*'((?:[^'\\]|\\.)+)'""")
TUPLE = re.compile(r"""\[\s*'[\w.-]+'\s*,\s*'((?:[^'\\]|\\.)+)'""")
CMD = re.compile(r'cmd\(\s*"[^"]+"\s*,\s*"([^"]+)"\s*\)')
MENU = re.compile(r'\.menu\(&\[([^\]]*)\]\)')
HELP = re.compile(r'\.help\("((?:[^"\\]|\\.)+)"\)')
WORD = re.compile(r"[A-Za-z]")
CODE_IN_TEXT = re.compile(r"[;=\[\]{}|\\]|\('|'\)|',|\w\(|\)\s*$|^\s*\)|\bconst\b|\breturn\b|\bnew \w+\(|\.\w+\(|\w\.\w")
CODEISH = re.compile(r"^[\w.-]+$|^[a-z]+[A-Z]\w*$|^--|^#|^\.|^[\d\s.,:%x×-]+$|^(?:Mod|Alt|Shift|Ctrl)\+")


def keep(s: str) -> bool:
    s = s.strip()
    if len(s) < 2 or len(s) > 300 or not WORD.search(s):
        return False
    if CODEISH.match(s) and not (s[:1].isupper() and " " not in s and s.isalpha()):
        return False
    if s.startswith(("http", "/api", "./", "cv-", "fd-", "ft-", "fc-", "fa-", "(", ",", ":")) or "=>" in s or s.count("_") > 1 or re.search(r":\s*(?:string|number|boolean|Set|Map|Record)\b", s):
        return False
    return True


def unescape(s: str) -> str:
    return html.unescape(s.replace("\\'", "'").replace("\\n", " ")).strip()


def collect() -> list[str]:
    found: set[str] = set()
    for f in SRC.rglob("*.ts"):
        if "i18n" in f.parts:
            continue
        text = f.read_text(encoding="utf-8")
        for m in re.finditer(r"const MENUS[^=]*=\s*\[([^\]]*)\]", text):
            found.update(re.findall(r"'([^']+)'", m.group(1)))
        for rx in (TEXT_NODE, ATTR, PROP, CALL, OPTION, TUPLE):
            for m in rx.finditer(text):
                s = unescape(m.group(1))
                if rx is TEXT_NODE and CODE_IN_TEXT.search(s):
                    continue
                if keep(s):
                    found.add(re.sub(r"\s+", " ", s))
    html_file = ROOT / "frontend" / "index.html"
    if html_file.exists():
        for rx in (TEXT_NODE, ATTR):
            for m in rx.finditer(html_file.read_text(encoding="utf-8")):
                s = unescape(m.group(1))
                if keep(s):
                    found.add(re.sub(r"\s+", " ", s))
    for f in ENGINE.rglob("*.rs"):
        text = f.read_text(encoding="utf-8")
        for m in CMD.finditer(text):
            found.add(m.group(1).strip())
        for m in MENU.finditer(text):
            for seg in re.findall(r'"([^"]+)"', m.group(1)):
                found.add(seg.strip())
        for m in HELP.finditer(text):
            found.add(m.group(1).replace('\\"', '"').strip())
    return sorted(s for s in found if keep(s))


def table(lang: str) -> dict:
    p = TABLES / f"{lang}.json"
    try:
        return json.loads(p.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def main(argv: list[str]) -> int:
    cmd = argv[0] if argv else "keys"
    if cmd == "keys":
        keys = collect()
        I18N.mkdir(parents=True, exist_ok=True)
        KEYS.write_text(json.dumps(keys, ensure_ascii=False, indent=0) + "\n", encoding="utf-8")
        print(f"[formdes] {len(keys)} interface strings in {KEYS.relative_to(ROOT)}")
        return 0
    if cmd == "check":
        keys = json.loads(KEYS.read_text(encoding="utf-8"))
        bad = 0
        for lang in LANGS:
            if lang in VARIANTS:
                print(f"  {lang:6} variant of {lang[:2]}: {len(table(lang))} own entries")
                continue
            missing = [k for k in keys if not table(lang).get(k)]
            bad += bool(missing)
            print(f"  {lang:6} {len(keys) - len(missing):5} of {len(keys)}" + (f"  missing e.g. {missing[0]!r}" if missing else ""))
        return 1 if bad else 0
    if cmd in ("export", "import") and len(argv) == 3:
        lang, path = argv[1], Path(argv[2])
        if lang not in LANGS:
            print(f"[formdes] language is one of {', '.join(LANGS)}", file=sys.stderr)
            return 2
        if cmd == "export":
            keys = json.loads(KEYS.read_text(encoding="utf-8"))
            t = table(lang)
            with path.open("w", newline="", encoding="utf-8") as fh:
                w = csv.writer(fh)
                w.writerow(["english", lang])
                for k in keys:
                    w.writerow([k, t.get(k, "")])
            print(f"[formdes] wrote {len(keys)} rows to {path}")
            return 0
        t = table(lang)
        with path.open(newline="", encoding="utf-8") as fh:
            for row in list(csv.reader(fh))[1:]:
                if len(row) >= 2 and row[1].strip():
                    t[row[0]] = row[1].strip()
        TABLES.mkdir(parents=True, exist_ok=True)
        (TABLES / f"{lang}.json").write_text(json.dumps(dict(sorted(t.items())), ensure_ascii=False, indent=0) + "\n", encoding="utf-8")
        print(f"[formdes] {lang}: {len(t)} entries")
        return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
