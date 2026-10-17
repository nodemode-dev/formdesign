"""The built-in assistant and the model-backed helpers (alt text, auto style, image generation).

Nothing here runs unless the person turned it on in Preferences > AI. The settings (with the API keys) live in one file in the
user data dir, mode 600; the window only ever learns whether a key is set. The assistant calls the same tools MCP clients call,
through mcp.call_tool with the client name "assistant", so Agent Connect's policy and approvals apply to it.
"""
import asyncio
import base64
import io
import json
import os
import re
import time
import urllib.parse
from pathlib import Path

import httpx
from PIL import Image

from app.core import config
from app.engine.core import CoreError, core
from app.services import agents as A

SETTINGS = config.DATA_DIR / "ai.json"
CLIENT = "assistant"
PROVIDERS = ("anthropic", "openai", "ollama")
IMAGE_PROVIDERS = ("openai", "stability")
DEFAULT_URL = {"anthropic": "https://api.anthropic.com", "openai": "https://api.openai.com/v1", "ollama": "http://127.0.0.1:11434"}
DEFAULT_IMAGE_URL = {"openai": "https://api.openai.com/v1", "stability": "https://api.stability.ai"}
MAX_STEPS = 24
MAX_HISTORY = 48
RESULT_CHARS = 12000
DEFAULTS = {"enabled": False, "provider": "anthropic", "model": "", "baseUrl": "", "apiKey": "",
            "image": {"provider": "openai", "model": "", "baseUrl": "", "apiKey": ""}}

SYSTEM = """You are the assistant inside FormDesign, a page-layout app. You work on the document the person has open by calling the same tools an MCP client would (outline, inspect, look, run_commands, find_commands, set_slots, make_layout and the rest).
Be brief. Read before you change: outline shows what is on the page, look shows it. Make changes with run_commands (one undoable batch) and check the result with look when the change is visual.
Some steps (publishing, overwriting a file, deleting pages) wait for the person's approval; if a tool says it was not approved, say so and ask what they want instead of retrying.
The context block at the end of each message names the current page and the selected objects. Do not use emoji or em dashes."""


class AIError(Exception):
    pass


# ---------------------------------------------------------------- settings

def _read() -> dict:
    try:
        d = json.loads(SETTINGS.read_text())
        return d if isinstance(d, dict) else {}
    except (OSError, ValueError):
        return {}


def _write(d: dict) -> None:
    SETTINGS.parent.mkdir(parents=True, exist_ok=True)
    tmp = SETTINGS.with_name(SETTINGS.name + ".tmp")
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as f:
        f.write(json.dumps(d, indent=1))
    os.chmod(tmp, 0o600)
    os.replace(tmp, SETTINGS)
    try:
        os.chmod(SETTINGS, 0o600)
    except OSError:
        pass


def settings() -> dict:
    d = _read()
    img = {**DEFAULTS["image"], **(d.get("image") if isinstance(d.get("image"), dict) else {})}
    return {**DEFAULTS, **{k: v for k, v in d.items() if k != "image"}, "image": img}


def _url(raw, what: str) -> str:
    raw = str(raw or "").strip().rstrip("/")
    if not raw:
        return ""
    p = urllib.parse.urlparse(raw)
    if p.scheme not in ("http", "https") or not p.hostname:
        raise AIError(f"{what} must be an http or https address")
    if p.username or p.password:
        raise AIError(f"{what} must not carry a user name or password; put the key in the API key field")
    return raw


def public() -> dict:
    s = settings()
    img = s["image"]
    return {"enabled": bool(s["enabled"]), "provider": s["provider"], "model": s["model"], "baseUrl": s["baseUrl"], "apiKeySet": bool(s["apiKey"]),
            "defaultUrls": DEFAULT_URL, "providers": list(PROVIDERS),
            "image": {"provider": img["provider"], "model": img["model"], "baseUrl": img["baseUrl"], "apiKeySet": bool(img["apiKey"]),
                      "defaultUrls": DEFAULT_IMAGE_URL, "providers": list(IMAGE_PROVIDERS)},
            "configured": configured(s), "imageConfigured": image_configured(s)}


def update(body: dict) -> dict:
    """Merge a settings form. A blank apiKey keeps the saved one; clearKey removes it; keys are never read back."""
    s = settings()
    if "enabled" in body:
        s["enabled"] = bool(body["enabled"])
    if body.get("provider") in PROVIDERS:
        s["provider"] = body["provider"]
    if "model" in body:
        s["model"] = str(body["model"] or "").strip()[:200]
    if "baseUrl" in body:
        s["baseUrl"] = _url(body["baseUrl"], "The base URL")
    if body.get("clearKey"):
        s["apiKey"] = ""
    elif str(body.get("apiKey") or "").strip():
        s["apiKey"] = str(body["apiKey"]).strip()[:600]
    im = body.get("image")
    if isinstance(im, dict):
        t = dict(s["image"])
        if im.get("provider") in IMAGE_PROVIDERS:
            t["provider"] = im["provider"]
        if "model" in im:
            t["model"] = str(im["model"] or "").strip()[:200]
        if "baseUrl" in im:
            t["baseUrl"] = _url(im["baseUrl"], "The image base URL")
        if im.get("clearKey"):
            t["apiKey"] = ""
        elif str(im.get("apiKey") or "").strip():
            t["apiKey"] = str(im["apiKey"]).strip()[:600]
        s["image"] = t
    _write(s)
    return public()


def configured(s: dict | None = None) -> bool:
    s = s or settings()
    if not s["enabled"] or not s["model"]:
        return False
    return s["provider"] == "ollama" or bool(s["apiKey"]) or bool(s["baseUrl"])


def image_configured(s: dict | None = None) -> bool:
    s = s or settings()
    if not s["enabled"]:
        return False
    k, _ = _image_target(s)
    return bool(k["apiKey"] or k["baseUrl"])


