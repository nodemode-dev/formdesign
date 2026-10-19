import json
import os
import shlex
import shutil
import subprocess
import time
from pathlib import Path

from app.core import config
from app.services import agents

KEEP_ENV = {"CLAUDE_CODE_OAUTH_TOKEN", "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX", "CLAUDE_CONFIG_DIR"}
CANDIDATES = (".claude/local/claude", ".local/bin/claude", ".npm-global/bin/claude", ".bun/bin/claude", ".volta/bin/claude")
SYSTEM_PATHS = ("/opt/homebrew/bin/claude", "/usr/local/bin/claude", "/usr/bin/claude")
PREFS = config.DATA_DIR / "claude.json"
FIELDS = ("claudeBin", "claudeFolder", "claudeFirstMessage")
INSTALL = ("Claude Code is not installed: install it with npm install -g @anthropic-ai/claude-code (or from https://claude.com/claude-code), "
           "run claude once in a terminal to sign in, then open it here again")
NOTE = ("You are Claude Code running inside FormDesign, the page layout and publishing app open on this computer. "
        "Work on the open document with the formdes MCP server's tools: start with get_document or outline, find commands with find_commands, "
        "change the layout with run_commands, and look with preview or look before and after a change. "
        "Every change lands in the document the person sees in the window and is one undo step. "
        "Use the formdes-harness skill when it is available.")
_cache: dict = {"at": 0.0, "value": None}


class ClaudeError(ValueError):
    pass


def prefs() -> dict:
    try:
        d = json.loads(PREFS.read_text())
    except (OSError, ValueError):
        return {}
    return {k: str(d.get(k) or "").strip() for k in FIELDS} if isinstance(d, dict) else {}


def save_prefs(body: dict) -> dict:
    d = prefs()
    for k in FIELDS:
        if k in body and body[k] is not None and not isinstance(body[k], str):
            raise ClaudeError(f"preference {k} must be text")
    for k in FIELDS:
        if k in body:
            d[k] = (body[k] or "").strip()[:2000]
    PREFS.parent.mkdir(parents=True, exist_ok=True)
    PREFS.write_text(json.dumps(d, indent=1))
    _cache["value"] = None
    return d


def login_shell() -> list[str]:
    sh = os.environ.get("SHELL") or shutil.which("bash") or "/bin/sh"
    return [sh, "-l"] if Path(sh).name in ("bash", "zsh", "fish") else [sh]


def base_env() -> dict[str, str]:
    from app.services import pty
    env = pty.environment()
    env["FORMDES_MCP_URL"] = engine_url() + "/mcp"
    return env


def child_env() -> dict[str, str]:
    return {k: v for k, v in base_env().items() if not k.startswith("CLAUDE") or k in KEEP_ENV}


def engine_url() -> str:
    return f"http://{config.HOST}:{config.PORT}"


def _runnable(path: str) -> bool:
    p = Path(path).expanduser()
    return p.is_file() and os.access(p, os.X_OK)


def _from_login_shell() -> str | None:
    if os.name == "nt":
        return None
    try:
        out = subprocess.run(login_shell() + ["-c", "command -v claude"], capture_output=True, text=True, timeout=10, env=child_env())
    except (OSError, subprocess.SubprocessError):
        return None
    found = out.stdout.strip().splitlines()[-1:] if out.returncode == 0 else []
    return found[0] if found and os.path.isabs(found[0]) else None


def cli() -> str | None:
    chosen = prefs().get("claudeBin", "")
    if chosen and _runnable(chosen):
        return str(Path(chosen).expanduser())
    found = os.environ.get("FORMDES_CLAUDE_BIN") or shutil.which("claude")
    if found:
        return found
    home = Path.home()
    for p in [home / c for c in CANDIDATES] + [Path(s) for s in SYSTEM_PATHS]:
        if p.is_file() and os.access(p, os.X_OK):
            return str(p)
    return _from_login_shell()


def info(fresh: bool = False) -> dict:
    if not fresh and _cache["value"] is not None and time.time() - _cache["at"] < 60:
        return _cache["value"]
    path, version = cli(), None
    if path:
        try:
            out = subprocess.run([path, "--version"], capture_output=True, text=True, timeout=15, env=child_env())
            version = (out.stdout.strip().split() or [None])[0]
        except (OSError, subprocess.SubprocessError):
            path = None
    _cache["value"], _cache["at"] = {"installed": bool(path), "path": path, "version": version}, time.time()
    return _cache["value"]


def mcp_config() -> str:
    return json.dumps({"mcpServers": {"formdes": {"type": "http", "url": engine_url() + "/mcp", "headers": {agents.INAPP_HEADER: agents.INAPP_TOKEN}}}})


def workdir() -> Path:
    raw = prefs().get("claudeFolder", "") or os.environ.get("FORMDES_AGENT_DIR", "").strip()
    folder = Path(raw).expanduser() if raw else config.DATA_DIR / "Agent"
    try:
        folder.mkdir(parents=True, exist_ok=True)
    except OSError as e:
        raise ClaudeError(f"Claude Code cannot start in {folder}: {e.strerror or e}; choose another folder in Preferences > Agents") from None
    return folder


def wrap(cmd: list[str]) -> list[str]:
    if os.name == "nt":
        return cmd
    return login_shell() + ["-c", "exec " + shlex.join(cmd)]


def argv(path: str, prompt: str | None = None, resume: str | None = None) -> list[str]:
    cmd = [path, "--mcp-config", mcp_config(), "--allowedTools", "mcp__formdes", "--append-system-prompt", NOTE]
    if resume:
        cmd += ["--resume", resume]
    if prompt:
        cmd.append(prompt)
    return wrap(cmd)


def status() -> dict:
    p = prefs()
    chosen = p.get("claudeBin", "")
    note = f"{chosen} is not a program FormDesign can run, so it uses the one found on this computer" if chosen and not _runnable(chosen) else ""
    try:
        folder = str(workdir())
    except ClaudeError as exc:
        folder = str(exc)
    return info(fresh=True) | {"folder": folder, "binNote": note, "install": INSTALL, **{k: p.get(k, "") for k in FIELDS}}
