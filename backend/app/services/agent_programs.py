import json
import os
import re
import subprocess
import time
from pathlib import Path

from app.core import config
from app.services import agent_lookup, agents, claude_code
from app.services.pty import PRESETS

TAB_ENTRY = "formdesign_tab"

PROGRAMS = {p["id"]: p for p in PRESETS if p["cmd"]}
REPORTED = {kind: {"label": p["label"], "install": p["install"]} for kind, p in PROGRAMS.items()} | {
    kind: {"label": agent_lookup.PROGRAMS[kind]["title"], "install": agent_lookup.install(kind)} for kind in agent_lookup.PROGRAMS if kind not in PROGRAMS}
BLOCK = re.compile(r"(?ms)^\[mcp_servers\.formdesign_tab\]\n.*?(?=^\[|\Z)")
_cache: dict = {"at": 0.0, "value": None}


class ProgramError(ValueError):
    pass


def find(kind: str) -> str | None:
    return claude_code.located(kind).path or None


def _claude_signed_in(path: str) -> bool | None:
    if os.environ.get("ANTHROPIC_API_KEY") or os.environ.get("CLAUDE_CODE_OAUTH_TOKEN"):
        return True
    try:
        out = agent_lookup.run([path, "auth", "status"], claude_code.child_env(path), claude_code.CHECK_WAIT)
        data = json.loads(out.stdout)
    except (OSError, subprocess.SubprocessError, ValueError):
        return None
    return bool(data.get("loggedIn")) if isinstance(data, dict) and "loggedIn" in data else None


def status(fresh: bool = False) -> dict:
    if not fresh and _cache["value"] is not None and time.time() - _cache["at"] < 60:
        return _cache["value"]
    out = {}
    for kind, spec in REPORTED.items():
        found = claude_code.info(fresh) if kind == "claude" else agent_lookup.answer(claude_code.located(kind, fresh), spec["install"], claude_code.OTHERWISE)
        out[kind] = found | {"signedIn": _claude_signed_in(found["path"]) if kind == "claude" and found["installed"] else None,
                             "install": spec["install"], "label": spec["label"], "title": spec["label"]}
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


def tab_folder(kind: str) -> Path:
    folder = config.DATA_DIR / "AgentTabs" / kind
    folder.mkdir(mode=0o700, parents=True, exist_ok=True)
    return folder


def _servers(path: Path) -> dict:
    data = json.loads(path.read_text(encoding="utf-8") or "{}") if path.exists() else {}
    if not isinstance(data, dict) or not isinstance(data.get("mcpServers", {}), dict):
        raise OSError(f"{path} is not an MCP servers file FormDesign can read, so it was left alone")
    return data


def _replace(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    part = path.with_name(path.name + f".part-{os.getpid()}")
    with os.fdopen(os.open(part, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600), "w", encoding="utf-8") as f:
        f.write(json.dumps(data, indent=2) + "\n")
    os.replace(part, path)


def write_tab_entry(folder: Path, token: str) -> None:
    path = folder / ".agents" / "mcp_config.json"
    data = _servers(path)
    data.setdefault("mcpServers", {})[TAB_ENTRY] = {"serverUrl": claude_code.engine_url() + "/mcp", "headers": {agents.INAPP_HEADER: token}}
    _replace(path, data)


def remove_tab_entry(folder: Path, token: str) -> None:
    path = folder / ".agents" / "mcp_config.json"
    try:
        data = _servers(path)
        entry = data.get("mcpServers", {}).get(TAB_ENTRY)
        if not isinstance(entry, dict) or (entry.get("headers") or {}).get(agents.INAPP_HEADER) != token:
            return
        del data["mcpServers"][TAB_ENTRY]
        _replace(path, data)
    except (OSError, ValueError):
        return


def launch(kind: str) -> tuple[list[str], dict[str, str], str, str]:
    spec = PROGRAMS[kind]
    found = claude_code.usable(kind)
    if found.state != "runs":
        raise ProgramError(agent_lookup.sentence(found, spec["install"], claude_code.OTHERWISE))
    exe = found.path
    if kind == "antigravity":
        token = claude_code.issue_for(kind)
        try:
            folder = tab_folder(kind)
            write_tab_entry(folder, token)
        except (OSError, ValueError) as exc:
            claude_code.revoke(token)
            raise ProgramError(f"{spec['label']} cannot be connected to FormDesign: {exc}") from None
        return claude_code.wrap([exe]), claude_code.child_env(exe) | {claude_code.TOKEN_VAR: token}, str(folder), token
    try:
        folder = claude_code.workdir()
    except claude_code.ClaudeError as exc:
        raise ProgramError(str(exc).replace("Claude Code", spec["label"], 1)) from None
    connect(kind, folder)
    token = claude_code.issue_for(kind)
    return claude_code.wrap([exe]), claude_code.child_env(exe) | {claude_code.TOKEN_VAR: token}, str(folder), token