def _image_target(s: dict) -> tuple[dict, str]:
    """The image provider's settings; a blank OpenAI-style key and address fall back to the chat provider's."""
    im = dict(s["image"])
    if im["provider"] == "openai":
        if not im["apiKey"] and s["provider"] == "openai":
            im["apiKey"] = s["apiKey"]
        if not im["baseUrl"] and s["provider"] == "openai":
            im["baseUrl"] = s["baseUrl"]
    im["baseUrl"] = im["baseUrl"] or DEFAULT_IMAGE_URL[im["provider"]]
    return im, im["provider"]


# ---------------------------------------------------------------- providers

def _client() -> httpx.AsyncClient:
    return httpx.AsyncClient(timeout=httpx.Timeout(180.0, connect=10.0), follow_redirects=False)


async def _post(url: str, headers: dict, **kw) -> httpx.Response:
    try:
        async with _client() as c:
            r = await c.post(url, headers=headers, **kw)
    except httpx.HTTPError as exc:
        raise AIError(f"could not reach the model at {urllib.parse.urlparse(url).netloc}: {exc.__class__.__name__}") from exc
    if r.status_code >= 400:
        detail = ""
        try:
            j = r.json()
            e = j.get("error", j)
            detail = e.get("message") if isinstance(e, dict) else str(e)
        except ValueError:
            detail = r.text[:200]
        raise AIError(f"the model answered {r.status_code}: {str(detail)[:300]}")
    return r


def _tools_spec() -> list[dict]:
    from app.routers.mcp import TOOLS
    return [{"name": t["name"], "description": t["description"], "schema": t["inputSchema"]} for t in TOOLS]


def _img_blocks(m: dict) -> list[dict]:
    return m.get("images") or []


def _to_anthropic(conv: list[dict]) -> list[dict]:
    out: list[dict] = []
    for m in conv:
        if m["role"] == "user":
            out.append({"role": "user", "content": [{"type": "text", "text": m["text"]}] + [{"type": "image", "source": {"type": "base64", "media_type": i["mime"], "data": i["data"]}} for i in _img_blocks(m)]})
        elif m["role"] == "assistant":
            blocks = ([{"type": "text", "text": m["text"]}] if m.get("text") else []) + [{"type": "tool_use", "id": c["id"], "name": c["name"], "input": c["args"]} for c in m.get("calls") or []]
            out.append({"role": "assistant", "content": blocks or [{"type": "text", "text": "."}]})
        else:
            content = [{"type": "text", "text": b["text"]} if b["type"] == "text" else {"type": "image", "source": {"type": "base64", "media_type": b["mime"], "data": b["data"]}} for b in m["content"]]
            blk = {"type": "tool_result", "tool_use_id": m["id"], "content": content or [{"type": "text", "text": "done"}], **({"is_error": True} if m.get("error") else {})}
            if out and out[-1]["role"] == "user" and out[-1].get("_results"):
                out[-1]["content"].append(blk)
            else:
                out.append({"role": "user", "content": [blk], "_results": True})
    for o in out:
        o.pop("_results", None)
    return out


async def _anthropic(s: dict, conv: list[dict], tools: list[dict], system: str) -> dict:
    base = s["baseUrl"] or DEFAULT_URL["anthropic"]
    body = {"model": s["model"], "max_tokens": 4096, "system": system, "messages": _to_anthropic(conv)}
    if tools:
        body["tools"] = [{"name": t["name"], "description": t["description"], "input_schema": t["schema"]} for t in tools]
    r = await _post(f"{base}/v1/messages", {"x-api-key": s["apiKey"], "anthropic-version": "2023-06-01", "content-type": "application/json"}, json=body)
    j = r.json()
    text = "".join(b.get("text", "") for b in j.get("content", []) if b.get("type") == "text")
    calls = [{"id": b["id"], "name": b["name"], "args": b.get("input") or {}} for b in j.get("content", []) if b.get("type") == "tool_use"]
    return {"text": text, "calls": calls}


def _data_uri(i: dict) -> str:
    return f"data:{i['mime']};base64,{i['data']}"


def _to_openai(conv: list[dict], system: str) -> list[dict]:
    out: list[dict] = [{"role": "system", "content": system}]
    pending: list[dict] = []
    for m in conv:
        if m["role"] != "tool" and pending:
            out.append({"role": "user", "content": pending})
            pending = []
        if m["role"] == "user":
            imgs = _img_blocks(m)
            out.append({"role": "user", "content": m["text"] if not imgs else [{"type": "text", "text": m["text"]}] + [{"type": "image_url", "image_url": {"url": _data_uri(i)}} for i in imgs]})
        elif m["role"] == "assistant":
            a: dict = {"role": "assistant", "content": m.get("text") or None}
            if m.get("calls"):
                a["tool_calls"] = [{"id": c["id"], "type": "function", "function": {"name": c["name"], "arguments": json.dumps(c["args"])}} for c in m["calls"]]
            out.append(a)
        else:
            texts = [b["text"] for b in m["content"] if b["type"] == "text"]
            out.append({"role": "tool", "tool_call_id": m["id"], "content": "\n".join(texts) or "done"})
            for b in m["content"]:
                if b["type"] == "image":
                    pending.append({"type": "text", "text": f"Image returned by {m.get('name', 'the tool')}:"})
                    pending.append({"type": "image_url", "image_url": {"url": _data_uri(b)}})
    if pending:
        out.append({"role": "user", "content": pending})
    return out


