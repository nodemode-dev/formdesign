import base64
import difflib
import json
import re
import time
import urllib.parse
from pathlib import Path

from fastapi import APIRouter, Request, Response

from app import __version__
from app.autolayout import make as M
from app.autolayout import rank as R
from app.autolayout import slots as S
from app.core import config
from app.engine.core import CoreError, core, turn_var
from app.engine.documents import documents
from app.intake import pack as P
from app.intake.store import packs
from app.routers.agents import window_only
from app.routers.templates import ID as TEMPLATE_ID
from app.services import agents as A
from app.services import books, libraries, pdfread, profiles, queries
from app.templates import brands, registry, spec

router = APIRouter()
VERSIONS = ["2026-07-28", "2025-11-25"]
INSTRUCTIONS = ("FormDesign (the formdesign server) is a professional page-layout application, running on this computer; these tools lay out and edit the "
                "documents open in its engine, and the FormDesign window shows every change live. The fast path: intake (files and a brief become a content pack), "
                "make_layout (a template filled with that content, about 80 percent of the way there), preview to look, set_slots to fix what the report "
                "lists, export_document. run_commands reaches every layout command for anything else; find_commands looks them up by word or domain and its "
                "errors name the fix. show moves the person's view so they can follow. Every change is undoable (undo). The person may have to approve risky "
                "steps (publishing, overwriting files, deleting pages) in Agent Connect; if a call says it was not approved, ask them. Paths are absolute. "
                "For the decision tree, get the layout_strategy prompt.")
STRATEGY = """How to make a layout in FormDesign with these tools.

1. Gather. intake with the absolute paths of everything the person gave you (markdown, docx, pdf, txt, html, csv, images, gifs) and their brief in their words. It returns a compact content pack summary and the best templates for it. Do not read the files yourself first: intake is cheaper.
2. Choose. Take the top ranked template unless the brief says otherwise; list_templates with a category or kind narrows the 71 built-ins and the person's own. get_template shows a template's slots.
3. Make. make_layout with the pack id and template id. Pass fills for anything the brief states that intake missed (cta, url, date, price...). It returns a report: filled, kept (still sample text), placeholders (still sample art), emptied, fit (trimmed or shrunk copy), unused content and preflight.
4. Look. preview the first page (and any page the report mentions). Check hierarchy, crops, and that no sample text is left where real content belongs.
5. Fix. set_slots with {slot: text} or {slot: image id or path}; flow:body takes markdown for the running story. For layout changes beyond slots, run_commands with layout commands (find_commands to look them up; object ids come from get_document).
6. Deliver. save_document (a .fdes keeps everything editable), export_document as pdf, idml, html, epub, png or jpg. Tell the person what is still placeholder.
   For a website or blog, export_document web (a folder or .zip of responsive pages, with an edition for print layouts) or publish_website publish for a link to open now; fix what its warnings list (alt text, button links, placeholder art).
7. More options: make_layout with variations 2 or 3 makes alternatives in other templates; the person picks in the window.
8. New templates: write the JSON spec (docs/TEMPLATES.md), validate_template until it renders cleanly with no overset outside flows, then save_template.
9. PDFs as sources: read_pdf for structured text (headings, paragraphs, lists), pdf_page_image to see a page.
10. Long documents in parts: save each part, then book (create, add each saved .fdes, paginate, sync, export) numbers them in sequence, shares styles and writes one PDF.
11. Clean-up and restyling across a document: find_change. count first, then change_all; the built-in queries (op queries) fix straight quotes, dashes, double spaces and stray returns, and findFormat with an empty find restyles every run in a style.
"""
PROMPT_TEXT = {
    "layout_strategy": STRATEGY,
    "fix_overset": "Find and fix overset text in the active FormDesign document.\n1. outline (or look on each page) and note every OVERSET item and its story.\n2. For a threaded body flow, prefer adding continuation pages (run_commands with layout.pages.insert, then frame.thread) or Smart Text Reflow over shrinking type.\n3. For headlines and short fields, set_slots with shorter copy, or run_commands type.attrs with a smaller size on that frame's text.\n4. look again at each page you touched; repeat until outline shows no OVERSET. Report what you changed.",
    "rebrand": "Re-skin the active FormDesign document with a brand.\n1. list_brands; if the person's brand is missing, save_brand with its colours (Ink, Paper Color, Accent, Accent 2, Muted) and fonts (display, text).\n2. make_layout again with the same pack and template and brand, or change swatches and styles with run_commands (swatch.edit, style edits) to keep manual work.\n3. look at the first pages and check contrast and that no sample colour is left.",
    "export_print": "Prepare the active FormDesign document for print.\n1. preflight; fix overset text, missing links and fonts, low resolution images and RGB images where the profile asks for CMYK.\n2. export_document pdf with bleed and crop marks (standard PDF/X-1a:2001, PDF/X-3:2002 or PDF/X-4 when the printer asks for one; PDF/A-2b when they ask for an archival PDF). PDF/X-1a and X-3 refuse transparency, so the export names the pages that use it.\n3. Package with fonts and links if the printer wants the source (run_commands file.package with a folder). The person may need to approve writing over existing files.",
    "export_web": "Publish the active FormDesign document as a website.\n1. preflight and look at each page; give every image alt text (run_commands object.altText) and every button a link.\n2. export_document web to a folder or .zip, or publish_website publish for a link served from this computer (the person approves publishing in Agent Connect).\n3. Fix what the export's warnings list, then publish again.",
}
PROMPTS = [
    {"name": "layout_strategy", "title": "How to lay out in FormDesign", "description": "The decision tree for FormDesign's tools: intake, choose a template, make, look, fix, deliver.", "arguments": []},
    {"name": "fix_overset", "title": "Fix overset text", "description": "Find every overset frame and fix it with pages, threads or copy, then check.", "arguments": []},
    {"name": "rebrand", "title": "Re-skin with a brand", "description": "Apply a brand's colours and fonts to the active document.", "arguments": []},
    {"name": "export_print", "title": "Export for print", "description": "Preflight, PDF with bleed and marks, and Package.", "arguments": []},
    {"name": "export_web", "title": "Export for the web", "description": "Alt text, links, website export or Publish Online.", "arguments": []},
]


