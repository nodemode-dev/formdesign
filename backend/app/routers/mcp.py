import base64
import json
import time
import urllib.parse
from pathlib import Path

from fastapi import APIRouter, Request, Response

from app import __version__
from app.autolayout import make as M
from app.autolayout import rank as R
from app.autolayout import slots as S
from app.engine.core import CoreError, core
from app.engine.documents import documents
from app.intake import pack as P
from app.intake.store import packs
from app.services import pdfread
from app.templates import brands, registry, spec

router = APIRouter()
VERSIONS = ["2026-07-28", "2025-11-25"]
INSTRUCTIONS = ("formdes is a page-layout app (an InDesign alternative) running on this computer; these tools lay out and edit the documents "
                "open in its engine, and the formdes window shows every change live. The fast path: intake (files and a brief become a content pack), "
                "make_layout (a template filled with that content, about 80 percent of the way there), preview to look, set_slots to fix what the report "
                "lists, export_document. run_commands reaches all 432 layout commands for anything else; find_commands looks them up. Every change is "
                "undoable (undo). Paths are absolute. For the decision tree, get the layout_strategy prompt.")
STRATEGY = """How to make a layout in formdes with these tools.

1. Gather. intake with the absolute paths of everything the person gave you (markdown, docx, pdf, txt, html, csv, images, gifs) and their brief in their words. It returns a compact content pack summary and the best templates for it. Do not read the files yourself first: intake is cheaper.
2. Choose. Take the top ranked template unless the brief says otherwise; list_templates with a category or kind narrows the 71 built-ins and the person's own. get_template shows a template's slots.
3. Make. make_layout with the pack id and template id. Pass fills for anything the brief states that intake missed (cta, url, date, price...). It returns a report: filled, kept (still sample text), placeholders (still sample art), emptied, fit (trimmed or shrunk copy), unused content and preflight.
4. Look. preview the first page (and any page the report mentions). Check hierarchy, crops, and that no sample text is left where real content belongs.
5. Fix. set_slots with {slot: text} or {slot: image id or path}; flow:body takes markdown for the running story. For layout changes beyond slots, run_commands with layout commands (find_commands to look them up; object ids come from get_document).
6. Deliver. save_document (a .fdes keeps everything editable), export_document as pdf, idml, html, epub, png or jpg. Tell the person what is still placeholder.
7. More options: make_layout with variations 2 or 3 makes alternatives in other templates; the person picks in the window.
8. New templates: write the JSON spec (docs/TEMPLATES.md), validate_template until it renders cleanly with no overset outside flows, then save_template.
9. PDFs as sources: read_pdf for structured text (headings, paragraphs, lists), pdf_page_image to see a page.
"""
PROMPTS = [{"name": "layout_strategy", "title": "How to lay out in formdes", "description": "The decision tree for formdes's tools: intake, choose a template, make, look, fix, deliver.", "arguments": []}]


def _tool(name: str, description: str, props: dict, required: list[str] | None = None, **hints: bool) -> dict:
    return {"name": name, "description": description, "inputSchema": {"type": "object", "properties": props, "required": required or [], "additionalProperties": False},
            "annotations": {"readOnlyHint": hints.get("read", False), "destructiveHint": hints.get("destructive", False), "idempotentHint": hints.get("idempotent", False), "openWorldHint": False}}