async def _openai(s: dict, conv: list[dict], tools: list[dict], system: str) -> dict:
    base = s["baseUrl"] or DEFAULT_URL["openai"]
    body: dict = {"model": s["model"], "messages": _to_openai(conv, system)}
    if tools:
        body["tools"] = [{"type": "function", "function": {"name": t["name"], "description": t["description"], "parameters": t["schema"]}} for t in tools]
    headers = {"content-type": "application/json", **({"authorization": f"Bearer {s['apiKey']}"} if s["apiKey"] else {})}
    j = (await _post(f"{base}/chat/completions", headers, json=body)).json()
    msg = ((j.get("choices") or [{}])[0]).get("message") or {}
    calls = []
    for c in msg.get("tool_calls") or []:
        fn = c.get("function") or {}
        try:
            args = json.loads(fn.get("arguments") or "{}")
        except ValueError:
            args = {}
        calls.append({"id": c.get("id") or f"call_{len(calls)}", "name": fn.get("name", ""), "args": args if isinstance(args, dict) else {}})
    content = msg.get("content")
    if isinstance(content, list):
        content = "".join(p.get("text", "") for p in content if isinstance(p, dict))
    return {"text": content or "", "calls": calls}


def _to_ollama(conv: list[dict], system: str) -> list[dict]:
    out: list[dict] = [{"role": "system", "content": system}]
    for m in conv:
        if m["role"] == "user":
            out.append({"role": "user", "content": m["text"], **({"images": [i["data"] for i in _img_blocks(m)]} if _img_blocks(m) else {})})
        elif m["role"] == "assistant":
            a: dict = {"role": "assistant", "content": m.get("text") or ""}
            if m.get("calls"):
                a["tool_calls"] = [{"function": {"name": c["name"], "arguments": c["args"]}} for c in m["calls"]]
            out.append(a)
        else:
            out.append({"role": "tool", "tool_name": m.get("name", ""), "content": "\n".join(b["text"] for b in m["content"] if b["type"] == "text") or "done"})
            imgs = [b["data"] for b in m["content"] if b["type"] == "image"]
            if imgs:
                out.append({"role": "user", "content": f"Image returned by {m.get('name', 'the tool')}.", "images": imgs})
    return out


async def _ollama(s: dict, conv: list[dict], tools: list[dict], system: str) -> dict:
    base = s["baseUrl"] or DEFAULT_URL["ollama"]
    body: dict = {"model": s["model"], "messages": _to_ollama(conv, system), "stream": False}
    if tools:
        body["tools"] = [{"type": "function", "function": {"name": t["name"], "description": t["description"], "parameters": t["schema"]}} for t in tools]
    headers = {"content-type": "application/json", **({"authorization": f"Bearer {s['apiKey']}"} if s["apiKey"] else {})}
    j = (await _post(f"{base}/api/chat", headers, json=body)).json()
    msg = j.get("message") or {}
    calls = []
    for c in msg.get("tool_calls") or []:
        fn = c.get("function") or {}
        args = fn.get("arguments")
        if isinstance(args, str):
            try:
                args = json.loads(args)
            except ValueError:
                args = {}
        calls.append({"id": f"call_{int(time.time() * 1000)}_{len(calls)}", "name": fn.get("name", ""), "args": args if isinstance(args, dict) else {}})
    return {"text": msg.get("content") or "", "calls": calls}


async def complete(conv: list[dict], tools: list[dict], system: str) -> dict:
    s = settings()
    if not configured(s):
        raise AIError("AI is off or not set up: turn it on in Preferences > AI and choose a provider, model and key")
    fn = {"anthropic": _anthropic, "openai": _openai, "ollama": _ollama}[s["provider"]]
    return await fn(s, conv, tools, system)


async def ask_text(system: str, prompt: str, images: list[dict] | None = None) -> str:
    r = await complete([{"role": "user", "text": prompt, "images": images or []}], [], system)
    return (r["text"] or "").strip()


def jpeg_1024(data: bytes) -> bytes:
    """An image downscaled to at most 1024 px on its long side, as a JPEG (transparency flattened onto white)."""
    try:
        im = Image.open(io.BytesIO(data))
        im.load()
    except Exception as exc:
        raise AIError(f"the picture could not be read: {exc.__class__.__name__}") from exc
    if im.mode in ("RGBA", "LA", "P"):
        im = im.convert("RGBA")
        bg = Image.new("RGB", im.size, (255, 255, 255))
        bg.paste(im, mask=im.getchannel("A"))
        im = bg
    else:
        im = im.convert("RGB")
    im.thumbnail((1024, 1024), Image.Resampling.LANCZOS)
    buf = io.BytesIO()
    im.save(buf, "JPEG", quality=82)
    return buf.getvalue()


def thumb_uri(jpeg: bytes) -> str:
    im = Image.open(io.BytesIO(jpeg))
    im.thumbnail((200, 200))
    buf = io.BytesIO()
    im.save(buf, "JPEG", quality=70)
    return "data:image/jpeg;base64," + base64.b64encode(buf.getvalue()).decode()


# ---------------------------------------------------------------- the assistant

conversations: dict[int, list[dict]] = {}
transcripts: dict[int, list[dict]] = {}
runs: dict[int, asyncio.Task] = {}


async def active_uid() -> int:
    st = await core.state()
    i = st.get("active")
    if i is None or not st.get("docs"):
        raise AIError("open a document first")
    return st["docs"][i]["uid"]


async def context(page: int | None) -> str:
    st = await core.state()
    docs = st.get("docs") or []
    title = docs[st["active"]]["title"] if st.get("active") is not None and docs else "no document"
    bits = [f"document {title}", f"current page {page or 1}"]
    sel = st.get("selection") or {}
    ids = sel.get("items") or []
    if ids:
        ins = await core.inspect()
        rows = []
        for sp in ins.get("spreads") or []:
            for it in sp.get("items") or []:
                if it["id"] in ids:
                    rows.append(f"#{it['id']} {str(it.get('kind', '')).replace(' frame', '')} {[round(v) for v in it.get('bounds', [])]}")
        bits.append("selected: " + "; ".join(rows[:12]))
    elif sel.get("text"):
        bits.append(f"text cursor in story {sel['text'].get('story')}")
    else:
        bits.append("nothing selected")
    return "<context>" + ", ".join(bits) + "</context>"


