import asyncio
import base64
import json
import os
import subprocess
import threading
import time
import uuid
from pathlib import Path
from typing import Any

from app.core import config
from app.engine.core import core
from app.services import claude_code

NOTE = (claude_code.NOTE + " You are answering in FormDesign's Claude Code panel, which shows Markdown to people who may never have used a terminal: "
        "write short paragraphs, use **bold** for panel and setting names and a bulleted list when you changed several things, avoid tables and long code blocks, "
        "say what you changed in a designer's words, and keep tool calls few by batching commands in one run_commands call.")
MAX_MESSAGES = 200
HIDDEN = {"ToolSearch"}
_lock = threading.Lock()
_loop: asyncio.AbstractEventLoop | None = None


def _blank_usage() -> dict:
    return {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "costUsd": 0.0, "context": 0, "window": 0, "model": ""}


STATE: dict[str, Any] = {"session": None, "proc": None, "busy": False, "cancelled": False, "messages": [], "turn": 0, "usage": _blank_usage()}


def _deliver(ev: dict) -> None:
    for q in list(core.listeners):
        try:
            q.put_nowait(ev)
        except asyncio.QueueFull:
            pass


def publish(kind: str, **data: Any) -> None:
    ev = {"type": "claude", "kind": kind, "turn": STATE["turn"], **data}
    if _loop is not None and _loop.is_running():
        _loop.call_soon_threadsafe(_deliver, ev)


def _tool_name(name: str) -> str:
    return name.split("__")[-1] if name.startswith("mcp__") else name


def _brief(value: Any) -> str:
    text = json.dumps(value, ensure_ascii=False) if not isinstance(value, str) else value
    return text if len(text) <= 160 else text[:157] + "..."


def _save_image(block: dict) -> str | None:
    src = block.get("source") or {}
    if src.get("type") != "base64" or not src.get("data"):
        return None
    ext = {"image/png": "png", "image/webp": "webp"}.get(str(src.get("media_type")), "jpg")
    folder = config.DATA_DIR / "Chat"
    try:
        folder.mkdir(parents=True, exist_ok=True)
        path = folder / f"{uuid.uuid4().hex}.{ext}"
        path.write_bytes(base64.b64decode(src["data"]))
    except (OSError, ValueError):
        return None
    return path.name


def image_path(name: str) -> Path | None:
    p = (config.DATA_DIR / "Chat" / Path(name).name)
    return p if p.is_file() else None


def _prune_images() -> None:
    keep = {img for m in STATE["messages"] for p in m.get("parts") or [] if isinstance(p, dict) for img in p.get("images") or []}
    try:
        for f in (config.DATA_DIR / "Chat").iterdir():
            if f.is_file() and f.name not in keep:
                f.unlink()
    except OSError:
        pass


def _usage_of(u: dict) -> tuple[int, int]:
    context = int(u.get("input_tokens") or 0) + int(u.get("cache_creation_input_tokens") or 0) + int(u.get("cache_read_input_tokens") or 0)
    return context, int(u.get("output_tokens") or 0)


def _argv(path: str, text: str, session: str | None) -> list[str]:
    cmd = [path, "-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages",
           "--mcp-config", claude_code.mcp_config(), "--allowedTools", "mcp__formdesign", "--append-system-prompt", NOTE]
    if session:
        cmd += ["--resume", session]
    return claude_code.wrap(cmd + ["--", text])


