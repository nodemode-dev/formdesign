"""Records what the FormDesign window asks the engine for while one demo opens, for the offline demo pages.

Run the engine and the window first, with a data folder of their own (see demo/README.md), then:

    python3 demo/record.py magazine demo/assets/session.js
    python3 demo/record.py event demos/event/assets/session.js
    python3 demo/record.py report demos/report/assets/session.js

Needs playwright, websockets, numpy and Pillow, and a Chrome or Chromium (CHROME, default /usr/bin/google-chrome).
"""
import argparse
import asyncio
import base64
import getpass
import io
import json
import os
import threading
import time
from pathlib import Path

import numpy as np
import websockets
from PIL import Image
from playwright.sync_api import sync_playwright

NO_REPLAY = {"pointer", "key", "text", "tool", "tiles"}
ENGINE_ONLY = ("term", "claude", "assistant/settings", "assistant/chat", "workflow/user", "core/font?")
POST_OK = ("actions/window", "commands/catalogue", "cloudfonts/missing", "demos/")
BUST = {"r", "v", "t", "rev"}
WORKSPACES = ["Advanced", "Book", "Digital Publishing", "Interactive for PDF", "Printing and Proofing", "Typography", "Editor", "Working with Claude", "Essentials"]
SCALES = (1.0, 2.0)
PAD = 48