def _blocks(out: dict) -> list[dict]:
    blocks: list[dict] = []
    for c in out.get("content") or []:
        if c.get("type") == "text":
            blocks.append({"type": "text", "text": str(c.get("text", ""))[:RESULT_CHARS]})
        elif c.get("type") == "image" and sum(1 for b in blocks if b["type"] == "image") < 2:
            blocks.append({"type": "image", "data": c.get("data", ""), "mime": c.get("mimeType", "image/png")})
    return blocks


def _trim(conv: list[dict]) -> None:
    while len(conv) > MAX_HISTORY:
        conv.pop(0)
        while conv and conv[0]["role"] != "user":
            conv.pop(0)


async def _loop(uid: int, text: str, page: int | None, emit) -> None:
    from app.routers import mcp
    conv = conversations.setdefault(uid, [])
    log = transcripts.setdefault(uid, [])
    A.clients.setdefault(CLIENT, {"name": CLIENT, "version": None, "transport": "assistant", "firstSeen": time.time(), "lastSeen": 0.0, "calls": 0})["lastSeen"] = time.time()
    conv.append({"role": "user", "text": f"{text}\n\n{await context(page)}"})
    log.append({"kind": "user", "text": text})
    # Each message the person sends starts a turn: everything the assistant changes answering it is one Undo.
    turn = f"{CLIENT}:{uid}"
    A.end_turns(turn)
    tools = _tools_spec()
    system = SYSTEM + "\n\n" + mcp.INSTRUCTIONS
    open_calls: dict[str, dict] = {}
    try:
        for _ in range(MAX_STEPS):
            _trim(conv)
            r = await complete(conv, tools, system)
            conv.append({"role": "assistant", "text": r["text"], "calls": r["calls"]})
            if r["text"]:
                log.append({"kind": "assistant", "text": r["text"]})
                await emit({"type": "text", "text": r["text"]})
            if not r["calls"]:
                return
            open_calls = {c["id"]: c for c in r["calls"]}
            for c in r["calls"]:
                step = {"kind": "tool", "id": c["id"], "name": c["name"], "args": A.short(c["args"], 300)}
                await emit({"type": "tool", **{k: v for k, v in step.items() if k != "kind"}})
                if c["name"] not in mcp.NAMES:
                    out = mcp._fail(f"No tool named {c['name']}. The tools are: {', '.join(sorted(mcp.NAMES))}")
                else:
                    out = await mcp.call_tool(c["name"], c["args"], CLIENT, turn)
                blocks = _blocks(out)
                note = next((b["text"] for b in blocks if b["type"] == "text"), "image")[:240]
                conv.append({"role": "tool", "id": c["id"], "name": c["name"], "content": blocks, "error": bool(out.get("isError"))})
                open_calls.pop(c["id"], None)
                step.update(ok=not out.get("isError"), note=note)
                log.append(step)
                await emit({"type": "tool_result", "id": c["id"], "ok": not out.get("isError"), "note": note})
        await emit({"type": "text", "text": "I stopped after many steps. Tell me to continue if you want more."})
    except asyncio.CancelledError:
        for cid, c in open_calls.items():
            conv.append({"role": "tool", "id": cid, "name": c["name"], "content": [{"type": "text", "text": "Stopped by the person before this ran."}], "error": True})
        log.append({"kind": "assistant", "text": "Stopped."})
        raise


async def chat(message: str, page: int | None):
    """Runs one turn and yields events for the panel; closing the iterator stops the run."""
    uid = await active_uid()
    if uid in runs and not runs[uid].done():
        raise AIError("the assistant is still working on this document; stop it first")
    if not configured():
        raise AIError("The assistant is off or not set up. Turn it on in Preferences > AI.")
    q: asyncio.Queue = asyncio.Queue()
    seen: set[str] = set()

    async def emit(ev: dict) -> None:
        await q.put(ev)

    async def work() -> None:
        try:
            await _loop(uid, message, page, emit)
            await q.put({"type": "done"})
        except asyncio.CancelledError:
            await q.put({"type": "stopped"})
        except AIError as exc:
            await q.put({"type": "error", "error": str(exc)})
        except Exception as exc:  # a failed turn should reach the panel, not the log alone
            await q.put({"type": "error", "error": f"{exc.__class__.__name__}: {exc}"})
        finally:
            await q.put(None)

    async def watch() -> None:
        while True:
            for aid, a in list(A.approvals.items()):
                if a["client"] == CLIENT and aid not in seen:
                    seen.add(aid)
                    await q.put({"type": "approval", "id": aid, "tool": a["tool"], "reason": a["reason"], "args": a["args"]})
            await asyncio.sleep(0.15)

    task = asyncio.create_task(work())
    runs[uid] = task
    watcher = asyncio.create_task(watch())
    try:
        while True:
            ev = await q.get()
            if ev is None:
                return
            yield ev
    finally:
        watcher.cancel()
        if not task.done():
            task.cancel()
            await asyncio.gather(task, return_exceptions=True)
        runs.pop(uid, None)


async def stop() -> bool:
    uid = await active_uid()
    t = runs.get(uid)
    if t and not t.done():
        t.cancel()
        return True
    return False


async def history() -> dict:
    uid = await active_uid()
    t = runs.get(uid)
    return {"doc": uid, "running": bool(t and not t.done()), "items": transcripts.get(uid, [])[-200:]}


async def reset() -> None:
    uid = await active_uid()
    conversations.pop(uid, None)
    transcripts.pop(uid, None)


async def test() -> str:
    return await ask_text("Reply with the single word ready.", "Are you there?")


# ---------------------------------------------------------------- alt text