def _run(text: str, path: str, folder: str) -> None:
    reply: dict[str, Any] = {"role": "assistant", "parts": [], "error": "", "meta": {}}
    tail: list[str] = []
    started = time.time()
    result: dict | None = None
    done_out = 0
    call_out = 0
    usage = STATE["usage"]
    try:
        proc = subprocess.Popen(_argv(path, text, STATE["session"]), cwd=folder, env=claude_code.child_env(), stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1, encoding="utf-8", errors="replace")
        STATE["proc"] = proc

        def drain() -> None:
            for line in proc.stderr:
                tail.append(line.rstrip())
                del tail[:-15]
        threading.Thread(target=drain, daemon=True).start()
        for line in proc.stdout:
            try:
                ev = json.loads(line)
            except ValueError:
                continue
            t = ev.get("type")
            if ev.get("parent_tool_use_id"):
                continue
            if t == "system" and ev.get("subtype") == "init":
                STATE["session"] = ev.get("session_id") or STATE["session"]
                usage["model"] = str(ev.get("model") or usage["model"])
            elif t == "stream_event":
                e = ev.get("event") or {}
                d = e.get("delta") or {}
                if e.get("type") in ("message_start", "message_delta"):
                    u = (e.get("message") or {}).get("usage") if e.get("type") == "message_start" else e.get("usage")
                    if isinstance(u, dict):
                        context, out = _usage_of(u)
                        if e.get("type") == "message_start":
                            done_out += call_out
                            call_out = 0
                            usage["context"] = context or usage["context"]
                        else:
                            call_out = out
                        publish("usage", context=usage["context"], window=usage["window"], output=done_out + call_out, model=usage["model"])
                elif e.get("type") == "content_block_delta" and d.get("type") == "text_delta" and d.get("text"):
                    if not reply["parts"] or reply["parts"][-1]["kind"] != "text":
                        reply["parts"].append({"kind": "text", "text": ""})
                    reply["parts"][-1]["text"] += d["text"]
                    publish("delta", text=d["text"])
            elif t == "assistant":
                for b in (ev.get("message") or {}).get("content") or []:
                    if isinstance(b, dict) and b.get("type") == "tool_use" and b.get("name") not in HIDDEN:
                        part = {"kind": "tool", "id": b.get("id"), "name": _tool_name(str(b.get("name") or "")), "input": _brief(b.get("input") or {}), "status": "running"}
                        reply["parts"].append(part)
                        publish("tool", **{k: part[k] for k in ("id", "name", "input")})
            elif t == "user":
                for b in (ev.get("message") or {}).get("content") or []:
                    if isinstance(b, dict) and b.get("type") == "tool_result" and any(p["kind"] == "tool" and p["id"] == b.get("tool_use_id") for p in reply["parts"]):
                        status = "error" if b.get("is_error") else "done"
                        content = b.get("content") if isinstance(b.get("content"), list) else []
                        images = [p for p in (_save_image(c) for c in content if isinstance(c, dict) and c.get("type") == "image") if p]
                        for p in reply["parts"]:
                            if p["kind"] == "tool" and p["id"] == b.get("tool_use_id"):
                                p["status"] = status
                                if images:
                                    p["images"] = images
                        publish("toolDone", id=b.get("tool_use_id"), status=status, images=images)
            elif t == "result":
                result = ev
                STATE["session"] = ev.get("session_id") or STATE["session"]
                total = ev.get("usage") or {}
                usage["input"] += _usage_of(total)[0]
                usage["output"] += int(total.get("output_tokens") or 0)
                usage["cacheRead"] += int(total.get("cache_read_input_tokens") or 0)
                usage["cacheWrite"] += int(total.get("cache_creation_input_tokens") or 0)
                usage["costUsd"] = round(usage["costUsd"] + float(ev.get("total_cost_usd") or 0), 6)
                per = (ev.get("modelUsage") or {}).get(usage["model"]) or {}
                usage["window"] = int(per.get("contextWindow") or usage["window"] or 0)
        proc.wait(timeout=30)
    except (OSError, subprocess.SubprocessError, claude_code.ClaudeError) as e:
        reply["error"] = f"Claude Code could not run: {e}"
    finally:
        STATE["proc"] = None
    if result is None and not reply["error"]:
        reply["error"] = "Stopped" if STATE.get("cancelled") else "Claude Code stopped before answering" + (f": {tail[-1]}" if tail else "")
    elif result is not None and result.get("is_error"):
        reply["error"] = str(result.get("result") or result.get("subtype") or "Claude Code reported an error")
    tokens = (result or {}).get("usage") or {}
    reply["meta"] = {"seconds": round(time.time() - started, 1), "turns": (result or {}).get("num_turns"), "costUsd": (result or {}).get("total_cost_usd"),
                     "inputTokens": _usage_of(tokens)[0], "outputTokens": int(tokens.get("output_tokens") or 0), "context": usage["context"], "window": usage["window"]}
    with _lock:
        STATE["messages"].append(reply)
        del STATE["messages"][:-MAX_MESSAGES]
        _prune_images()
        STATE["busy"] = False
        STATE["cancelled"] = False
    publish("done", error=reply["error"], meta=reply["meta"], usage=dict(usage))


def state() -> dict:
    return {"busy": STATE["busy"], "messages": STATE["messages"], "session": STATE["session"] or "", "turn": STATE["turn"], "usage": STATE["usage"]}


