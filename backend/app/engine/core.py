import asyncio
import json
import logging
import os
from contextlib import asynccontextmanager
from dataclasses import dataclass, field
from typing import Any

from app.core import config

log = logging.getLogger("formdes.core")


class CoreError(Exception):
    pass


@dataclass
class Reply:
    result: Any
    bin: bytes | None = None
    meta: dict = field(default_factory=dict)


class Core:
    def __init__(self) -> None:
        self.proc: asyncio.subprocess.Process | None = None
        self.lock = asyncio.Lock()
        self.next_id = 0
        self.rev = 0
        self.sel_rev = 0
        self.listeners: set[asyncio.Queue] = set()
        self.version = ""
        self.binary = None
        self._stderr_task: asyncio.Task | None = None
        self.restarts = 0
        self.guard = None

    async def start(self) -> None:
        binary = config.core_binary()
        if binary is None:
            raise CoreError("the layout core is not built: run ./formdes install-core")
        self.binary = binary
        env = dict(os.environ)
        env["FORMDES_FONT_DIRS"] = os.pathsep.join(str(d) for d in config.font_dirs())
        env["FORMDES_DICT_DIRS"] = os.pathsep.join(str(d) for d in config.dictionary_dirs())
        self.proc = await asyncio.create_subprocess_exec(
            str(binary), stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE,
            env=env, limit=1 << 30)
        self._stderr_task = asyncio.create_task(self._drain_stderr())
        hello = await self.call("hello")
        self.version = hello.result.get("core", "")
        log.info("formdes-core %s: %s commands, %s tools", self.version, hello.result.get("commands"), hello.result.get("tools"))

    async def _drain_stderr(self) -> None:
        assert self.proc and self.proc.stderr
        while True:
            line = await self.proc.stderr.readline()
            if not line:
                return
            log.warning(line.decode(errors="replace").rstrip())

    def alive(self) -> bool:
        return self.proc is not None and self.proc.returncode is None

    async def stop(self) -> None:
        if self.proc and self.proc.returncode is None:
            try:
                self.proc.stdin.close()
                await asyncio.wait_for(self.proc.wait(), 3)
            except (asyncio.TimeoutError, OSError):
                self.proc.kill()

    def announce(self, event: dict) -> None:
        for q in list(self.listeners):
            try:
                q.put_nowait(event)
            except asyncio.QueueFull:
                pass

    @asynccontextmanager
    async def exclusive(self):
        async with self.lock:
            if not self.alive():
                raise CoreError("the layout core is not running")
            holder = self
            last: dict = {}

            class Held:
                async def call(self, method: str, params: dict | None = None) -> Reply:
                    holder.next_id += 1
                    req = json.dumps({"id": holder.next_id, "method": method, "params": params or {}}, separators=(",", ":"))
                    holder.proc.stdin.write(req.encode() + b"\n")
                    await holder.proc.stdin.drain()
                    while True:
                        line = await asyncio.wait_for(holder.proc.stdout.readline(), 300)
                        if not line:
                            raise CoreError("the layout core closed its output")
                        head = json.loads(line)
                        data = await holder.proc.stdout.readexactly(head["bin"]) if head.get("bin") else None
                        if head.get("id") in (None, holder.next_id):
                            break
                    last.update({k: head.get(k) for k in ("rev", "selRev", "changed", "damage") if k in head})
                    if not head.get("ok"):
                        raise CoreError(head.get("error") or f"{method} failed")
                    return Reply(head.get("result"), data, {})

            try:
                yield Held()
            finally:
                pass
        await self._notify(last, "engine", "exclusive")

    async def call(self, method: str, params: dict | None = None, *, source: str = "engine", timeout: float = 300) -> Reply:
        if self.guard is not None and source != "guard" and method in ("exec", "batch", "text", "key", "pointer", "findChange"):
            await self.guard(method, params or {}, source)
        async with self.lock:
            if not self.alive():
                if self.proc is not None:
                    self.restarts += 1
                    log.error("formdes-core stopped (exit %s); restarting", self.proc.returncode)
                    await self._restart_unlocked()
                else:
                    raise CoreError("the layout core is not running")
            self.next_id += 1
            req = json.dumps({"id": self.next_id, "method": method, "params": params or {}}, separators=(",", ":"))
            try:
                self.proc.stdin.write(req.encode() + b"\n")
                await self.proc.stdin.drain()
                while True:
                    line = await asyncio.wait_for(self.proc.stdout.readline(), timeout)
                    if not line:
                        raise CoreError("the layout core closed its output")
                    head = json.loads(line)
                    data = await self.proc.stdout.readexactly(head["bin"]) if head.get("bin") else None
                    if head.get("id") in (None, self.next_id):
                        break
                    log.warning("formdes-core: skipped the late reply to request %s", head.get("id"))
            except (BrokenPipeError, ConnectionResetError, asyncio.IncompleteReadError) as exc:
                raise CoreError(f"the layout core stopped while answering {method}: {exc}") from exc
        meta = {k: head.get(k) for k in ("rev", "selRev", "changed", "damage", "requests") if k in head}
        await self._notify(meta, source, method)
        if not head.get("ok"):
            raise CoreError(head.get("error") or f"{method} failed")
        return Reply(head.get("result"), data, meta)

    async def _restart_unlocked(self) -> None:
        self.proc = None
        binary = config.core_binary()
        env = dict(os.environ)
        env["FORMDES_FONT_DIRS"] = os.pathsep.join(str(d) for d in config.font_dirs())
        env["FORMDES_DICT_DIRS"] = os.pathsep.join(str(d) for d in config.dictionary_dirs())
        self.proc = await asyncio.create_subprocess_exec(
            str(binary), stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE,
            env=env, limit=1 << 30)
        self._stderr_task = asyncio.create_task(self._drain_stderr())
        recovery = config.CACHE_DIR / "recovery"
        if recovery.is_dir():
            self.next_id += 1
            self.proc.stdin.write(json.dumps({"id": self.next_id, "method": "exec", "params": {"command": "file.recovery.open", "params": {"dir": str(recovery)}}}).encode() + b"\n")
            await self.proc.stdin.drain()
            await self.proc.stdout.readline()

    async def _notify(self, meta: dict, source: str, method: str) -> None:
        rev, sel = meta.get("rev", self.rev), meta.get("selRev", self.sel_rev)
        if rev == self.rev and sel == self.sel_rev:
            return
        event = {"type": "changed", "rev": rev, "selRev": sel, "docChanged": rev != self.rev, "source": source, "method": method}
        if meta.get("changed"):
            event["damage"] = meta.get("damage")
        self.rev, self.sel_rev = rev, sel
        for q in list(self.listeners):
            try:
                q.put_nowait(event)
            except asyncio.QueueFull:
                pass

    def subscribe(self) -> asyncio.Queue:
        q: asyncio.Queue = asyncio.Queue(maxsize=256)
        self.listeners.add(q)
        return q

    def unsubscribe(self, q: asyncio.Queue) -> None:
        self.listeners.discard(q)

    async def exec(self, command: str, params: dict | None = None, *, source: str = "engine") -> Any:
        return (await self.call("exec", {"command": command, "params": params or {}}, source=source)).result

    async def batch(self, commands: list[dict] | None = None, script: str | None = None, *, atomic: bool = True, source: str = "engine") -> Any:
        p: dict = {"atomic": atomic}
        if script is not None:
            p["script"] = script
        else:
            p["commands"] = commands or []
        return (await self.call("batch", p, source=source)).result

    async def state(self) -> dict:
        return (await self.call("state")).result

    async def inspect(self) -> dict:
        return (await self.call("inspect")).result


core = Core()