def canon(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


class Recorder:
    def __init__(self, demo: str, ui: str):
        self.demo, self.ui = demo, ui
        self.phase = "boot"
        self.pending: dict[int, tuple] = {}
        self.ws: list[dict] = []
        self.http: list[dict] = []
        self.hello: dict | None = None
        self.last = time.time()

    def on_socket(self, sock):
        if not sock.url.endswith("/api/ws"):
            return

        def sent(f):
            self.last = time.time()
            if isinstance(f, str):
                m = json.loads(f)
                if "id" in m:
                    self.pending[m["id"]] = (m["method"], m.get("params", {}), self.phase)

        def received(f):
            self.last = time.time()
            if not isinstance(f, str):
                return
            m = json.loads(f)
            if m.get("type") == "hello" and self.hello is None:
                self.hello = m
            req = self.pending.pop(m.get("id"), None) if "id" in m else None
            if req and req[0] not in NO_REPLAY:
                self.ws.append({"phase": req[2], "method": req[0], "params": req[1], "reply": {k: v for k, v in m.items() if k != "id"}})
        sock.on("framesent", sent)
        sock.on("framereceived", received)

    def on_request(self, r):
        if "/api/" not in r.url:
            return
        self.last = time.time()
        path = r.url.split("/api/", 1)[1]
        if path.startswith(ENGINE_ONLY) or (r.method != "GET" and not path.startswith(POST_OK)):
            return
        try:
            resp = r.response()
            body = resp.body() if resp else None
        except Exception:
            return
        if body is None:
            return
        self.http.append({"phase": self.phase, "method": r.method, "path": path, "body": r.post_data or "", "status": resp.status,
                          "type": resp.headers.get("content-type", "").split(";")[0], "data": body})

    def quiet(self, pg, secs=3.0, cap=60.0):
        t0 = time.time()
        while time.time() - t0 < cap:
            pg.wait_for_timeout(250)
            if time.time() - self.last > secs:
                return

    def walk_panels(self, pg):
        for name in WORKSPACES:
            try:
                pg.select_option("select.fd-ws-select", label=name, timeout=3000)
            except Exception:
                continue
            pg.wait_for_timeout(1200)
            for t in pg.query_selector_all('[role="tab"]'):
                try:
                    if t.is_visible() and t.get_attribute("aria-selected") != "true" and not t.evaluate('n => !!n.closest(".cv-doc-tabs, .fd-dock")'):
                        t.click(timeout=1500)
                        pg.wait_for_timeout(500)
                except Exception:
                    pass
        self.quiet(pg)

    def run(self, chrome: str, engine: str):
        with sync_playwright() as p:
            b = p.chromium.launch(executable_path=chrome, args=["--no-sandbox", "--no-proxy-server"])
            pg = b.new_page(viewport={"width": 1440, "height": 900})
            pg.on("websocket", self.on_socket)
            pg.on("requestfinished", self.on_request)
            pg.goto(self.ui)
            pg.wait_for_selector("body.app-ready")
            self.quiet(pg)
            self.phase = "open"
            card = f'.cv-home-demo[data-demo="{self.demo}"]'
            pg.click(card)
            pg.wait_for_timeout(1000)
            pg.wait_for_selector(f'{card}[aria-busy="false"]', state="attached", timeout=120000)
            self.quiet(pg, 4)
            self.phase = "doc"
            pg.reload()
            pg.wait_for_selector("body.app-ready")
            self.quiet(pg, 4)
            self.walk_panels(pg)
            t = threading.Thread(target=lambda: asyncio.run(selection_tool(engine, self.ui.rstrip("/"))))
            t.start()
            t.join()
            pg.reload()
            pg.wait_for_selector("body.app-ready")
            self.quiet(pg, 4)
            b.close()


class Scrub:
    def __init__(self, data_dir: Path, engine: str):
        repo = Path(__file__).resolve().parents[1]
        home = Path.home()
        self.pairs = [(str(data_dir), "~/FormDesign"), (str(repo), "formdesign"), (str(home), "~"), (f"/Users/{getpass.getuser()}", "~"),
                      (engine.split("://", 1)[1].split("/", 1)[0], "127.0.0.1:8196")]

    def __call__(self, v):
        if isinstance(v, str):
            for a, b in self.pairs:
                v = v.replace(a, b)
            return v
        if isinstance(v, list):
            return [self(x) for x in v]
        if isinstance(v, dict):
            return {k: self(x) for k, x in v.items()}
        return v


def path_key(method: str, path: str) -> str:
    p, _, q = path.partition("?")
    kept = [kv for kv in q.split("&") if kv and kv.split("=", 1)[0] not in BUST]
    return f"{method} {p}" + ("?" + "&".join(kept) if kept else "")


def webp(data: bytes) -> bytes:
    buf = io.BytesIO()
    Image.open(io.BytesIO(data)).save(buf, "WEBP", quality=86, method=6)
    return buf.getvalue()


def http_entry(r: dict, scrub: Scrub) -> dict:
    data, t = r["data"], r["type"]
    e = {"status": r["status"], "type": t}
    if t == "application/json":
        e["json"] = scrub(json.loads(data or b"null"))
    elif t.startswith("image/"):
        if t == "image/png":
            data, e["type"] = webp(data), "image/webp"
        e["b64"] = base64.b64encode(data).decode()
    else:
        e["text"] = scrub(data.decode("utf-8", "replace"))
    return e


async def call(sock, n: int, method: str, params: dict | None = None):
    await sock.send(json.dumps({"id": n, "method": method, "params": params or {}}))
    while True:
        m = await sock.recv()
        if isinstance(m, bytes):
            k = int.from_bytes(m[:4], "big")
            head = json.loads(m[4:4 + k])
            if head.get("id") == n:
                return head, m[4 + k:]
        elif json.loads(m).get("id") == n:
            return json.loads(m), None


async def selection_tool(engine: str, origin: str) -> None:
    async with websockets.connect(engine, origin=origin) as sock:
        await sock.recv()
        await call(sock, 1, "tool", {"id": "selection"})


async def render(sock, n: int, bounds: list[float], s: float, bg: list[int]) -> np.ndarray:
    x0, y0, x1, y1 = bounds
    W, H = round((x1 - x0) * s), round((y1 - y0) * s)
    out = np.zeros((H, W, 4), np.uint8)
    for ty in range(0, H, 2048):
        for tx in range(0, W, 2048):
            w, h = min(2048, W - tx), min(2048, H - ty)
            head, data = await call(sock, n, "tiles", {"tiles": [{"key": "k", "s": s, "ox": x0 + tx / s, "oy": y0 + ty / s, "w": w, "h": h}], "bg": bg,
                                                       "preview": False, "quality": "high", "overprint": False, "notes": True, "changes": False})
            t = head["result"]["tiles"][0]
            out[ty:ty + h, tx:tx + w] = np.frombuffer(data[t["offset"]:t["offset"] + t["len"]], np.uint8).reshape(h, w, 4)
    return out


async def pictures(engine: str, origin: str):
    async with websockets.connect(engine, origin=origin, max_size=None) as sock:
        await sock.recv()
        lay = (await call(sock, 1, "layout"))[0]["result"]
        ov = (await call(sock, 2, "overlays", {"zoom": 0.64, "rect": lay["pasteboard"]}))[0]
        spreads, n = [], 3
        for slot in lay["slots"]:
            b = slot["bounds"]
            bounds = [b[0] - PAD, b[1] - PAD, b[2] + PAD, b[3] + PAD]
            scales = []
            for s in SCALES:
                dark = (await render(sock, n, bounds, s, [0, 0, 0]))[..., :3].astype(np.float32)
                light = (await render(sock, n + 1, bounds, s, [255, 255, 255]))[..., :3].astype(np.float32)
                n += 2
                alpha = np.clip(255.0 - (light - dark).mean(axis=2), 0, 255)
                rgb = np.clip(dark * 255.0 / np.maximum(alpha, 1e-3)[..., None], 0, 255)
                buf = io.BytesIO()
                Image.fromarray(np.dstack([rgb, alpha]).astype(np.uint8), "RGBA").save(buf, "WEBP", quality=82, alpha_quality=90, method=6)
                scales.append({"s": s, "w": int(alpha.shape[1]), "h": int(alpha.shape[0]), "b64": base64.b64encode(buf.getvalue()).decode()})
            spreads.append({"bounds": bounds, "scales": scales})
        return {k: v for k, v in ov.items() if k != "id"}, spreads


def bake(rec: Recorder, scrub: Scrub, overlays: dict, spreads: list) -> dict:
    tables = {"home": {"ws": {}, "http": {}}, "doc": {"ws": {}, "http": {}}}
    for r in rec.ws:
        tables["home" if r["phase"] == "boot" else "doc"]["ws"][f"{r['method']} {canon(r['params'])}"] = scrub(r["reply"])
    opened = None
    for r in rec.http:
        key, entry = path_key(r["method"], r["path"]), http_entry(r, scrub)
        if r["method"] == "POST" and r["path"].startswith("demos/"):
            opened = {"key": key, "reply": entry}
            continue
        tab = tables["home" if r["phase"] == "boot" else "doc"]["http"]
        tab[key] = entry
        if r["body"]:
            tab[f"{key} {r['body']}"] = entry
    for kind in ("ws", "http"):
        home, doc = tables["home"][kind], tables["doc"][kind]
        for k in [k for k, v in doc.items() if home.get(k) == v]:
            del doc[k]
        for t in (home, doc) if kind == "http" else ():
            for k in [k for k in t if k.count(" ") >= 2 and t.get(k.split(" ", 2)[0] + " " + k.split(" ", 2)[1]) == t[k]]:
                del t[k]
    return {"v": 1, "demo": rec.demo, "hello": rec.hello, "home": tables["home"], "doc": tables["doc"], "open": opened, "overlays": scrub(overlays), "spreads": spreads}


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("demo", choices=["magazine", "event", "report"])
    ap.add_argument("out")
    ap.add_argument("--ui", default="http://localhost:5196/")
    ap.add_argument("--engine", default="ws://127.0.0.1:8196/api/ws")
    ap.add_argument("--data", default=os.environ.get("FORMDESIGN_DATA_DIR", str(Path.home() / "FormDesign")))
    a = ap.parse_args()
    rec = Recorder(a.demo, a.ui)
    rec.run(os.environ.get("CHROME", "/usr/bin/google-chrome"), a.engine)
    scrub = Scrub(Path(a.data).expanduser().resolve(), a.engine)
    overlays, spreads = asyncio.run(pictures(a.engine, a.ui.rstrip("/")))
    text = canon(bake(rec, scrub, overlays, spreads))
    for a_, _ in scrub.pairs[:-1]:
        assert a_ not in text, "an absolute path is left in the recording"
    Path(a.out).parent.mkdir(parents=True, exist_ok=True)
    Path(a.out).write_text(f"(window.FD_SESSION = {text});\n", encoding="utf-8")
    print(a.out, len(text), "bytes")


if __name__ == "__main__":
    main()