_PATH = {"type": "string", "description": "Absolute path (~ is expanded)."}
_OBJ = {"type": "object"}
TOOLS = [
    _tool("intake", "Reads files (md, docx, pdf, txt, rtf, html, csv, xlsx, images, gifs, folders) and a brief into a content pack: articles as typed blocks, images with orientation, palette and focal point, brief fields. Returns a compact summary and the top templates for it.",
          {"paths": {"type": "array", "items": {"type": "string"}, "description": "Absolute paths of files or folders."}, "brief": {"type": "string", "maxLength": 8000, "description": "What the person wants, in their words; key: value lines (headline, cta, date, venue, price, url) are read as fields."},
           "kind": {"type": "string", "description": "Force the kind (magazine, ad-social, web-landing, flyer...)."}}, []),
    _tool("list_templates", "Lists templates (built-in and the person's): id, category, size, pages, kinds. Filter by category (magazine, newsletter, book, report, print, stationery, ad, web, presentation), kind or a query.",
          {"category": {"type": "string"}, "kind": {"type": "string"}, "query": {"type": "string"}}, read=True, idempotent=True),
    _tool("get_template", "One template's card and slots (what make_layout and set_slots can fill).", {"id": {"type": "string"}}, ["id"], read=True, idempotent=True),
    _tool("make_layout", "Makes a new document from a template filled with a content pack: text into slots, the main article threaded through the body flow with continuation pages added as needed, images matched to frames by shape and role, copy fitted. Returns a short report of what is filled, kept as sample, placeholder, trimmed and unused. Without template, the best ranked one is used.",
          {"pack": {"type": "string", "description": "Pack id from intake."}, "template": {"type": "string"}, "brand": {"description": "A saved brand name, or {name, palette: {accent, accent2, ink, paper, dark, light, muted, tint}, fonts: {display: [family, style], text, accent}, logo}."},
           "fills": {"type": "object", "description": "Slot values that win over the pack: {cta: 'Book now', title: '...'}."}, "title": {"type": "string"},
           "variations": {"type": "integer", "minimum": 1, "maximum": 4, "default": 1, "description": "Also make alternatives in the next best templates."},
           "placeholders": {"type": "boolean", "default": True, "description": "Fill image frames that have no picture with on-palette sample art."}}, ["pack"]),
    _tool("preview", "The active document's page as a PNG image (pages count from 1).", {"page": {"type": "integer", "minimum": 1, "default": 1}, "max_size": {"type": "integer", "minimum": 128, "maximum": 2048, "default": 1100}}, read=True, idempotent=True),
    _tool("get_slots", "The active document's named slots (frames labelled slot:NAME and flow:NAME): kind, page, the first words of their text or the placed image.", {}, read=True, idempotent=True),
    _tool("set_slots", "Sets slots in the active document: text (a string, or a list for several paragraphs), an image (a pack image id like img2, or an absolute path), or markdown for flow:body. Reports slots that now overflow.",
          {"values": {"type": "object", "description": "{slot: value}"}}, ["values"]),
    _tool("get_document", "The active document: title, size, pages, every page's items (id, kind, slot, bounds in points, overflow), styles and swatches. detail items lists items; summary only counts.",
          {"detail": {"type": "string", "enum": ["summary", "items"], "default": "items"}}, read=True, idempotent=True),
    _tool("list_documents", "The open documents and which is active.", {}, read=True, idempotent=True),
    _tool("switch_document", "Makes an open document active, by its index from list_documents.", {"index": {"type": "integer", "minimum": 0}}, ["index"], idempotent=True),
    _tool("new_document", "Opens a new document: from a template (with its sample content, or empty slots with sample false) or blank with a size in points.",
          {"template": {"type": "string"}, "brand": {}, "sample": {"type": "boolean", "default": True}, "title": {"type": "string"},
           "width": {"type": "number"}, "height": {"type": "number"}, "pages": {"type": "integer", "minimum": 1}, "facing": {"type": "boolean"}, "preset": {"type": "string", "description": "Letter, A4, A5, Tabloid..."}}),
    _tool("open_document", "Opens a .fdes, .fdoc or .idml file.", {"path": _PATH}, ["path"]),
    _tool("save_document", "Saves the active document (.fdes by default, keeps slots and the content pack; .fdoc or .idml by extension).", {"path": _PATH}, destructive=True),
    _tool("export_document", "Exports the active document: pdf, idml, html, epub, epub-fixed, txt, rtf, xml, png or jpg (one page: options.page from 0), or a print booklet.",
          {"format": {"type": "string", "enum": ["pdf", "idml", "html", "epub", "epub-fixed", "txt", "rtf", "xml", "png", "jpg", "booklet"]}, "path": _PATH, "options": {"type": "object", "description": "Passed to the exporter (pdf: pages, bleed, marks...)."}}, ["format", "path"], destructive=True),
    _tool("run_commands", "Runs layout commands on the active document as one undoable batch (all or nothing): [{command, params}]. Later steps can use earlier results as $N.id. find_commands lists commands and their parameters.",
          {"commands": {"type": "array", "items": _OBJ, "minItems": 1, "maxItems": 400}}, ["commands"]),
    _tool("find_commands", "Searches the 432 layout commands by id, label or menu: each with its parameters.", {"query": {"type": "string"}, "limit": {"type": "integer", "minimum": 1, "maximum": 60, "default": 20}}, ["query"], read=True, idempotent=True),
    _tool("undo", "Steps back through the active document's history.", {"steps": {"type": "integer", "minimum": 1, "maximum": 100, "default": 1}}),
    _tool("redo", "Steps forward through the active document's history.", {"steps": {"type": "integer", "minimum": 1, "maximum": 100, "default": 1}}),
    _tool("preflight", "Checks the active document for output problems: overset text, missing fonts or links, low resolution images, RGB in print.", {"min_ppi": {"type": "integer", "default": 150}}, read=True, idempotent=True),
    _tool("read_pdf", "Reads a PDF: info (pages, metadata), structure (headings, paragraphs, lists as markdown-like blocks, columns joined, headers removed), text of pages, or search.",
          {"path": _PATH, "mode": {"type": "string", "enum": ["info", "structure", "text", "search"], "default": "structure"}, "pages": {"type": "array", "items": {"type": "integer"}, "description": "1-based pages for text."},
           "query": {"type": "string"}}, ["path"], read=True, idempotent=True),
    _tool("pdf_page_image", "A PDF page as a PNG image (pages count from 1).", {"path": _PATH, "page": {"type": "integer", "minimum": 1, "default": 1}, "max_size": {"type": "integer", "minimum": 128, "maximum": 2048, "default": 1100}}, ["path"], read=True, idempotent=True),
    _tool("list_brands", "The saved brand kits: palette, fonts and logo that re-skin any template.", {}, read=True, idempotent=True),
    _tool("save_brand", "Saves a brand kit: name, palette roles (#rrggbb for ink, paper, accent, accent2, muted, tint, dark, light), font roles ([family, style] for display, text, accent, mono), logo (absolute path).",
          {"name": {"type": "string"}, "palette": _OBJ, "fonts": _OBJ, "logo": {"type": "string"}}, ["name"]),
    _tool("validate_template", "Checks a template JSON spec (docs/TEMPLATES.md): structural problems, then compiles it with sample content and returns pages, slots, overflow, preflight and page images.",
          {"spec": _OBJ, "render": {"type": "boolean", "default": True}}, ["spec"], read=True),
    _tool("save_template", "Saves a template JSON spec to the person's template library after it compiles; it appears in list_templates and the gallery.", {"spec": _OBJ}, ["spec"]),
]
NAMES = {t["name"] for t in TOOLS}
_status = {"calls": 0, "last": None, "at": 0.0}