async def _selection() -> dict:
    return (await core.state()).get("selection") or {}


async def _image_blocks() -> list[dict]:
    web = (await core.call("web")).result
    return [b for b in web.get("blocks", []) if b.get("kind") == "image" and b.get("item") is not None]


async def asset_bytes(asset: int) -> bytes:
    r = await core.call("web.asset", {"asset": asset})
    return r.bin or b""


async def alt_propose(mode: str = "auto") -> dict:
    """Alt text for the selected images, or for every image without any. Nothing is written."""
    if not configured():
        raise AIError("AI is off or not set up: turn it on in Preferences > AI. A vision-capable model is needed.")
    sel = set((await _selection()).get("items") or [])
    imgs = await _image_blocks()
    chosen = [b for b in imgs if b["item"] in sel] if (mode != "missing" and sel) else []
    scope = "selected"
    if not chosen:
        chosen = [b for b in imgs if not (b.get("alt") or "").strip()]
        scope = "missing"
    web = (await core.call("web")).result
    title = web.get("title") or ""
    sem = asyncio.Semaphore(3)
    system = "You write alt text for images in a page layout, for screen readers. Describe what the picture shows and why it is there, in one plain sentence under 125 characters. Do not start with 'image of' or 'picture of'. No quotation marks, no emoji."

    async def one(b: dict) -> dict:
        row = {"id": b["item"], "asset": b["asset"], "name": b.get("name", ""), "current": b.get("alt", ""), "proposed": "", "error": "", "thumb": ""}
        async with sem:
            try:
                jpg = jpeg_1024(await asset_bytes(b["asset"]))
                row["thumb"] = thumb_uri(jpg)
                prompt = f"Write the alt text for this image. File name: {b.get('name', '')}. Document: {title}."
                row["proposed"] = re.sub(r"\s+", " ", await ask_text(system, prompt, [{"mime": "image/jpeg", "data": base64.b64encode(jpg).decode()}])).strip().strip('"')[:300]
            except (AIError, CoreError) as exc:
                row["error"] = str(exc)
        return row

    rows = await asyncio.gather(*(one(b) for b in chosen))
    return {"scope": scope, "items": rows, "total": len(imgs)}


async def alt_apply(items: list[dict]) -> dict:
    cmds = [{"command": "object.altText", "params": {"text": str(i.get("text", ""))[:500], "ids": [int(i["id"])]}} for i in items if str(i.get("text", "")).strip()]
    if not cmds:
        return {"written": 0}
    await core.batch(cmds, source="assistant")
    return {"written": len(cmds)}


# ---------------------------------------------------------------- auto style

ROLES = ("title", "h1", "h2", "h3", "body", "caption", "quote", "list")
STYLE_WORDS = {
    "title": r"^(title|headline|display)\b|\bh0\b",
    "h1": r"^(heading|head|header|h)\s*1\b|^head ?a$|^heading$",
    "h2": r"^(heading|head|header|h)\s*2\b|sub-?head|^head ?b$",
    "h3": r"^(heading|head|header|h)\s*3\b|^head ?c$",
    "body": r"^(body|text|paragraph|normal)\b",
    "caption": r"caption|credit|figure",
    "quote": r"quote|pull",
    "list": r"\blist|bullet|numbered",
}
DEFAULT_STYLES = {"Title": ("title", {"size": 1.0}), "Heading 1": ("h1", {"size": 2.0}), "Heading 2": ("h2", {"size": 1.5}), "Heading 3": ("h3", {"size": 1.2})}
UNSTYLED = ("[No Paragraph Style]", "[Basic Paragraph]")
BULLETS = "".join(chr(c) for c in (0x2022, 0x25CF, 0x25AA, 0x2013, 0x2014))
OPEN_Q = "".join(chr(c) for c in (0x201C, 0x2018)) + '"'
CLOSE_Q = "".join(chr(c) for c in (0x201D, 0x2019)) + '"'
DASHES = "".join(chr(c) for c in (0x2013, 0x2014)) + "-"
LIST_RE = re.compile(r"^\s*(?:[" + BULLETS + r"*-]|\(?\d{1,3}[.)]|\(?[a-z][.)]|\(?[ivx]{1,4}[.)])\s+", re.I)
MULTI_NUM_RE = re.compile(r"^\s*(\d+(?:\.\d+)+)\.?\s+\S")
CAPTION_RE = re.compile(r"^\s*(?:fig(?:ure)?\.?|table|photo|image|chart|map|source|credit|photograph)\s*\d*[.:)]?\s", re.I)


def _eff_size(para: dict, styles: dict) -> tuple[float, bool]:
    st = styles.get(para.get("style")) or styles.get("[Basic Paragraph]") or {}
    base = float(st.get("size") or 12.0)
    runs = [r for r in para.get("runs", []) if r.get("t")]
    total = sum(len(r["t"]) for r in runs) or 1
    size = sum(base * float(r.get("size", 1.0)) * len(r["t"]) for r in runs) / total if runs else base
    bold = (int(st.get("weight") or 400) >= 600) or (sum(len(r["t"]) for r in runs if r.get("b")) / total > 0.6)
    return round(size, 1), bold


def para_facts(paras: list[dict], styles: dict) -> list[dict]:
    out = []
    for i, p in enumerate(paras):
        text = "".join(r.get("t", "") for r in p.get("runs", [])).strip()
        size, bold = _eff_size(p, styles)
        out.append({"i": i, "text": text, "size": size, "bold": bold, "style": p.get("style"), "list": bool(p.get("list"))})
    return out


