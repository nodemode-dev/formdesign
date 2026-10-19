import asyncio
import os
import secrets
import shutil
import signal
import struct
import subprocess
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
        if p["id"] == "claude":
            from app.services import claude_code
            found = claude_code.info()["installed"]
        else:
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
    title: str = ""
    argv: list = field(default_factory=list)
    cwd: str = ""
    env: dict = field(default_factory=dict)
    cols: int = 100
    rows: int = 30
    gen: int = 0

    @property
    def alive(self) -> bool:
        return self.exited is None

    def write(self, data: bytes) -> None:
        if self.exited is not None:
            return
        if self.win is not None:
            self.win.write(data.decode("utf-8", "replace"))
        else:
            os.write(self.fd, data)

    def resize(self, cols: int, rows: int) -> None:
        cols, rows = max(2, min(500, cols)), max(1, min(300, rows))
        self.cols, self.rows = cols, rows
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
        return [{"id": s.id, "preset": s.preset, "title": s.title, "alive": s.alive, "exited": s.exited} for s in self.sessions.values()]

    def live(self, preset: str) -> Session | None:
        return next((s for s in self.sessions.values() if s.preset == preset and s.alive), None)

    async def reuse(self, preset: str, prompt: str | None) -> Session | None:
        s = self.live(preset)
        if s is not None and prompt:
            s.write(prompt.encode("utf-8"))
            await asyncio.sleep(0.05)
            s.write(b"\r")
        return s

    async def create(self, preset: str, cols: int, rows: int, prompt: str | None = None, resume: str | None = None) -> Session:
        if len(self.sessions) >= MAX_SESSIONS:
            raise ValueError(f"at most {MAX_SESSIONS} terminals can be open")
        spec = next((p for p in PRESETS if p["id"] == preset), None)
        if spec is None:
            raise ValueError(f"unknown preset `{preset}`")
        env, cwd = environment(), str(workdir())
        title = spec["label"]
        if spec["cmd"] is None:
            argv = shell_argv()
            title = "Terminal"
        elif preset == "claude":
            title = "Claude Code (from Chat)" if resume else "Claude Code"
            from app.services import claude_code
            found = claude_code.info(fresh=True)
            if not found["installed"]:
                raise ValueError(claude_code.INSTALL)
            try:
                cwd = str(claude_code.workdir())
            except claude_code.ClaudeError as exc:
                raise ValueError(str(exc)) from None
            argv = claude_code.argv(found["path"], prompt or claude_code.prefs().get("claudeFirstMessage") or None, resume)
            env = claude_code.child_env()
        else:
            exe = shutil.which(spec["cmd"])
            if not exe:
                raise ValueError(f"{spec['label']} is not installed. Install it with: {spec.get('install')}")
            argv = [exe]
        sid = secrets.token_hex(6)
        s = Session(sid, preset, None, title=title, argv=argv, cwd=cwd, env=env, cols=max(2, min(500, cols)), rows=max(1, min(300, rows)))
        try:
            self._start(s, asyncio.get_running_loop())
        except OSError as e:
            raise ValueError(f"{title} cannot start in {cwd}: {e.strerror or e}") from None
        self.sessions[sid] = s
        return s

    def _start(self, s: Session, loop: asyncio.AbstractEventLoop) -> None:
        if sys.platform == "win32":
            from winpty import PtyProcess
            proc = PtyProcess.spawn(s.argv, cwd=s.cwd, env=s.env, dimensions=(s.rows, s.cols))
            s.proc, s.win = proc, proc
            threading.Thread(target=self._pump_win, args=(s, loop, s.gen), daemon=True).start()
            return
        import pty
        master, slave = pty.openpty()
        s.fd = master
        s.resize(s.cols, s.rows)
        try:
            proc = subprocess.Popen(s.argv, stdin=slave, stdout=slave, stderr=slave, cwd=s.cwd, env=s.env,
                                    start_new_session=True, close_fds=True)
        except OSError:
            os.close(slave)
            os.close(master)
            s.fd = -1
            raise
        os.close(slave)
        s.proc = proc
        os.set_blocking(master, False)
        loop.add_reader(master, self._on_readable, s, loop)

    async def restart(self, sid: str) -> Session:
        s = self.sessions.get(sid)
        if s is None:
            raise KeyError(sid)
        loop = asyncio.get_running_loop()
        s.gen += 1
        old, was_alive = s.proc, s.exited is None
        if s.fd >= 0:
            try:
                loop.remove_reader(s.fd)
            except (RuntimeError, ValueError):
                pass
            try:
                os.close(s.fd)
            except OSError:
                pass
            s.fd = -1
        if was_alive:
            try:
                if s.win is not None:
                    s.win.terminate(force=True)
                else:
                    os.killpg(old.pid, signal.SIGKILL)
                    await loop.run_in_executor(None, lambda: old.wait(timeout=3))
            except (OSError, ProcessLookupError, subprocess.TimeoutExpired):
                pass
            if self.sessions.get(sid) is not s:
                raise KeyError(sid)
        s.exited, s.win = None, None
        s.buffer.clear()
        try:
            Path(s.cwd).mkdir(parents=True, exist_ok=True)
            self._start(s, loop)
        except OSError as e:
            self._exit(s, 1)
            raise ValueError(f"{s.title} cannot start in {s.cwd}: {e.strerror or e}") from None
        s.feed(b"\x1bc")
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

    def _pump_win(self, s: Session, loop: asyncio.AbstractEventLoop, gen: int) -> None:
        win = s.win
        while True:
            try:
                text = win.read(65536)
            except EOFError:
                break
            if s.gen != gen:
                return
            if text:
                loop.call_soon_threadsafe(s.feed, text.encode("utf-8"))
        if s.gen == gen:
            loop.call_soon_threadsafe(self._exit, s, win.exitstatus or 0, gen)

    def _exit(self, s: Session, code: int, gen: int | None = None) -> None:
        if s.exited is not None or (gen is not None and gen != s.gen):
            return
        s.exited = code
        line = "Claude Code exited" if s.preset == "claude" else f"Process exited with code {code}"
        s.feed(f"\r\n[{line} · Restart runs it again]\r\n".encode())
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