def _local_origin(origin: str | None) -> bool:
    if not origin:
        return True
    host = urllib.parse.urlparse(origin).hostname or ""
    return host in ("localhost", "127.0.0.1", "::1", "tauri.localhost") or origin.startswith("tauri://")


def _ok(req_id, result: dict, modern: bool) -> dict:
    return {"jsonrpc": "2.0", "id": req_id, "result": {**result, **({"resultType": "complete"} if modern else {})}}


def _err(req_id, code: int, message: str) -> dict:
    return {"jsonrpc": "2.0", "id": req_id, "error": {"code": code, "message": message}}


def _text(data) -> dict:
    return {"content": [{"type": "text", "text": data if isinstance(data, str) else json.dumps(data, ensure_ascii=False, separators=(",", ":"))}]}


def _image(png: bytes, note: str = "") -> dict:
    out = [{"type": "image", "data": base64.b64encode(png).decode(), "mimeType": "image/png"}]
    if note:
        out.append({"type": "text", "text": note})
    return {"content": out}


def _fail(text: str) -> dict:
    return {"content": [{"type": "text", "text": text}], "isError": True}


def _abs(raw) -> Path:
    p = Path(str(raw or "")).expanduser()
    if not p.is_absolute():
        raise ValueError(f"{raw} is not an absolute path")
    return p


def _card_line(c: dict) -> str:
    return f"{c['id']} | {c['category']} | {c['size'][0]:g}x{c['size'][1]:g}{c['unit']} | {c['pages']}p | {','.join(c['kinds'][:3])}"