def send(text: str, context: str = "", note: str = "") -> dict:
    global _loop
    shown = str(text or "").strip()
    if not shown:
        raise claude_code.ClaudeError("Type a message first")
    prompt = f"{shown} ({context.strip()})" if context.strip() else shown
    found = claude_code.info(fresh=True)
    if not found["installed"]:
        raise claude_code.ClaudeError(claude_code.INSTALL)
    folder = str(claude_code.workdir())
    _loop = asyncio.get_running_loop()
    with _lock:
        if STATE["busy"]:
            raise claude_code.ClaudeError("Claude is still answering; wait or press Stop")
        STATE["busy"] = True
        STATE["turn"] += 1
        STATE["messages"].append({"role": "user", "text": shown, "note": note.strip()})
    publish("start", text=shown, note=note.strip())
    threading.Thread(target=_run, args=(prompt, found["path"], folder), daemon=True).start()
    return {"turn": STATE["turn"]}


def cancel() -> dict:
    proc = STATE["proc"]
    if proc is not None and proc.poll() is None:
        STATE["cancelled"] = True
        proc.terminate()
    return {"stopped": proc is not None}


def reset() -> dict:
    global _loop
    cancel()
    try:
        _loop = asyncio.get_running_loop()
    except RuntimeError:
        pass
    with _lock:
        STATE["session"] = None
        STATE["messages"] = []
        STATE["turn"] += 1
        STATE["usage"] = _blank_usage()
        if not STATE["busy"]:
            _prune_images()
    publish("reset")
    return {"reset": True}


def _project_dir(folder: str) -> Path:
    root = Path(os.environ.get("CLAUDE_CONFIG_DIR") or Path.home() / ".claude").expanduser()
    return root / "projects" / "".join(ch if ch.isascii() and ch.isalnum() else "-" for ch in folder)


def _transcript(path: Path) -> list[dict]:
    out: list[dict] = []
    try:
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        return out
    for line in lines:
        try:
            ev = json.loads(line)
        except ValueError:
            continue
        msg = ev.get("message") or {}
        if ev.get("type") not in ("user", "assistant") or ev.get("isSidechain") or ev.get("isMeta"):
            continue
        content = msg.get("content")
        texts = [content] if isinstance(content, str) else [b.get("text", "") for b in content or [] if isinstance(b, dict) and b.get("type") == "text"]
        tools = [b for b in content or [] if isinstance(b, dict) and b.get("type") == "tool_use"] if isinstance(content, list) else []
        text = "\n\n".join(t for t in texts if t and not t.startswith("<"))
        if ev["type"] == "user" and text:
            out.append({"role": "user", "text": text, "note": "from Claude Code"})
        elif ev["type"] == "assistant" and (text or tools):
            parts = [{"kind": "tool", "id": b.get("id"), "name": _tool_name(str(b.get("name") or "")), "input": _brief(b.get("input") or {}), "status": "done"}
                     for b in tools if b.get("name") not in HIDDEN]
            if text:
                parts.append({"kind": "text", "text": text})
            if out and out[-1]["role"] == "assistant":
                out[-1]["parts"] += parts
            elif parts:
                out.append({"role": "assistant", "parts": parts, "error": "", "meta": {}})
    return out[-MAX_MESSAGES:]


def sessions() -> dict:
    folder = _project_dir(str(claude_code.workdir()))
    items = []
    for f in sorted(folder.glob("*.jsonl"), key=lambda p: p.stat().st_mtime, reverse=True)[:20]:
        first = next((m["text"] for m in _transcript(f) if m["role"] == "user"), "")
        items.append({"id": f.stem, "modified": f.stat().st_mtime, "title": first[:90], "current": f.stem == STATE["session"]})
    return {"sessions": items, "folder": str(folder)}


def adopt(session: str = "") -> dict:
    global _loop
    if STATE["busy"]:
        raise claude_code.ClaudeError("Claude is still answering; wait or press Stop")
    listed = sessions()["sessions"]
    pick = next((s for s in listed if s["id"] == session), None) if session else (listed[0] if listed else None)
    if pick is None:
        raise claude_code.ClaudeError("No Claude Code conversation was found in FormDesign's agent folder yet")
    messages = _transcript(_project_dir(str(claude_code.workdir())) / f"{pick['id']}.jsonl")
    _loop = asyncio.get_running_loop()
    with _lock:
        STATE["session"] = pick["id"]
        STATE["messages"] = messages
        STATE["turn"] += 1
        STATE["usage"] = _blank_usage()
    publish("adopted", session=pick["id"], count=len(messages))
    return {"session": pick["id"], "messages": len(messages)}
