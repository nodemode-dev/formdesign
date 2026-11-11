import json
import re

from app.core import config

MODES = ("text", "grep", "glyph", "object")
KEYS = ("find", "change", "caseSensitive", "wholeWord", "scope", "include", "findFormat", "changeFormat", "glyph", "toGlyph", "object", "objectChange")

BUILTIN = [
    {"name": "Multiple returns to one", "mode": "grep", "find": r"\r\r+", "change": r"\r"},
    {"name": "Multiple spaces to one", "mode": "grep", "find": "[ ]{2,}", "change": " "},
    {"name": "Spaces and tabs at paragraph start", "mode": "grep", "find": r"^[ \t]+", "change": ""},
    {"name": "Spaces and tabs at paragraph end", "mode": "grep", "find": r"[ \t]+$", "change": ""},
    {"name": "Space before punctuation", "mode": "grep", "find": r"[ ]+([.,;:!?])", "change": "$1"},
    {"name": "Hyphen in number ranges to en dash", "mode": "grep", "find": r"(?<=\d)-(?=\d)", "change": "~="},
    {"name": "Double hyphen to em dash", "mode": "grep", "find": "--", "change": "~_"},
    {"name": "Straight double quotes to curly", "mode": "grep", "find": r'"([^"\r]*)"', "change": "~{$1~}"},
    {"name": "Straight apostrophes to curly", "mode": "grep", "find": r"(?<=\w)'(?=\w)", "change": "~]"},
    {"name": "Three dots to an ellipsis", "mode": "grep", "find": r"\.\.\.", "change": "~e"},
    {"name": "Phone numbers to dotted form", "mode": "grep", "find": r"\(?(\d{3})\)?[-. ]?(\d{3})[-. ]?(\d{4})\b", "change": "$1.$2.$3"},
    {"name": "Hairline strokes to 0.5 pt", "mode": "object", "object": {"strokeWeight": 0.25}, "objectChange": {"strokeWeight": 0.5}},
]


class QueryError(ValueError):
    pass


def slug(name: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", (name or "").lower()).strip("-") or "query"


def clean(q: dict) -> dict:
    name = str(q.get("name") or "").strip()[:80]
    if not name:
        raise QueryError("A saved query needs a name.")
    mode = q.get("mode") if q.get("mode") in MODES else "text"
    out = {"name": name, "mode": mode}
    for k in KEYS:
        if q.get(k) not in (None, "", {}):
            out[k] = q[k]
    return out


def listing() -> list[dict]:
    out = [{**q, "builtin": True} for q in BUILTIN]
    if config.QUERIES_DIR.is_dir():
        for f in sorted(config.QUERIES_DIR.glob("*.json")):
            try:
                out.append(json.loads(f.read_text()))
            except ValueError:
                continue
    return out


def get(name: str) -> dict | None:
    key = (name or "").strip().lower()
    return next((q for q in listing() if q["name"].lower() == key), None)


def save(q: dict) -> dict:
    out = clean(q)
    if any(b["name"].lower() == out["name"].lower() for b in BUILTIN):
        raise QueryError(f"\"{out['name']}\" is a built-in query; save under another name.")
    config.QUERIES_DIR.mkdir(parents=True, exist_ok=True)
    (config.QUERIES_DIR / f"{slug(out['name'])}.json").write_text(json.dumps(out, indent=2), encoding="utf-8")
    return out


def delete(name: str) -> None:
    if any(b["name"].lower() == (name or "").lower() for b in BUILTIN):
        raise QueryError("Built-in queries cannot be deleted.")
    path = config.QUERIES_DIR / f"{slug(name)}.json"
    if not path.is_file():
        raise QueryError(f"No saved query named \"{name}\".")
    path.unlink()


def params(q: dict) -> dict:
    return {k: v for k, v in q.items() if k in KEYS or k == "mode"}