def heuristic_roles(facts: list[dict]) -> dict[int, str]:
    """Classify paragraphs without a model: from type size against the body size, length, quotes, captions and numbering."""
    weight: dict[float, int] = {}
    for f in facts:
        weight[f["size"]] = weight.get(f["size"], 0) + len(f["text"])
    body = max(weight, key=lambda k: weight[k]) if weight else 12.0
    sizes = sorted({f["size"] for f in facts if f["text"]})
    varied = len(sizes) > 1
    roles: dict[int, str] = {}
    live = [f for f in facts if f["text"]]
    first = live[0]["i"] if live else -1
    for n, f in enumerate(live):
        t, size = f["text"], f["size"]
        nxt = live[n + 1] if n + 1 < len(live) else None
        short = len(t) <= 70 and not re.search(r"[.!?]\s*$", t) and "\n" not in t
        long_next = nxt is not None and len(nxt["text"]) > max(80, len(t) * 1.6)
        ratio = size / body if body else 1.0
        m = MULTI_NUM_RE.match(t)
        numbered = bool(LIST_RE.match(t)) or f["list"]
        lone = numbered and short and long_next and not f["list"] and not (n > 0 and LIST_RE.match(live[n - 1]["text"])) and not (nxt and LIST_RE.match(nxt["text"]))
        if m and short:
            role = "heading_num"
        elif numbered and not lone:
            role = "list"
        elif lone:
            role = "h2"
        elif t[:1] in OPEN_Q and len(t) < 420 and (t[-1:] in CLOSE_Q or re.search("[" + CLOSE_Q + r"]\s*[" + re.escape(DASHES) + r"]\s*\S", t)):
            role = "quote"
        elif CAPTION_RE.match(t) and len(t) < 240 or (ratio < 0.92 and len(t) < 160):
            role = "caption"
        elif varied and ratio >= 1.7 and len(t) < 120:
            role = "title" if f["i"] == first else "h1"
        elif varied and ratio >= 1.35 and len(t) < 140:
            role = "h1" if f["i"] == first else "h2"
        elif varied and ratio >= 1.12 and len(t) < 140:
            role = "h2" if ratio >= 1.2 else "h3"
        elif f["bold"] and short and len(t) < 90 and ratio >= 0.99 and (long_next or nxt is None):
            role = "h3"
        elif short and long_next and len(t) < (60 if not varied else 30):
            role = ("h1" if f["i"] == first else "h2") if not varied else "h3"
        else:
            role = "body"
        if role == "heading_num":
            depth = m.group(1).count(".") + 1 if m else 1
            role = "h2" if depth <= 2 else "h3"
        roles[f["i"]] = role
    return roles


async def ai_roles(facts: list[dict]) -> dict[int, str]:
    live = [f for f in facts if f["text"]]
    lines = [f'[{f["i"]}] size={f["size"]} bold={int(f["bold"])} len={len(f["text"])}: {json.dumps(f["text"][:160], ensure_ascii=False)}' for f in live]
    system = ("You classify the paragraphs of a document for a page layout. For each numbered paragraph choose one role from: title, h1, h2, h3, body, caption, quote, list. "
              "Use the type size, boldness, length and wording. Answer with only a JSON array like [{\"i\":0,\"role\":\"h1\"}] covering every paragraph.")
    raw = await ask_text(system, "\n".join(lines))
    m = re.search(r"\[.*\]", raw, re.S)
    if not m:
        raise AIError("the model did not return a list of roles")
    try:
        arr = json.loads(m.group(0))
    except ValueError as exc:
        raise AIError("the model's list of roles could not be read") from exc
    out = {}
    for r in arr:
        if isinstance(r, dict) and isinstance(r.get("i"), int) and r.get("role") in ROLES:
            out[r["i"]] = r["role"]
    if not out:
        raise AIError("the model returned no usable roles")
    return out


def resolve_styles(names: list[str]) -> dict[str, str]:
    """Which of the document's paragraph styles stands for each role (by name); roles without one are absent."""
    found: dict[str, str] = {}
    for role, pat in STYLE_WORDS.items():
        for n in names:
            if n not in UNSTYLED and re.search(pat, n.strip(), re.I):
                found[role] = n
                break
    return found


async def _story_targets(sel: dict, blocks: list[dict]) -> list[dict]:
    stories = [b for b in blocks if b.get("kind") == "story"]
    if sel.get("text") and sel["text"].get("story") is not None:
        return [b for b in stories if b["story"] == sel["text"]["story"]]
    items = set(sel.get("items") or [])
    if items:
        picked = [b for b in stories if items & set(b.get("frames") or []) or b.get("item") in items]
        if picked:
            return picked
    return [b for b in stories if all(p.get("style") in UNSTYLED for p in b.get("paras", []))]


async def autostyle_plan(use_ai: bool = True, create_missing: bool = True) -> dict:
    web = (await core.call("web")).result
    blocks = web.get("blocks", [])
    sel = await _selection()
    targets = await _story_targets(sel, blocks)
    if not targets:
        raise AIError("There is no unstyled text to style: select a text frame, or place the cursor in a story")
    ins = await core.inspect()
    names = [s if isinstance(s, str) else s.get("name") for s in ins.get("paragraphStyles") or []]
    have = resolve_styles(names)
    pstyles = (web.get("styles") or {}).get("paragraph") or {}
    ai_on = use_ai and configured()
    mode, note = "heuristic", ""
    plan = []
    for b in targets:
        facts = para_facts(b["paras"], pstyles)
        roles: dict[int, str] = {}
        if ai_on:
            try:
                roles = await ai_roles(facts)
                mode = "ai"
            except AIError as exc:
                note = str(exc)
        base = heuristic_roles(facts)
        for f in facts:
            if f["text"]:
                roles.setdefault(f["i"], base.get(f["i"], "body"))
        offs = (await core.call("story.paras", {"story": b["story"]})).result["paras"]
        if len(offs) != len(facts):
            continue
        plan.append({"story": b["story"], "paras": [{"i": f["i"], "text": f["text"][:90], "role": roles[f["i"]], "start": offs[f["i"]]["start"], "size": f["size"]} for f in facts if f["text"]]})
    used = {p["role"] for s in plan for p in s["paras"]}
    body_size = 12.0
    if pstyles:
        body_size = float((pstyles.get(have.get("body") or "[Basic Paragraph]") or {}).get("size") or 12.0)
    return {"mode": mode, "note": note, "stories": plan, "styles": have, "missing": sorted(r for r in used if r not in have and r != "body"), "bodySize": body_size,
            "selection": sel, "create": create_missing}


