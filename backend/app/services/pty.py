import asyncio
import os
import secrets
import shutil
import signal
import struct
import sys
import threading
from dataclasses import dataclass, field
from pathlib import Path

from app.core import config

TOKEN = secrets.token_urlsafe(32)
MAX_SESSIONS = 12
SCROLLBACK = 256 * 1024

PRESETS = [
    {"id": "shell", "label": "Shell", "cmd": None},
    {"id": "claude", "label": "Claude Code", "cmd": "claude", "install": "npm install -g @anthropic-ai/claude-code"},
    {"id": "codex", "label": "Codex", "cmd": "codex", "install": "npm install -g @openai/codex"},
    {"id": "cursor", "label": "Cursor CLI", "cmd": "cursor-agent", "install": "curl https://cursor.com/install -fsS | bash"},
]


def workdir() -> Path:
    root = config.REPO_ROOT
    if not config.FROZEN and (root / "formdes").exists():
        return root
    config.DATA_DIR.mkdir(parents=True, exist_ok=True)
    return config.DATA_DIR


def shell_argv() -> list[str]:
    if sys.platform == "win32":
        return [os.environ.get("COMSPEC", "cmd.exe")]
    sh = os.environ.get("SHELL") or shutil.which("bash") or "/bin/sh"
    return [sh, "-l"] if Path(sh).name in ("bash", "zsh") else [sh]


def presets() -> list[dict]:
    out = []
    for p in PRESETS:
        found = True if p["cmd"] is None else bool(shutil.which(p["cmd"]))
        out.append({"id": p["id"], "label": p["label"], "available": found, "install": None if found else p.get("install")})
    return out


def environment() -> dict[str, str]:
    env = dict(os.environ)
    root = str(workdir())
    env["TERM"] = "xterm-256color"
    env["COLORTERM"] = "truecolor"
    env["FORMDES_ENGINE_URL"] = f"http://{config.HOST}:{config.PORT}"
    env["FORMDES_TERMINAL"] = "1"
    if not config.FROZEN:
        env["PATH"] = root + os.pathsep + env.get("PATH", "")
    return env


@dataclass
class Session:
    id: str
    preset: str
    proc: object
    fd: int = -1
    win: object = None
    buffer: bytearray = field(default_factory=bytearray)
    listeners: set = field(default_factory=set)
    exited: int | None = None

    def write(self, data: bytes) -> None:
        if self.exited is not None:
            return
        if self.win is not None:
            self.win.write(data.decode("utf-8", "replace"))
        else:
            os.write(self.fd, data)

    def resize(self, cols: int, rows: int) -> None:
        cols, rows = max(2, min(500, cols)), max(1, min(300, rows))
        if self.win is not None:
            self.win.setwinsize(rows, cols)
            return
        import fcntl
        import termios
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))

    def feed(self, data: bytes) -> None:
        self.buffer.extend(data)
        if len(self.buffer) > SCROLLBACK:
            del self.buffer[: len(self.buffer) - SCROLLBACK]
        for q in list(self.listeners):
            q.put_nowait(data)


class Terminals:
    def __init__(self) -> None:
        self.sessions: dict[str, Session] = {}

    def list(self) -> list[dict]:
        return [{"id": s.id, "preset": s.preset, "exited": s.exited} for s in self.sessions.values()]

    async def create(self, preset: str, cols: int, rows: int) -> Session:
        if len(self.sessions) >= MAX_SESSIONS:
            raise ValueError(f"at most {MAX_SESSIONS} terminals can be open")
        spec = next((p for p in PRESETS if p["id"] == preset), None)
        if spec is None:
            raise ValueError(f"unknown preset `{preset}`")
        if spec["cmd"] is None:
            argv = shell_argv()
        else:
            exe = shutil.which(spec["cmd"])
            if not exe:
                raise ValueError(f"{spec['label']} is not installed. Install it with: {spec.get('install')}")
            argv = [exe]
        sid = secrets.token_hex(6)
        loop = asyncio.get_running_loop()
        if sys.platform == "win32":
            from winpty import PtyProcess
            proc = PtyProcess.spawn(argv, cwd=str(workdir()), env=environment(), dimensions=(rows, cols))
            s = Session(sid, preset, proc, win=proc)
            self.sessions[sid] = s
            threading.Thread(target=self._pump_win, args=(s, loop), daemon=True).start()
        else:
            import pty
            import subprocess
            master, slave = pty.openpty()
            s = Session(sid, preset, None, fd=master)
            s.resize(cols, rows)
            proc = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave, cwd=str(workdir()), env=environment(),
                                    start_new_session=True, close_fds=True)
            os.close(slave)
            s.proc = proc
            os.set_blocking(master, False)
            self.sessions[sid] = s
            loop.add_reader(master, self._on_readable, s, loop)
        return s

    def _on_readable(self, s: Session, loop: asyncio.AbstractEventLoop) -> None:
        try:
            data = os.read(s.fd, 65536)
        except BlockingIOError:
            return
        except OSError:
            data = b""
        if data:
            s.feed(data)
            return
        loop.remove_reader(s.fd)
        self._exit(s, s.proc.wait())

    def _pump_win(self, s: Session, loop: asyncio.AbstractEventLoop) -> None:
        while True:
            try:
                text = s.win.read(65536)
            except EOFError:
                break
            if text:
                loop.call_soon_threadsafe(s.feed, text.encode("utf-8"))
        loop.call_soon_threadsafe(self._exit, s, s.win.exitstatus or 0)

    def _exit(self, s: Session, code: int) -> None:
        s.exited = code
        s.feed(f"\r\n[process exited with code {code}]\r\n".encode())
        if s.fd >= 0:
            try:
                os.close(s.fd)
            except OSError:
                pass
            s.fd = -1

    def close(self, sid: str) -> None:
        s = self.sessions.pop(sid, None)
        if not s:
            return
        if s.exited is None:
            try:
                if s.win is not None:
                    s.win.terminate(force=True)
                else:
                    os.killpg(s.proc.pid, signal.SIGHUP)
            except (OSError, ProcessLookupError):
                pass
        if s.fd >= 0:
            try:
                asyncio.get_running_loop().remove_reader(s.fd)
            except (RuntimeError, ValueError):
                pass
            try:
                os.close(s.fd)
            except OSError:
                pass

    def close_all(self) -> None:
        for sid in list(self.sessions):
            self.close(sid)


terminals = Terminals()