async def _document(detail: str) -> dict:
    ins = await core.inspect()
    labels = {s["id"]: s["slot"] for s in await S.list_slots(core, "mcp")}
    over = {s.get("id") for s in ins.get("stories") or [] if s.get("overset")}
    out = {"title": ins.get("title"), "pages": ins.get("pageCount"), "dirty": ins.get("dirty"),
           "styles": [s if isinstance(s, str) else s.get("name") for s in ins.get("paragraphStyles") or []][:60],
           "swatches": [s if isinstance(s, str) else s.get("name") for s in ins.get("swatches") or []][:40]}
    if over:
        out["overset"] = sorted(over)
    if detail == "items":
        spreads = []
        for sp in ins.get("spreads") or []:
            items = []
            for it in sp.get("items", [])[:80]:
                kind = str(it.get("kind", "")).strip("<>").replace(" frame", "").replace("graphic ", "")
                row = {"id": it["id"], "kind": kind, "b": [round(v) for v in it.get("bounds", [])]}
                if it["id"] in labels:
                    row["slot"] = labels[it["id"]]
                if it.get("story") in over:
                    row["overset"] = True
                items.append(row)
            spreads.append({"spread": sp.get("index"), "items": items})
        out["spreads"] = spreads
    return out


async def _run(name: str, a: dict) -> dict:
    if name == "intake":
        paths = [_abs(p) for p in a.get("paths") or []]
        missing = [str(p) for p in paths if not p.exists()]
        if missing:
            return _fail(f"not found: {', '.join(missing[:5])}")
        pack = P.ingest(paths, a.get("brief") or "", {"kind": a["kind"]} if a.get("kind") else {})
        s = P.summary(pack)
        s["ranked"] = [{"id": r["id"], "score": r["score"], "why": r["why"]} for r in R.rank(pack, limit=5)]
        return _text(s)
    if name == "list_templates":
        cards = registry.cards(a.get("category"), a.get("kind"), a.get("query"))
        return _text("\n".join(["id | category | size | pages | kinds", *(_card_line(c) for c in cards), f"{len(cards)} templates"]))
    if name == "get_template":
        t = registry.get(a.get("id", ""))
        if t is None:
            return _fail(f"no template {a.get('id')}: list_templates shows them")
        c = spec.card(t)
        return _text({**c, "flowsContinue": {k: bool((v or {}).get("page")) for k, v in (t.get("flow") or {}).items()}})
    if name == "make_layout":
        pack = packs.get(a.get("pack"))
        if pack is None:
            return _fail(f"no content pack {a.get('pack')}: run intake first")
        b = brands.resolve(a.get("brand"))
        n = int(a.get("variations") or 1)
        picks = [a["template"]] if a.get("template") else []
        if n > 1 or not picks:
            picks += [r["id"] for r in R.rank(pack, limit=n + 1) if r["id"] not in picks]
        reps = []
        for tid in picks[:n]:
            rep = await M.make(core, pack, tid, brand=b, fills=a.get("fills"), options={"title": a.get("title"), "placeholders": a.get("placeholders", True)}, source="mcp")
            await documents.adopt(template={"id": tid, "name": (registry.get(tid) or {}).get("name")}, brand=b, brief=(pack.get("brief") or {}).get("text", ""),
                                  pack_id=pack["id"], slots=rep.pop("slots", {}))
            reps.append(rep)
        return _text(reps[0] if len(reps) == 1 else {"variations": reps, "active": "the last one; switch_document to change"})
    if name == "preview":
        r = await core.call("page.png", {"page": max(0, int(a.get("page", 1)) - 1), "maxSide": int(a.get("max_size", 1100))}, source="mcp")
        st = await core.state()
        return _image(r.bin or b"", f"page {a.get('page', 1)} of {(await core.inspect()).get('pageCount')}; documents open: {len(st.get('docs', []))}")
    if name == "get_slots":
        return _text(S.compact(await S.list_slots(core, "mcp")))
    if name == "set_slots":
        m = await documents.meta_for_active()
        pack = packs.get(m.pack_id) if m and m.pack_id else None
        return _text(await S.set_slots(core, a.get("values") or {}, pack, "mcp"))
    if name == "get_document":
        return _text(await _document(a.get("detail", "items")))
    if name == "list_documents":
        ov = await documents.overview()
        return _text({"active": ov.get("active"), "docs": [{"index": i, "title": d.get("title"), "template": d.get("template"), "dirty": d.get("dirty")} for i, d in enumerate(ov.get("docs", []))]})
    if name == "switch_document":
        await core.call("docs.activate", {"index": int(a["index"])}, source="mcp")
        return _text(await documents.overview())
    if name == "new_document":
        if a.get("template"):
            b = brands.resolve(a.get("brand"))
            built = await registry.build(core, a["template"], brand=b, sample=a.get("sample", True), title=a.get("title"), source="mcp")
            await documents.adopt(template={"id": a["template"], "name": built["rt"]["name"]}, brand=b)
            return _text({"template": a["template"], "pages": len(built["rt"]["pages"]), "slots": sorted({s.get("slot") for s in built["slots"] if s.get("slot")})})
        params = {k: v for k, v in (("width", a.get("width")), ("height", a.get("height")), ("pages", a.get("pages")), ("facingPages", a.get("facing")), ("preset", a.get("preset")), ("title", a.get("title"))) if v is not None}
        await documents.new(params, source="mcp")
        return _text(await _document("summary"))
    if name == "open_document":
        return _text(await documents.open_path(str(_abs(a["path"])), source="mcp"))
    if name == "save_document":
        return _text(await documents.save(str(_abs(a["path"])) if a.get("path") else None, source="mcp"))
    if name == "export_document":
        p = _abs(a["path"])
        p.parent.mkdir(parents=True, exist_ok=True)
        res, _, _ = await documents.export(a["format"], str(p), a.get("options") or {})
        return _text({"path": str(p), **({k: v for k, v in res.items() if k != "path"} if isinstance(res, dict) else {})})
    if name == "run_commands":
        res = await core.batch(a["commands"], source="mcp")
        results = res.get("results", []) if isinstance(res, dict) else res
        return _text({"completed": len(results), "results": results[-20:]})
    if name == "find_commands":
        r = await core.call("commands", {"filter": a.get("query", ""), "enabledOnly": False}, source="mcp")
        cmds = r.result if isinstance(r.result, list) else (r.result or {}).get("commands", [])
        rows = [f"{c.get('id')} | {c.get('label')} | {c.get('params') or ''}"[:300] for c in cmds[: int(a.get("limit", 20))]]
        return _text("\n".join(rows) or "no commands match")
    if name in ("undo", "redo"):
        for _ in range(int(a.get("steps", 1))):
            await core.exec(f"edit.{name}", {}, source="mcp")
        return _text({"ok": True})
    if name == "preflight":
        pf = await core.exec("preflight.run", {"minPpi": int(a.get("min_ppi", 150))}, source="mcp")
        issues = pf if isinstance(pf, list) else (pf or {}).get("issues", [])
        return _text({"issues": [{k: i.get(k) for k in ("severity", "message", "page", "item") if i.get(k) is not None} for i in issues[:40]], "total": len(issues)})
    if name == "read_pdf":
        p = str(_abs(a["path"]))
        mode = a.get("mode", "structure")
        if mode == "info":
            return _text(pdfread.info(p))
        if mode == "search":
            return _text(pdfread.search(p, a.get("query") or "")[:60])
        if mode == "text":
            pages = a.get("pages") or [1]
            return _text({str(n): pdfread.page_text(p, n - 1) for n in pages[:20]})
        st = pdfread.structure(p)
        return _text({k: v for k, v in st.items() if k != "spans"})
    if name == "pdf_page_image":
        png = pdfread.render(str(_abs(a["path"])), max(0, int(a.get("page", 1)) - 1), max_side=float(a.get("max_size", 1100)))
        return _image(png)
    if name == "list_brands":
        return _text(brands.listing())
    if name == "save_brand":
        return _text(brands.save(a))
    if name == "validate_template":
        t = a.get("spec") or {}
        probs = spec.problems(t)
        if probs:
            return _text({"ok": False, "problems": probs})
        rep = await registry.check_spec(t, render=a.get("render", True), size=900)
        renders = rep.pop("renders", [])
        content = _text(rep)["content"] + [{"type": "image", "data": r, "mimeType": "image/png"} for r in renders[:4]]
        return {"content": content}
    if name == "save_template":
        t = a.get("spec") or {}
        existing = registry.get(t.get("id", ""))
        if existing is not None and not existing.get("_user"):
            return _fail(f"{t.get('id')} is a built-in template id; choose another")
        probs = spec.problems(t)
        if probs:
            return _fail("; ".join(probs[:8]))
        rep = await registry.check_spec(t, render=False)
        if not rep.get("ok"):
            return _fail(rep.get("error", "does not compile"))
        f = registry.user_path(t)
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text(json.dumps({k: v for k, v in t.items() if not k.startswith("_")}, indent=2, ensure_ascii=False))
        registry.templates(refresh=True)
        return _text({"saved": str(f), "overset": rep.get("overset")})
    return _fail(f"no tool {name}")