def _new_style_cmd(role: str, body: float) -> dict:
    spec = {"title": ("Title", body * 2.6, "Bold", {"spaceAfter": 10}), "h1": ("Heading 1", body * 2.0, "Bold", {"spaceBefore": 14, "spaceAfter": 6}),
            "h2": ("Heading 2", body * 1.5, "Bold", {"spaceBefore": 10, "spaceAfter": 4}), "h3": ("Heading 3", body * 1.2, "Bold", {"spaceBefore": 8, "spaceAfter": 3}),
            "caption": ("Caption", body * 0.8, "Italic", {"spaceBefore": 2}), "quote": ("Quote", body * 1.15, "Italic", {"leftIndent": 18, "rightIndent": 18, "spaceBefore": 6, "spaceAfter": 6}),
            "list": ("List", body, "Regular", {"leftIndent": 14, "spaceAfter": 2})}[role]
    return {"command": "style.paragraph.create", "params": {"name": spec[0], "para": spec[3], "chars": {"size": round(spec[1], 1), "fontStyle": spec[2]}}}


async def autostyle_apply(plan: dict) -> dict:
    have = dict(plan["styles"])
    cmds: list[dict] = []
    created = []
    for role in plan["missing"] if plan.get("create") else []:
        c = _new_style_cmd(role, plan["bodySize"])
        have[role] = c["params"]["name"]
        created.append(c["params"]["name"])
        cmds.append(c)
    applied = 0
    for s in plan["stories"]:
        for p in s["paras"]:
            name = have.get(p["role"]) or (have.get("body") if p["role"] != "body" else None)
            if p["role"] == "body" and not have.get("body"):
                name = "[Basic Paragraph]"
            if not name:
                continue
            cmds.append({"command": "text.select", "params": {"story": s["story"], "anchor": p["start"]}})
            cmds.append({"command": "style.paragraph.apply", "params": {"name": name}})
            applied += 1
    if not applied:
        return {"applied": 0, "created": [], "mode": plan["mode"]}
    sel = plan.get("selection") or {}
    if sel.get("items"):
        cmds.append({"command": "selection.set", "params": {"ids": sel["items"]}})
    await core.batch(cmds, source="assistant")
    return {"applied": applied, "created": created, "mode": plan["mode"], "note": plan.get("note", "")}


# ---------------------------------------------------------------- images

def links_dir(title: str, path: str | None) -> Path:
    if path:
        p = Path(path)
        d = p.parent / f"{p.stem} Links"
    else:
        d = config.DATA_DIR / "Links" / (re.sub(r"[^A-Za-z0-9._-]+", "-", title or "Untitled").strip("-") or "Untitled")
    d.mkdir(parents=True, exist_ok=True)
    return d


async def _doc_info() -> tuple[str, str | None]:
    from app.engine.documents import documents
    st = await core.state()
    d = st["docs"][st["active"]]
    m = documents.meta.get(d["uid"])
    return d.get("title") or "Untitled", (m.path if m and m.path else d.get("path"))


async def _frame(fid: int | None) -> dict:
    sel = (await _selection()).get("items") or []
    want = fid if fid is not None else (sel[0] if sel else None)
    if want is None:
        raise AIError("Select a graphic frame first; the picture is placed into it")
    ins = await core.inspect()
    for sp in ins.get("spreads") or []:
        for it in sp.get("items") or []:
            if it["id"] == want:
                if "graphic" not in str(it.get("kind", "")):
                    raise AIError("The selected object is not a graphic frame")
                x0, y0, x1, y1 = it["bounds"]
                return {"id": want, "w": x1 - x0, "h": y1 - y0}
    raise AIError(f"No object with id {want}")


def _openai_size(w: float, h: float) -> tuple[int, int]:
    r = w / h if h else 1.0
    return (1536, 1024) if r > 1.25 else (1024, 1536) if r < 0.8 else (1024, 1024)


def _png(im: Image.Image) -> bytes:
    buf = io.BytesIO()
    im.save(buf, "PNG")
    return buf.getvalue()


async def _fetch_image(url: str) -> bytes:
    p = urllib.parse.urlparse(url)
    if p.scheme not in ("http", "https"):
        raise AIError("the provider returned an address that is not http or https")
    try:
        async with _client() as c:
            r = await c.get(url)
    except httpx.HTTPError as exc:
        raise AIError("the generated picture could not be downloaded") from exc
    if r.status_code >= 400 or len(r.content) > 64 * 1024 * 1024:
        raise AIError("the generated picture could not be downloaded")
    return r.content


def _from_openai(j: dict) -> tuple[str, str | None]:
    d = (j.get("data") or [{}])[0]
    return d.get("b64_json") or "", d.get("url")


async def _openai_image(k: dict, prompt: str, size: tuple[int, int], edit: tuple[bytes, bytes] | None) -> bytes:
    base = k["baseUrl"]
    headers = {"authorization": f"Bearer {k['apiKey']}"} if k["apiKey"] else {}
    model = k["model"] or "gpt-image-1"
    sz = f"{size[0]}x{size[1]}"
    if edit:
        files = {"image": ("image.png", edit[0], "image/png"), "mask": ("mask.png", edit[1], "image/png")}
        r = await _post(f"{base}/images/edits", headers, data={"model": model, "prompt": prompt, "size": sz, "n": "1"}, files=files)
    else:
        r = await _post(f"{base}/images/generations", {**headers, "content-type": "application/json"}, json={"model": model, "prompt": prompt, "size": sz, "n": 1})
    b64, url = _from_openai(r.json())
    if b64:
        return base64.b64decode(b64)
    if url:
        return await _fetch_image(url)
    raise AIError("the image provider returned no picture")


