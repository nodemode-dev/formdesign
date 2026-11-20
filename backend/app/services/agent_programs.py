import json
import os
import re
import shlex
import shutil
import subprocess
import time
from pathlib import Path

from app.services import agents, claude_code
from app.services.pty import PRESETS

PROGRAMS = {p["id"]: p for p in PRESETS if p["cmd"]}
VERSION = re.compile(r"\d+(?:\.\d+)+[\w.+-]*")
BLOCK = re.compile(r"(?ms)^\[mcp_servers\.formdesign_tab\]\n.*?(?=^\[|\Z)")
_cache: dict = {"at": 0.0, "value": None}


class ProgramError(ValueError):
    pass


def find(kind: str) -> str | None:
    binary = PROGRAMS[kind]["cmd"]
    found = shutil.which(binary)
    if found or os.name == "nt":
        return found
    try:
        out = subprocess.run(claude_code.login_shell() + ["-c", f"command -v {shlex.quote(binary)}"], capture_output=True, text=True, timeout=10,
                             env=claude_code.base_env(), stdin=subprocess.DEVNULL)
    except (OSError, subprocess.SubprocessError):
        return None
    lines = out.stdout.strip().splitlines()[-1:] if out.returncode == 0 else []
    return lines[0] if lines and os.path.isabs(lines[0]) and os.access(lines[0], os.X_OK) else None


def _version(path: str) -> str | None:
    try:
        out = subprocess.run([path, "--version"], capture_output=True, text=True, timeout=10, env=claude_code.base_env(), stdin=subprocess.DEVNULL)
    except (OSError, subprocess.SubprocessError):
        return None
    m = VERSION.search(out.stdout)
    return m.group(0) if m else None


def _claude_signed_in(path: str) -> bool | None:
    if os.environ.get("ANTHROPIC_API_KEY") or os.environ.get("CLAUDE_CODE_OAUTH_TOKEN"):
        return True
    try:
        out = subprocess.run([path, "auth", "status"], capture_output=True, text=True, timeout=15, env=claude_code.child_env(), stdin=subprocess.DEVNULL)
        data = json.loads(out.stdout)
    except (OSError, subprocess.SubprocessError, ValueError):
        return None
    return bool(data.get("loggedIn")) if isinstance(data, dict) and "loggedIn" in data else None


def status(fresh: bool = False) -> dict:
    if not fresh and _cache["value"] is not None and time.time() - _cache["at"] < 60:
        return _cache["value"]
    found = claude_code.info(fresh)
    out = {"claude": {"installed": found["installed"], "path": found["path"], "version": found["version"],
                      "signedIn": _claude_signed_in(found["path"]) if found["installed"] else None,
                      "install": PROGRAMS["claude"]["install"], "label": PROGRAMS["claude"]["label"]}}
    for kind in ("codex", "cursor"):
        path = find(kind)
        out[kind] = {"installed": path is not None, "path": path, "version": _version(path) if path else None, "signedIn": None,
                     "install": PROGRAMS[kind]["install"], "label": PROGRAMS[kind]["label"]}
    _cache["value"], _cache["at"] = out, time.time()
    return out


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600), "w", encoding="utf-8") as f:
        f.write(text)


def connect(kind: str, folder: Path) -> str | None:
    url = claude_code.engine_url() + "/mcp"
    try:
        if kind == "cursor":
            path = folder / ".cursor" / "mcp.json"
            raw = path.read_text(encoding="utf-8") if path.exists() else ""
            data = json.loads(raw) if raw.strip() else {}
            if not isinstance(data, dict) or not isinstance(data.get("mcpServers", {}), dict):
                return None
            entry = {"url": url, "headers": {agents.INAPP_HEADER: "${env:" + claude_code.TOKEN_VAR + "}"}}
            had = data.get("mcpServers", {}).get("formdesign_tab")
            if had is not None and claude_code.TOKEN_VAR not in json.dumps(had):
                return None
            if had != entry:
                data.setdefault("mcpServers", {})["formdesign_tab"] = entry
                _write(path, json.dumps(data, indent=2) + "\n")
            return str(path)
        if kind == "codex":
            path = folder / ".codex" / "config.toml"
            block = f"[mcp_servers.formdesign_tab]\nurl ={json.dumps(url)}\nenv_http_headers = {{ {json.dumps(agents.INAPP_HEADER)} = {json.dumps(claude_code.TOKEN_VAR)} }}\n"
            text = path.read_text(encoding="utf-8") if path.exists() else ""
            found = BLOCK.search(text)
            if found and claude_code.TOKEN_VAR not in found.group(0):
                return None
            if not found or found.group(0).strip() != block.strip():
                _write(path, text[:found.start()] + block + "\n" + text[found.end():] if found else (text.rstrip("\n") + "\n\n" if text.strip() else "") + block)
            return str(path)
    except (OSError, ValueError):
        return None
    return None


def launch(kind: str) -> tuple[list[str], dict[str, str], str, str]:
    spec = PROGRAMS[kind]
    exe = find(kind)
    if not exe:
        raise ProgramError(f"{spec['label']} is not installed. Install it with: {spec.get('install')}")
    try:
        folder = claude_code.workdir()
    except claude_code.ClaudeError as exc:
        raise ProgramError(str(exc).replace("Claude Code", spec["label"], 1)) from None
    connect(kind, folder)
    token = claude_code.issue_for(kind)
    return claude_code.wrap([exe]), claude_code.base_env() | {claude_code.TOKEN_VAR: token}, str(folder), token