async def call_tool(name: str, args: dict) -> dict:
    _status["calls"] += 1
    _status["last"] = name
    _status["at"] = time.time()
    try:
        return await _run(name, args)
    except CoreError as exc:
        return _fail(f"{exc}")
    except (ValueError, KeyError, TypeError, OSError) as exc:
        return _fail(f"{exc.__class__.__name__}: {exc}")


@router.post("/mcp")
async def mcp(request: Request) -> Response:
    if not _local_origin(request.headers.get("origin")):
        return Response(status_code=403, content=json.dumps(_err(None, -32600, "Origin not allowed")), media_type="application/json")
    try:
        msg = json.loads(await request.body())
    except ValueError:
        return Response(content=json.dumps(_err(None, -32700, "Parse error")), media_type="application/json", status_code=400)
    if isinstance(msg, list):
        return Response(content=json.dumps(_err(None, -32600, "Batches are not supported")), media_type="application/json", status_code=400)
    req_id, method, params = msg.get("id"), msg.get("method"), msg.get("params") or {}
    if req_id is None:
        return Response(status_code=202)
    meta = params.get("_meta") or {}
    version = meta.get("io.modelcontextprotocol/protocolVersion") or request.headers.get("mcp-protocol-version") or "2025-11-25"
    modern = version == "2026-07-28"
    server = {"name": "formdes", "version": __version__}
    if version not in VERSIONS and method != "initialize":
        out = _err(req_id, -32602, f"Unsupported protocol version {version}; supported: {', '.join(VERSIONS)}")
    elif method == "server/discover":
        out = _ok(req_id, {"supportedVersions": VERSIONS, "capabilities": {"tools": {}, "prompts": {}}, "_meta": {"io.modelcontextprotocol/serverInfo": server}, "instructions": INSTRUCTIONS}, True)
    elif method == "initialize":
        asked = params.get("protocolVersion")
        out = _ok(req_id, {"protocolVersion": asked if asked in VERSIONS else "2025-11-25", "capabilities": {"tools": {}, "prompts": {}}, "serverInfo": server, "instructions": INSTRUCTIONS}, False)
    elif method == "ping":
        out = _ok(req_id, {}, modern)
    elif method == "prompts/list":
        out = _ok(req_id, {"prompts": PROMPTS}, modern)
    elif method == "prompts/get":
        out = _ok(req_id, {"description": PROMPTS[0]["description"], "messages": [{"role": "user", "content": {"type": "text", "text": STRATEGY}}]}, modern) if params.get("name") == "layout_strategy" else _err(req_id, -32602, f"No prompt named {params.get('name')}")
    elif method == "tools/list":
        out = _ok(req_id, {"tools": TOOLS}, modern)
    elif method == "tools/call":
        name, args = params.get("name"), params.get("arguments") or {}
        out = _ok(req_id, await call_tool(name, args), modern) if name in NAMES and isinstance(args, dict) else _err(req_id, -32602, f"No tool named {name}")
    else:
        out = _err(req_id, -32601, f"Method not found: {method}")
    return Response(content=json.dumps(out), media_type="application/json")


@router.get("/api/mcp/status")
def status() -> dict:
    return {"endpoint": "http://127.0.0.1:8196/mcp", "tools": len(TOOLS), **_status}