async def _stability_image(k: dict, prompt: str, ratio: str, edit: tuple[bytes, dict] | None) -> bytes:
    base = k["baseUrl"]
    headers = {"authorization": f"Bearer {k['apiKey']}", "accept": "image/*"}
    if edit:
        data = {"prompt": prompt, "output_format": "png", **{d: str(v) for d, v in edit[1].items()}}
        r = await _post(f"{base}/v2beta/stable-image/edit/outpaint", headers, data=data, files={"image": ("image.png", edit[0], "image/png")})
    else:
        engine = (k["model"] or "core").strip().strip("/")
        r = await _post(f"{base}/v2beta/stable-image/generate/{engine}", headers, data={"prompt": prompt, "aspect_ratio": ratio, "output_format": "png"}, files={"none": ("", b"")})
    if r.headers.get("content-type", "").startswith("application/json"):
        j = r.json()
        if j.get("image"):
            return base64.b64decode(j["image"])
        raise AIError("the image provider returned no picture")
    return r.content


def _ratio(w: float, h: float) -> str:
    r = w / h if h else 1.0
    return min(("1:1", "16:9", "9:16", "3:2", "2:3", "4:3", "3:4", "21:9", "9:21", "5:4", "4:5"), key=lambda s: abs(int(s.split(":")[0]) / int(s.split(":")[1]) - r))


async def _store_and_place(png: bytes, frame: dict, label: str) -> dict:
    title, path = await _doc_info()
    try:
        im = Image.open(io.BytesIO(png))
        im.load()
        buf = io.BytesIO()
        im.save(buf, "PNG")
        png = buf.getvalue()
    except Exception as exc:
        raise AIError("the provider's picture could not be read") from exc
    d = links_dir(title, path)
    slug = re.sub(r"[^a-z0-9]+", "-", label.lower()).strip("-")[:40] or "image"
    f = d / f"ai-{time.strftime('%Y%m%d-%H%M%S')}-{slug}.png"
    f.write_bytes(png)
    await core.batch([{"command": "file.place", "params": {"path": str(f), "frame": frame["id"]}},
                      {"command": "object.fit", "params": {"mode": "fillProportionally", "ids": [frame["id"]]}}], source="assistant")
    return {"path": str(f), "frame": frame["id"], "pixels": list(im.size)}


async def image_generate(prompt: str, style: str = "", frame_id: int | None = None) -> dict:
    s = settings()
    if not image_configured(s):
        raise AIError("Set up an image provider in Preferences > AI first")
    prompt = (prompt or "").strip()
    if not prompt:
        raise AIError("Describe the picture to generate")
    frame = await _frame(frame_id)
    k, kind = _image_target(s)
    full = f"{prompt}. Style: {style}." if style and style != "none" else prompt
    if kind == "openai":
        png = await _openai_image(k, full, _openai_size(frame["w"], frame["h"]), None)
    else:
        png = await _stability_image(k, full, _ratio(frame["w"], frame["h"]), None)
    return await _store_and_place(png, frame, prompt)


async def image_expand(frame_id: int | None = None, prompt: str = "") -> dict:
    s = settings()
    if not image_configured(s):
        raise AIError("Set up an image provider in Preferences > AI first")
    frame = await _frame(frame_id)
    web = (await core.call("web")).result
    blk = next((b for b in web.get("blocks", []) if b.get("kind") == "image" and b.get("item") == frame["id"]), None)
    if not blk:
        raise AIError("The frame holds no picture to extend; use Generate Image to fill an empty frame")
    try:
        src = Image.open(io.BytesIO(await asset_bytes(blk["asset"])))
        src.load()
    except Exception as exc:
        raise AIError("the placed picture could not be read") from exc
    src = src.convert("RGBA")
    fa = frame["w"] / frame["h"]
    ia = src.width / src.height
    if abs(fa - ia) < 0.02:
        raise AIError("The picture already has the frame's shape; resize the frame to extend the picture")
    k, kind = _image_target(s)
    text = prompt or "Extend the picture naturally in every direction, matching its style, light and subject."
    if kind == "openai":
        cw, ch = _openai_size(frame["w"], frame["h"])
    else:
        cw, ch = (1536, int(round(1536 / fa))) if fa >= 1 else (int(round(1536 * fa)), 1536)
    scale = min(cw / src.width, ch / src.height)
    sw, sh = max(1, int(src.width * scale)), max(1, int(src.height * scale))
    ox, oy = (cw - sw) // 2, (ch - sh) // 2
    if kind == "openai":
        canvas = Image.new("RGBA", (cw, ch), (0, 0, 0, 0))
        canvas.paste(src.resize((sw, sh), Image.Resampling.LANCZOS), (ox, oy))
        mask = Image.new("RGBA", (cw, ch), (0, 0, 0, 0))
        mask.paste(Image.new("RGBA", (sw, sh), (0, 0, 0, 255)), (ox, oy))
        png = await _openai_image(k, text, (cw, ch), (_png(canvas), _png(mask)))
    else:
        im = src.resize((sw, sh), Image.Resampling.LANCZOS).convert("RGB")
        pads = {"left": ox, "right": cw - sw - ox, "up": oy, "down": ch - sh - oy}
        buf = io.BytesIO()
        im.save(buf, "PNG")
        png = await _stability_image(k, text, "", (buf.getvalue(), {a: min(2000, max(0, v)) for a, v in pads.items()}))
    out = await _store_and_place(png, frame, "expand")
    return {**out, "added": {"width": cw - sw, "height": ch - sh}}