def _tool(name: str, description: str, props: dict, required: list[str] | None = None, **hints: bool) -> dict:
    title = name.replace("_", " ").capitalize()
    return {"name": name, "title": title, "description": description, "inputSchema": {"type": "object", "properties": props, "required": required or [], "additionalProperties": False},
            "annotations": {"title": title, "readOnlyHint": hints.get("read", False), "destructiveHint": hints.get("destructive", False), "idempotentHint": hints.get("idempotent", False), "openWorldHint": False}}


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
    _tool("open_document", "Opens a .fdes, .fdoc or .idml file. A .pdf, or a .pub, .qxd or .qxp (format versions 3.1 to 4.1), .pmd or .p65 file, is rebuilt as an editable layout (pages at the source size, text frames with fonts, sizes and colours, placed pictures, rectangles and lines; what cannot be converted stays as a locked backdrop layer) and a fidelity report comes back; backdrop: auto or never. A .dxf drawing (model space; ASCII or binary) becomes one page sized to its extents with lines, polylines, arcs, circles, ellipses, splines and solid hatches as shapes and text as text frames; skipped entities are counted in the report. .dwg cannot be read (no permissively licensed reader): export a DXF. .indd cannot be read: export an IDML package from the application that made it.", {"path": _PATH, "backdrop": {"type": "string", "enum": ["auto", "never"]}}, ["path"]),
    _tool("save_document", "Saves the active document (.fdes by default, keeps slots and the content pack; .fdoc or .idml by extension).", {"path": _PATH}, destructive=True),
    _tool("export_document", "Exports the active document: pdf, idml, html, epub, epub-fixed, txt, rtf, xml, png or jpg (one page: options.page from 0), a print booklet, "
          "or web: a responsive multi-page website (path is a folder, or a .zip). web options: split auto|articles|pages|single, layout article|landing|card|email|docs, "
          "edition (true adds the page-by-page edition), baseUrl (adds sitemap, feed and share links), title, description, language. Returns the pages made and warnings to fix.",
          {"format": {"type": "string", "enum": ["pdf", "idml", "html", "web", "epub", "epub-fixed", "txt", "rtf", "xml", "png", "jpg", "booklet"]}, "path": _PATH, "options": {"type": "object", "description": "Passed to the exporter (pdf: pages, bleed, marks...; web: split, layout, edition, baseUrl, title, description, language)."}}, ["format", "path"], destructive=True),
    _tool("publish_website", "Publish Online: the active document as a website that FormDesign serves on this computer at a link (127.0.0.1), kept up to date by publishing again. "
          "publish (options as export_document's web options; publishing the same document again updates its site). list: the published sites with their links. "
          "unpublish (slug). To put a site on the public web, export_document web to a folder or .zip and upload that to a static host.",
          {"action": {"type": "string", "enum": ["publish", "list", "unpublish"]}, "slug": {"type": "string"}, "options": _OBJ}, ["action"], destructive=True),
    _tool("review", "Review the active document. import_pdf_comments (path of a PDF this document exported, annotated in Acrobat or Preview): comments, highlights, strike-outs and insert-text marks become Notes at "
          "the text under them, returning what was imported and what could not be placed. share: publishes a review site (Share for Review) on this computer and returns its link; reviewers click a page to pin a comment, "
          "and each arrives as a Note at that position. comments: the shared review's comments with replies and state. reply (id, text, name) and resolve (id, resolved) answer them. stop: takes the review site down.",
          {"action": {"type": "string", "enum": ["import_pdf_comments", "share", "comments", "reply", "resolve", "stop"]}, "path": _PATH, "id": {"type": "integer", "minimum": 1},
           "text": {"type": "string", "maxLength": 2000}, "name": {"type": "string", "maxLength": 60}, "resolved": {"type": "boolean", "default": True}}, ["action"], destructive=True),
    _tool("place_word", "Places a .docx with Microsoft Word Import Options. scan (path): the Word paragraph and character styles in the file (with counts), the document's styles and the mapping used last time. "
          "place (path): styleMap {paragraph: {'Heading 1': {mode: existing|new|word, name}}, character: {...}} maps each Word style to a document style, a new style or keeps Word's (use word); "
          "options removeStyles, preserveOverrides, footnotes, quotes (booleans). Places into frame (or the selected frame), otherwise loads the Place cursor. The mapping is remembered per document.",
          {"action": {"type": "string", "enum": ["scan", "place"]}, "path": _PATH, "styleMap": _OBJ, "options": _OBJ, "frame": {"type": "integer"}}, ["action", "path"], destructive=True),
    _tool("run_commands", "Runs layout commands on the active document as one undoable batch (all or nothing): [{command, params}]. Later steps can use earlier results as $N.id. find_commands lists commands and their parameters.",
          {"commands": {"type": "array", "items": _OBJ, "minItems": 1, "maxItems": 400}}, ["commands"]),
    _tool("find_commands", "Searches the layout commands by id, label or menu, or lists a domain (type, layout, object, table, swatch, style, file...): one line each with its parameters, and how many match. With no match it suggests close ids and lists the domains.",
          {"query": {"type": "string"}, "domain": {"type": "string", "description": "Command id prefix, e.g. table or swatch."}, "limit": {"type": "integer", "minimum": 1, "maximum": 120, "default": 20}}, [], read=True, idempotent=True),
    _tool("undo", "Steps back through the active document's history.", {"steps": {"type": "integer", "minimum": 1, "maximum": 100, "default": 1}}),
    _tool("redo", "Steps forward through the active document's history.", {"steps": {"type": "integer", "minimum": 1, "maximum": 100, "default": 1}}),
    _tool("preflight", "Checks the active document for output problems: overset text, missing fonts or links, low resolution images, RGB in print. profile names a preflight profile "
          "([Basic], Commercial print, PDF/X-1a no transparency, Saddle-stitched booklet, Digital and screen, or one the person saved) that adds bleed, trim safety, minimum type size, "
          "maximum resolution, spot ink count, transparency and page count checks.", {"min_ppi": {"type": "integer"}, "profile": {"type": "string"}}, read=True, idempotent=True),
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
    _tool("book", "Books collect saved .fdes documents as chapters. list: the books and the saved documents that can be added. create (name). info, add (doc path, at), remove (index), move (index, to), source (index: the style source chapter), paginate (number pages across chapters), sync (copy the source's styles and swatches to every chapter), export (one PDF of every chapter, written to path or the Exports folder). Chapters open with unsaved changes stop paginate and sync.",
          {"action": {"type": "string", "enum": ["list", "create", "info", "add", "remove", "move", "source", "paginate", "sync", "export"]}, "book": {"type": "string", "description": "The .fdbook path (from list or create)."},
           "name": {"type": "string"}, "doc": _PATH, "at": {"type": "integer", "minimum": 0}, "index": {"type": "integer", "minimum": 0}, "to": {"type": "integer", "minimum": 0}, "path": _PATH},
          ["action"], destructive=True),
    _tool("find_change", "Find/Change across the active document. mode text (plain words; ^p return, ^t tab, ^n line break, ^9 digit, ^$ letter, ^? any character), grep "
          "(regular expressions with \\r return, \\n line break, ~= en dash, ~_ em dash, ~e ellipsis, ~{ ~} curly quotes, lookahead and lookbehind; $1 in change), glyph "
          "(glyph {char: a character or U+2014, fontFamily, fontStyle} to toGlyph) or object (object {kind: text|graphic|shape|line|group, fill, stroke, strokeWeight, opacity 0 to 1, layer, "
          "objectStyle} to objectChange). findFormat and changeFormat take paragraphStyle, characterStyle and character attributes (fontFamily, fontStyle, size, fill). op count, all (matches "
          "with page and context), change_all (one undo step; locked text is skipped), queries (the saved and built-in queries). query runs a saved query by name; save_as saves these settings. "
          "An empty change with no changeFormat deletes the matches.",
          {"op": {"type": "string", "enum": ["count", "all", "change_all", "queries"]}, "mode": {"type": "string", "enum": ["text", "grep", "glyph", "object"], "default": "text"},
           "find": {"type": "string"}, "change": {"type": "string"}, "query": {"type": "string"}, "save_as": {"type": "string"},
           "caseSensitive": {"type": "boolean"}, "wholeWord": {"type": "boolean"}, "scope": {"type": "string", "enum": ["document", "story", "selection"], "default": "document"},
           "include": {"type": "object", "description": "locked, hidden, parents: booleans (default false)"}, "findFormat": _OBJ, "changeFormat": _OBJ,
           "glyph": _OBJ, "toGlyph": _OBJ, "object": _OBJ, "objectChange": _OBJ}, ["op"], destructive=True),
    _tool("library", "Object libraries: reusable objects (a logo lockup, a footer, a pull-quote box) saved with their styles, swatches and images. list: the libraries. create (name). items (name): "
          "each item's index, name and description. add (name, ids or the selection, title, description). place (name, index, x and y in points on the spread, spread): a copy in the active "
          "document, bringing missing styles and swatches. rename (name, index, title, description). remove (name, index).",
          {"action": {"type": "string", "enum": ["list", "create", "items", "add", "place", "rename", "remove"]}, "name": {"type": "string"}, "index": {"type": "integer", "minimum": 0},
           "ids": {"type": "array", "items": {"type": "integer"}}, "title": {"type": "string"}, "description": {"type": "string"},
           "x": {"type": "number"}, "y": {"type": "number"}, "spread": {"type": "integer", "minimum": 0}}, ["action"], destructive=True),
    _tool("outline", "A compact outline of the active document: each spread with its pages and parent, and every item on it in one line (id, kind, slot, story, bounds in points, overset, the first words of its text), plus styles in use. Cheaper than get_document when you need to find things; page limits it to one page (from 1).",
          {"page": {"type": "integer", "minimum": 1}, "max_items": {"type": "integer", "minimum": 5, "maximum": 400, "default": 120}}, [], read=True, idempotent=True),
    _tool("inspect", "Properties of objects by id: kind, bounds, fill, stroke, object style, layer, story, columns, overset, locked and hidden. fields keeps only the keys you name, so replies stay short.",
          {"ids": {"type": "array", "items": {"type": "integer"}, "minItems": 1, "maxItems": 100}, "fields": {"type": "array", "items": {"type": "string"}}}, ["ids"], read=True, idempotent=True),
    _tool("look", "Looks at a page: a PNG (a region of it when region is given, in page points [x0, y0, x1, y1]) plus a short list of visible problems on that page: overset text, empty graphic frames, objects off the page.",
          {"page": {"type": "integer", "minimum": 1, "default": 1}, "region": {"type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4}, "max_size": {"type": "integer", "minimum": 128, "maximum": 2048, "default": 900}}, [], read=True, idempotent=True),
    _tool("use_tool", "Uses one of the 36 layout tools the way a person would, for what only a tool does (drawing a path with the pen, a polygon, a gradient drag): tool id (selection, type, pen, rectangleFrame, ellipse, polygon, gradientSwatch...), then a click [x, y] or a drag [x0, y0, x1, y1] in the same spread points outline and inspect use, optionally text to type afterwards. One undo step per gesture.",
          {"tool": {"type": "string"}, "spread": {"type": "integer", "minimum": 0, "default": 0}, "click": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}, "drag": {"type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4}, "text": {"type": "string"}}, ["tool"]),
    _tool("show", "Moves the person's FormDesign window so they can follow what you did: go to a page, zoom (fit page, fit spread or a percentage), open a panel, pick a tool, or select objects by id. Changes nothing in the document.",
          {"page": {"type": "integer", "minimum": 1, "description": "Page to show, counting from 1."}, "zoom": {"type": "string", "description": "fit-page, fit-spread, or a percentage such as 150."},
           "panel": {"type": "string", "description": "Panel id: pages, layers, links, properties, character, paragraph, swatches, styles, preflight, make, slots, terminal, connect, find..."},
           "tool": {"type": "string", "description": "Tool id from the engine's tool list, e.g. selection, type, rectangle-frame."},
           "select": {"type": "array", "items": {"type": "integer"}, "description": "Object ids to select (get_document lists them)."}}, [], idempotent=True),
    _tool("list_commands", "The one command catalogue the menus, the search box, the CLI and agents share: engine commands and the window's own commands, each with id, label, menu, group, what it does, when to use it, its parameters, whether it changes the document and whether Undo can reverse it. q filters by word; group filters by group (file, edit, layout, type, object, path, table, style, swatch, window...).",
          {"q": {"type": "string"}, "group": {"type": "string"}, "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 60}}, [], read=True, idempotent=True),
    _tool("run_command", "Runs one command from list_commands by id, including the window's own commands (panels, view, workspaces), with its parameters. Commands Undo cannot reverse wait for the person's approval. For several layout steps as one undo step, run_commands is cheaper.",
          {"id": {"type": "string"}, "params": {"type": "object"}}, ["id"]),
]
NAMES = {t["name"] for t in TOOLS}
_status = {"calls": 0, "last": None, "at": 0.0}


def agents_enabled() -> bool:
    return A.enabled()


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


_CMDS: list = []


async def _all_commands() -> list:
    if not _CMDS:
        r = await core.call("commands", {"enabledOnly": False}, source="mcp")
        _CMDS.extend(r.result if isinstance(r.result, list) else (r.result or {}).get("commands", []))
    return _CMDS


async def _hint(msg: str, commands: list) -> str:
    """The engine's error plus what to do about it: the step that failed, its signature, or the nearest command ids."""
    cmds = await _all_commands()
    by_id = {c["id"]: c for c in cmds}
    out = [msg]
    ids = [c.get("command") for c in commands if isinstance(c, dict)]
    unknown = [i for i in ids if i and i not in by_id]
    for i in unknown[:3]:
        near = difflib.get_close_matches(i, list(by_id), n=4, cutoff=0.45)
        out.append(f"`{i}` is not a command" + (f"; did you mean {', '.join(near)}?" if near else "; find_commands with a word from it lists real ids."))
    if not unknown:
        m = re.search(r"step (\d+)", msg)
        step = int(m.group(1)) - 1 if m else (0 if len(ids) == 1 else None)
        if step is not None and 0 <= step < len(ids) and ids[step] in by_id:
            c = by_id[ids[step]]
            out.append(f"{c['id']} takes {c.get('params') or '{}'}")
        elif len(ids) == 1 and ids[0] in by_id:
            out.append(f"{ids[0]} takes {by_id[ids[0]].get('params') or '{}'}")
    if "no document" in msg.lower():
        out.append("Open or make a document first: new_document, open_document or make_layout.")
    if re.search(r"select|selection", msg, re.I):
        out.append("Pass ids explicitly (get_document lists them) or select with show {select: [ids]}.")
    return " ".join(out)


def _b(v) -> list:
    return [round(x) for x in v] if isinstance(v, list) else v


async def _outline(a: dict) -> dict:
    ins = await core.inspect()
    labels = {s["id"]: s["slot"] for s in await S.list_slots(core, "mcp")}
    stories = {s["id"]: s for s in ins.get("stories") or []}
    want = int(a["page"]) - 1 if a.get("page") else None
    cap, used = int(a.get("max_items", 120)), 0
    lines = [f"{ins.get('title')} · {ins.get('pageCount')} pages · {len(stories)} stories · {'unsaved changes' if ins.get('dirty') else 'saved'}"]
    for sp in ins.get("spreads") or []:
        pages = sp.get("pages") or []
        if want is not None and not any(p.get("abs") == want for p in pages):
            continue
        names = ", ".join(f"{p.get('name')}{'(' + p['parent'] + ')' if p.get('parent') else ''}" for p in pages)
        lines.append(f"spread {sp.get('index')}: pages {names}")
        for it in sp.get("items") or []:
            if used >= cap:
                lines.append(f"  ... more items: raise max_items or pass page")
                break
            used += 1
            bits = [f"#{it['id']}", str(it.get("kind", "")).replace(" frame", "")]
            if it["id"] in labels:
                bits.append(f"slot:{labels[it['id']]}")
            st = stories.get(it.get("story"))
            if st:
                bits.append(f"story {st['id']}")
            bits.append(str(_b(it.get("bounds"))).replace(" ", ""))
            if it.get("overset") or (st and st.get("overset")):
                bits.append("OVERSET")
            if it.get("locked"):
                bits.append("locked")
            if st and st.get("preview") and st.get("frames", [None])[0] == it["id"]:
                bits.append(json.dumps(st["preview"].replace("\n", " / ")[:48], ensure_ascii=False))
            lines.append("  " + " ".join(bits))
    used_styles = sorted({str(s) for s in (ins.get("paragraphStyles") or [])[:30]})
    lines.append(f"paragraph styles: {', '.join(x if isinstance(x, str) else str(x) for x in used_styles)}"[:600])
    return _text("\n".join(lines))


async def _inspect(a: dict) -> dict:
    ins = await core.inspect()
    ids = {int(i) for i in a.get("ids") or []}
    fields = set(a.get("fields") or [])
    out, found = [], set()
    for sp in ins.get("spreads") or []:
        for it in sp.get("items") or []:
            if it["id"] in ids:
                found.add(it["id"])
                row = {k: (_b(v) if k == "bounds" else v) for k, v in it.items() if (not fields or k in fields or k == "id") and v not in ("", None, False, [])}
                row["spread"] = sp.get("index")
                out.append(row)
    missing = sorted(ids - found)
    res: dict = {"items": out}
    if missing:
        res["notFound"] = missing
        res["hint"] = "ids come from outline or get_document; items inside groups are listed under their group"
    return _text(res)


async def _look(a: dict) -> dict:
    page = max(1, int(a.get("page", 1)))
    size = int(a.get("max_size", 900))
    ins = await core.inspect()
    pg_info, items = None, []
    for sp in ins.get("spreads") or []:
        for p in sp.get("pages") or []:
            if p.get("abs") == page - 1:
                pg_info, items = p, sp.get("items") or []
    if pg_info is None:
        return _fail(f"no page {page}: the document has {ins.get('pageCount')} pages")
    region = a.get("region")
    r = await core.call("page.png", {"page": page - 1, "maxSide": size if not region else 2048}, source="mcp")
    png = r.bin or b""
    if region:
        from io import BytesIO
        from PIL import Image
        im = Image.open(BytesIO(png))
        pb = pg_info.get("bounds") or [0, 0, 612, 792]
        sx, sy = im.width / max(1.0, pb[2] - pb[0]), im.height / max(1.0, pb[3] - pb[1])
        x0, y0, x1, y1 = [float(v) for v in region]
        box = (max(0, int(x0 * sx)), max(0, int(y0 * sy)), min(im.width, int(x1 * sx)), min(im.height, int(y1 * sy)))
        if box[2] <= box[0] or box[3] <= box[1]:
            return _fail("region is empty or outside the page; it is [x0, y0, x1, y1] in page points")
        im = im.crop(box)
        im.thumbnail((size, size))
        buf = BytesIO()
        im.save(buf, "PNG")
        png = buf.getvalue()
    pb = pg_info.get("bounds") or [0, 0, 0, 0]
    issues = []
    stories = {s["id"]: s for s in ins.get("stories") or []}
    for it in items:
        b = it.get("bounds") or [0, 0, 0, 0]
        if b[2] < pb[0] or b[0] > pb[2]:
            continue
        st = stories.get(it.get("story"))
        if it.get("overset") or (st and st.get("overset") and st.get("frames", [None])[-1] == it["id"]):
            issues.append(f"#{it['id']} text is overset")
        if str(it.get("kind", "")).strip("<>") in ("graphic", "graphic frame") and not it.get("image") and not it.get("link"):
            issues.append(f"#{it['id']} empty graphic frame")
        covers = b[0] <= pb[0] and b[1] <= pb[1] and b[2] >= pb[2] and b[3] >= pb[3]
        if not covers and (b[0] < pb[0] - 0.5 or b[2] > pb[2] + 0.5 or b[1] < pb[1] - 0.5 or b[3] > pb[3] + 0.5):
            issues.append(f"#{it['id']} extends past the page edge")
    note = f"page {page} of {ins.get('pageCount')}" + ("; " + "; ".join(issues[:12]) if issues else "; no overset text, empty frames or objects off the page")
    return _image(png, note)


async def _use_tool(a: dict) -> dict:
    events = []
    if a.get("drag"):
        x0, y0, x1, y1 = [float(v) for v in a["drag"]]
        events = [{"kind": "down", "x": x0, "y": y0}, {"kind": "drag", "x": (x0 + x1) / 2, "y": (y0 + y1) / 2}, {"kind": "drag", "x": x1, "y": y1}, {"kind": "up", "x": x1, "y": y1}]
    elif a.get("click"):
        x, y = [float(v) for v in a["click"]]
        events = [{"kind": "down", "x": x, "y": y}, {"kind": "up", "x": x, "y": y}]
    else:
        return _fail("give click [x, y] or drag [x0, y0, x1, y1] in spread points (outline shows item bounds)")
    tr = (await core.call("tools", {}, source="mcp")).result
    tr = tr.get("tools", tr) if isinstance(tr, dict) else tr
    ids = [t["id"] for g in tr for t in (g if isinstance(g, list) else [g]) if isinstance(t, dict)]
    if a["tool"] not in ids:
        near = difflib.get_close_matches(a["tool"], ids, n=3, cutoff=0.3)
        return _fail(f"no tool `{a['tool']}`" + (f"; did you mean {', '.join(near)}?" if near else "") + f" Tools: {', '.join(ids)}")
    lay = (await core.call("layout", {}, source="mcp")).result or {}
    want = int(a.get("spread", 0))
    slot = next((s for s in lay.get("slots") or [] if (s.get("spread") or {}).get("index") == want and (s.get("spread") or {}).get("kind") == "doc"), None)
    if slot is None:
        return _fail(f"no spread {want}: outline lists the spreads")
    xa, xb, xc, xd, xe, xf = slot.get("xf") or [1, 0, 0, 1, 0, 0]
    for e in events:
        x, y = e["x"], e["y"]
        e["x"], e["y"] = xa * x + xc * y + xe, xb * x + xd * y + xf
    r = await core.call("pointer", {"events": events, "tool": a["tool"], "zoom": 1}, source="mcp")
    out = {"tool": a["tool"], "result": r.result}
    if a.get("text"):
        t = await core.call("text", {"text": a["text"]}, source="mcp")
        out["typed"] = t.result
    return _text(out)


async def _run(name: str, a: dict) -> dict:
    if name == "outline":
        return await _outline(a)
    if name == "inspect":
        return await _inspect(a)
    if name == "look":
        return await _look(a)
    if name == "use_tool":
        return await _use_tool(a)
    if name == "show":
        if a.get("select"):
            await core.exec("selection.set", {"ids": [int(i) for i in a["select"]]}, source="mcp")
        A.view(page=int(a["page"]) - 1 if a.get("page") else None, zoom=a.get("zoom"), panel=a.get("panel"), tool=a.get("tool"))
        return _text("shown in the window" if core.listeners else "no FormDesign window is open; nothing to show")
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
        from app.services import layoutimport
        if str(a["path"]).lower().endswith(tuple(layoutimport.OPEN_EXTS | layoutimport.INDD_EXTS | layoutimport.DWG_EXTS)):
            return _text(await layoutimport.open_layout(str(_abs(a["path"])), {"backdrop": a.get("backdrop", "auto")}))
        return _text(await documents.open_path(str(_abs(a["path"])), source="mcp"))
    if name == "save_document":
        return _text(await documents.save(str(_abs(a["path"])) if a.get("path") else None, source="mcp"))
    if name == "export_document":
        p = _abs(a["path"])
        p.parent.mkdir(parents=True, exist_ok=True)
        res, _, _ = await documents.export(a["format"], str(p), a.get("options") or {})
        return _text({"path": str(p), **({k: v for k, v in res.items() if k != "path"} if isinstance(res, dict) else {})})
    if name == "run_commands":
        try:
            res = await core.batch(a["commands"], source="mcp")
        except CoreError as exc:
            return _fail(await _hint(str(exc), a["commands"]))
        results = res.get("results", []) if isinstance(res, dict) else res
        return _text({"completed": len(results), "results": results[-20:]})
    if name == "list_commands":
        from app.services import commands as C
        rows = await C.catalogue(str(a.get("q") or ""))
        grp = str(a.get("group") or "").strip().lower()
        if grp:
            rows = [r for r in rows if r["group"].lower() == grp]
        lim = int(a.get("limit", 60))
        keys = ("id", "kind", "label", "menu", "group", "description", "when", "params", "mutates", "risky", "keys")
        return _text({"count": len(rows), "showing": min(lim, len(rows)), "commands": [{k: r.get(k) for k in keys if r.get(k) not in (None, "", [], {})} for r in rows[:lim]]})
    if name == "run_command":
        from app.services import commands as C
        if str(a["id"]).startswith(("ui.terminal.", "ui.claude")):
            return _fail(f"{a['id']} belongs to the person: outside agents cannot open, read or type into the terminal dock or the Claude chat")
        try:
            return _text(await C.run(str(a["id"]), a.get("params") or {}, "mcp"))
        except C.CommandError as exc:
            return _fail(str(exc))
    if name == "find_commands":
        cmds = await _all_commands()
        q, dom = str(a.get("query") or "").lower().strip(), str(a.get("domain") or "").lower().strip()
        hits = [c for c in cmds if (not dom or c["id"].lower().startswith(dom + ".")) and (not q or q in c["id"].lower() or q in str(c.get("label", "")).lower() or q in " ".join(c.get("menu") or []).lower())]
        lim = int(a.get("limit", 20))
        rows = [f"{c.get('id')} | {c.get('label')} | {c.get('params') or '{}'}"[:300] for c in hits[:lim]]
        head = f"{len(hits)} of {len(cmds)} commands match" + (f"; showing {lim}, raise limit or narrow the query for the rest" if len(hits) > lim else "")
        if not hits:
            near = difflib.get_close_matches(q, [c["id"] for c in cmds], n=5, cutoff=0.4) if q else []
            doms = sorted({c["id"].split(".")[0] for c in cmds})
            return _text(f"no commands match{'; close ids: ' + ', '.join(near) if near else ''}. Domains: {', '.join(doms)}")
        return _text("\n".join([head, "id | label | params", *rows]))
    if name in ("undo", "redo"):
        for _ in range(int(a.get("steps", 1))):
            await core.exec(f"edit.{name}", {}, source="mcp")
        return _text({"ok": True})
    if name == "preflight":
        checks: dict = {}
        if a.get("profile"):
            prof = profiles.get(a["profile"])
            if prof is None:
                return _fail(f"no preflight profile named {a['profile']}; known: " + ", ".join(p["name"] for p in profiles.listing()))
            checks.update(prof.get("checks") or {})
        if a.get("min_ppi"):
            checks["minPpi"] = int(a["min_ppi"])
        pf = (await core.call("preflight", {"profile": checks}, source="mcp")).result
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
        if not isinstance(t, dict) or not TEMPLATE_ID.match(str(t.get("id", ""))):
            return _fail("id must be kebab-case letters, digits and hyphens")
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
    if name == "book":
        return await _book(a)
    if name == "find_change":
        return await _find(a)
    if name == "library":
        return await _library(a)
    if name == "publish_website":
        return await _publish(a)
    if name == "review":
        return await _review(a)
    if name == "place_word":
        from app.services import wordimport
        if a.get("action") == "scan":
            return _text({**wordimport.scan(str(a["path"])), "document": {k: sorted(v) for k, v in (await wordimport.known_styles()).items()}, "remembered": await wordimport.recall()})
        return _text(await wordimport.place(str(a["path"]), a.get("styleMap") or {}, a.get("options") or {}, a.get("frame")))
    return _fail(f"no tool {name}")


async def _review(a: dict) -> dict:
    from app.services import pdfcomments
    from app.services.review import clean, reviews
    base = f"http://127.0.0.1:{config.PORT}"
    act = a.get("action")
    if act == "import_pdf_comments":
        return _text(await pdfcomments.import_comments(str(a.get("path") or "")))
    if act == "share":
        rec = await reviews.share()
        return _text({"url": reviews.url(base, rec), "pages": rec.get("pages"), "comments": len(rec["comments"]), "note": "loopback only: reviewers must be on this computer"})
    rec = await reviews.for_active()
    if rec is None:
        return _fail("this document is not shared for review: use action share")
    if act == "stop":
        return _text({"stopped": await reviews.stop()})
    if act == "comments":
        return _text({"shared": bool(rec.get("open")), "url": reviews.url(base, rec), "comments": [reviews.public(c) for c in rec["comments"]]})
    cid = int(a.get("id") or 0)
    if act == "reply":
        text = clean(a.get("text"), 2000)
        if not text:
            return _fail("reply needs text")
        return _text(reviews.public(await reviews.reply(rec, cid, clean(a.get("name"), 60) or "Agent", text, True)))
    if act == "resolve":
        return _text(reviews.public(await reviews.resolve(rec, cid, bool(a.get("resolved", True)))))
    return _fail(f"unknown action {act}")


async def _publish(a: dict) -> dict:
    from app.services.website import published
    base = f"http://127.0.0.1:{config.PORT}/sites/"
    act = a.get("action")
    if act == "list":
        return _text({"sites": [{"slug": s["slug"], "title": s.get("title"), "url": f"{base}{s['slug']}/", "pages": s.get("pages"), "updated": s.get("updated")} for s in published.entries()],
                      "active": await published.active_slug()})
    if act == "unpublish":
        slug = str(a.get("slug") or "")
        return _text({"unpublished": slug}) if published.unpublish(slug) else _fail(f"no published site {slug}")
    if act == "publish":
        rec = await published.publish(a.get("options") or {}, a.get("slug"))
        return _text({"url": f"{base}{rec['slug']}/", **{k: rec.get(k) for k in ("slug", "title", "pages", "split", "files", "bytes", "path", "warnings")}})
    return _fail(f"unknown action {act}: publish, list or unpublish")


async def _library(a: dict) -> dict:
    act, lib = a.get("action"), str(a.get("name") or "")
    try:
        if act == "list":
            return _text(libraries.listing())
        if act == "create":
            return _text(libraries.create(lib))
        if act == "items":
            return _text([{k: v for k, v in it.items() if k != "thumb"} for it in libraries.items(lib)])
        if act == "add":
            return _text(await libraries.add(lib, a.get("title"), str(a.get("description") or ""), a.get("ids")))
        if act == "place":
            return _text(await libraries.place(lib, int(a.get("index", -1)), a.get("x"), a.get("y"), a.get("spread")))
        if act == "rename":
            return _text(libraries.update(lib, int(a.get("index", -1)), a.get("title"), a.get("description")))
        if act == "remove":
            libraries.remove(lib, int(a.get("index", -1)))
            return _text({"ok": True})
    except libraries.LibraryError as exc:
        return _fail(str(exc))
    return _fail(f"unknown action {act}")


async def _find(a: dict) -> dict:
    op = a.get("op", "all")
    if op == "queries":
        return _text([{k: q[k] for k in ("name", "mode", "find", "change", "object", "objectChange", "builtin") if q.get(k) not in (None, "")} for q in queries.listing()])
    p: dict = {}
    if a.get("query"):
        q = queries.get(a["query"])
        if q is None:
            return _fail(f"no saved query named {a['query']}; op queries lists them")
        p.update(queries.params(q))
    p.update({k: v for k, v in a.items() if k not in ("op", "query", "save_as") and v is not None})
    p.setdefault("mode", "text")
    if a.get("save_as"):
        try:
            queries.save({**p, "name": a["save_as"]})
        except queries.QueryError as exc:
            return _fail(str(exc))
    p["op"] = "changeAll" if op == "change_all" else "all"
    if op != "change_all":
        p["limit"] = 200 if op == "all" else 20000
    res = (await core.call("findChange", p, source="mcp")).result
    if op == "count":
        return _text({"total": res.get("total"), "truncated": res.get("truncated", False)})
    if op == "all":
        keep = ("id", "kind", "name", "story", "note", "cell", "start", "end", "text", "before", "after", "locked", "overset")
        rows = []
        for m in res.get("matches", []):
            row = {k: m[k] for k in keep if m.get(k) is not None and m.get(k) != "" and m.get(k) is not False}
            if isinstance(m.get("page"), int):
                row["page"] = m["page"] + 1
            rows.append(row)
        return _text({"total": res.get("total"), "truncated": res.get("truncated", False), "pages": "count from 1", "matches": rows})
    return _text(res)


async def _book(a: dict) -> dict:
    act = a.get("action")
    if act == "list":
        return _text({"books": books.all_books(), "documents": books.project_documents()[:40]})
    if act == "create":
        return _text(await books.info(books.create(str(a.get("name") or "Book"))))
    path = str(_abs(a.get("book") or ""))
    try:
        if act == "info":
            return _text(await books.info(path))
        if act == "add":
            books.add(path, str(_abs(a.get("doc") or "")), a.get("at"))
        elif act == "remove":
            books.remove(path, int(a.get("index", -1)))
        elif act == "move":
            books.move(path, int(a.get("index", -1)), int(a.get("to", 0)))
        elif act == "source":
            books.set_source(path, int(a.get("index", -1)))
        elif act in ("paginate", "sync"):
            r = await books.run(path, [("book.paginate" if act == "paginate" else "book.syncStyles", {})], write_back=True)
            return _text({k: r[k] for k in ("chapters", "changed", "reopen")})
        elif act == "export":
            r, out = await books.export_pdf(path)
            if a.get("path"):
                dest = _abs(a["path"])
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_bytes(out.read_bytes())
                out = dest
            return _text({"path": str(out), "bytes": out.stat().st_size, "chapters": len(r["chapters"])})
        else:
            return _fail(f"unknown book action {act}")
        return _text(await books.info(path))
    except books.BookError as exc:
        return _fail(str(exc))


async def call_tool(name: str, args: dict, client: str = "Unknown client", session: str | None = None) -> dict:
    if not agents_enabled():
        return _fail("Agent access to FormDesign is turned off. Turn it on in FormDesign under Window > Agent Connect (or Preferences > Agents).")
    pol = A.policy(client)
    if pol == "deny":
        A.record(client, name, args, False, 0, "refused: this client is set to Deny in Agent Connect", core.rev)
        return _fail(f"{client} is set to Deny in FormDesign's Agent Connect panel; the person can change that there.")
    gate_args = args
    if name == "save_document":
        try:
            m = await documents.meta_for_active()
            if args.get("path") or (m is not None and not m.path):
                target = await documents.save_target(str(_abs(args["path"])) if args.get("path") else None)
                gate_args = {**args, "path": str(target)} if target is not None else args
        except (CoreError, ValueError, OSError):
            gate_args = args
    reason = A.gate_reason(name, gate_args)
    if reason and pol == "ask":
        if not core.listeners:
            A.record(client, name, args, False, 0, f"refused: {reason}, and no window is open to ask", core.rev)
            return _fail(f"No FormDesign window is open to approve this ({reason}). Open FormDesign and retry, or set {client} to Allow in Agent Connect.")
        if not await A.ask(client, name, args, reason):
            A.record(client, name, args, False, 0, f"not approved: {reason}", core.rev)
            return _fail(f"The person did not approve this ({reason}) in FormDesign's Agent Connect panel within {int(A.APPROVAL_WAIT)} seconds, or declined it. Ask them, then retry.")
    _status["calls"] += 1
    _status["last"] = name
    _status["at"] = time.time()
    t0, rev0 = time.perf_counter(), core.rev
    # Every change this call makes joins the agent's current turn, so Undo takes back a turn at once.
    token = turn_var.set(A.turn_key(session or client))
    try:
        out = await _run(name, args)
    except CoreError as exc:
        out = _fail(f"{exc}")
    except (ValueError, KeyError, TypeError, OSError) as exc:
        out = _fail(f"{exc.__class__.__name__}: {exc}")
    finally:
        turn_var.reset(token)
    first = next((c.get("text", "") for c in out.get("content", []) if c.get("type") == "text"), "image")
    A.record(client, name, args, not out.get("isError"), (time.perf_counter() - t0) * 1000, first, rev0)
    return out


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
    client, new_sid = A.identify(request.headers, params, method or "")
    version = meta.get("io.modelcontextprotocol/protocolVersion") or request.headers.get("mcp-protocol-version") or "2025-11-25"
    modern = version == "2026-07-28"
    server = {"name": "formdesign", "version": __version__}
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
        pr = next((p for p in PROMPTS if p["name"] == params.get("name")), None)
        out = _ok(req_id, {"description": pr["description"], "messages": [{"role": "user", "content": {"type": "text", "text": PROMPT_TEXT[pr["name"]]}}]}, modern) if pr else _err(req_id, -32602, f"No prompt named {params.get('name')}; prompts/list shows {', '.join(p['name'] for p in PROMPTS)}")
    elif method == "tools/list":
        out = _ok(req_id, {"tools": TOOLS}, modern)
    elif method == "tools/call":
        name, args = params.get("name"), params.get("arguments") or {}
        out = _ok(req_id, await call_tool(name, args, client, request.headers.get("mcp-session-id")), modern) if name in NAMES and isinstance(args, dict) else _err(req_id, -32602, f"No tool named {name}")
    else:
        out = _err(req_id, -32601, f"Method not found: {method}")
    if new_sid:
        A.push("clients")
    return Response(content=json.dumps(out), media_type="application/json", headers={"Mcp-Session-Id": new_sid} if new_sid else None)


@router.get("/api/mcp/status")
def status() -> dict:
    return {"endpoint": "http://127.0.0.1:8196/mcp", "tools": len(TOOLS), "enabled": agents_enabled(), **_status}


@router.post("/api/mcp/settings")
async def settings(request: Request) -> dict:
    window_only(request)
    body = json.loads(await request.body() or b"{}")
    if not isinstance(body, dict) or not isinstance(body.get("enabled"), bool):
        return {"ok": False, "error": "expected {enabled: true or false}"}
    A.set_enabled(body["enabled"])
    return {"ok": True, "enabled": body["enabled"]}
