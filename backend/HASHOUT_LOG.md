# HASHOUT_LOG · backend · FormDesign

Living log for the engine service. Source files stay comment-free; what they would have said lives here, indexed. Read it before editing `backend/`.

Current build version: v0.85.3
Last updated: 2026-11-10 · one-column CSV files are read; a bad or empty zip becomes an intake warning; a wrong-shaped template spec answers problems; phone photos are measured and placed upright; a long paste reaches the terminal whole; intake and PDF reading by agents no longer freeze the engine; review comments posted at the same time all stay; a failing tool or a malformed MCP request gets a JSON-RPC answer; names without Latin letters get their own file; downloads with non-Latin names arrive; an engine on a fallback port tells everyone its real port; non-Latin names save on Windows; save_document asks before overwriting the file it will really write; an uploaded file name cannot leave the pack folder; a website export never deletes files it did not write; published sites and placed assets run sandboxed; a plug-in's fetch checks every redirect hop; document text stays inside a published page's JSON-LD block; run_commands asks for approval wherever run_command does; a template is saved only inside the template library; only the in-app token makes a client the in-app Claude Code; 2026-10-31 · .js, .mjs and .css keep their types on Windows; formdesign-core links the C runtime statically on Windows; 2026-10-30 · the Windows desktop build stops on a failed bundle step; 2026-10-26 · fixes the test suite found (FB-045 to FB-049); 2026-10-25 · LocalGuard; 2026-10-23 · full fonts for the Reserved Font Name families; the ICC profile in the frozen engine

---

## v0.85.5 · one-column CSV files are read (2026-11-10)

- refers-to: app/intake/office.py :: read_sheet
- kind: fix
- logged: v0.85.5 · 2026-11-10

`csv.Sniffer().sniff` cannot find a delimiter in a one-column file (a guest list, a price column) and raises csv.Error, so the whole table was lost and the pack warned "could not be read (Error: Could not determine delimiter)". `read_sheet` now falls back to `csv.excel` (comma) when sniffing fails, which reads one value per row. Checked: `name`, `Alice`, `Bob` as guests.csv gives the table `guests` with 3 rows and no warning.

## v0.85.5 · a bad or empty zip becomes a warning, not a failed intake (2026-11-10)

- refers-to: app/intake/pack.py :: Intake.expand
- kind: fix
- logged: v0.85.5 · 2026-11-10

`ingest` turns a file it cannot read into a warning, but `expand` runs before that loop and opened each .zip with no handling: a corrupt archive (BadZipFile), an encrypted member (RuntimeError) or a zip that holds only folders or skipped members (its folder is never made, so `iterdir` raised FileNotFoundError) made `/api/intake` and `/api/intake/paths` answer 500 and dropped every other file in the upload. The zip branch now catches `zipfile.BadZipFile`, OSError and RuntimeError into a "not a readable zip" warning and lists the extracted folder only when it exists; members already extracted before an error are still read. Checked: ingest of a corrupt zip, an empty zip, a folder-only zip, a .md and a CSV gives the article, the table and one warning.

## v0.85.5 · a wrong-shaped template spec answers problems instead of a 500 (2026-11-10)

- refers-to: app/templates/spec.py :: problems
- kind: fix
- logged: v0.85.5 · 2026-11-10

FB-048 checked the shapes of `page`, `pages`, `parents` and `flow`, but not what they hold: `"parents": {"A": {"items": []}}` (a natural mistake, parents are lists), `"flow": {"body": "x"}`, `"pages": [{"items": "abc"}]` or `[{"items": [1]}]`, and `"styles": "x"` still raised AttributeError, so `POST /api/templates/validate` and save answered 500 and the MCP tools failed outside their handling. `problems` now also reports a parent that is not a list, a flow that is not an object (or whose `page` is not one), `styles` or `styles.paragraph` that is not an object, and then any `items` list (parents, pages, flow pages) that is not a list of objects, returning before it reads inside them. Checked: each of those specs returns one shape problem; the 71 shipped templates still return none.

## v0.85.5 · phone photos are measured and placed upright (2026-11-10)

- refers-to: app/intake/images.py :: normalize
- kind: fix
- logged: v0.85.5 · 2026-11-10

Phone cameras store portrait photos as landscape pixels with EXIF Orientation 6 or 8, and neither the backend nor the core read that tag: `describe` reported landscape width, height and orientation, `assign_images` matched the photo to landscape frames, and the core placed the raw pixels, turned 90 degrees in the layout and the export. `normalize` now reads tag 0x0112; when it asks for a turn (any value but 0 or 1) it writes `ImageOps.exif_transpose` of the image to `<stem>-upright.jpg` (quality 95) for a JPEG or `<stem>-upright.png` otherwise, and returns that path, so `describe` measures and the core places the upright copy (the tag is dropped from it, so nothing turns it twice). The `-upright` name keeps the copy from landing on the source when the source is already in the pack's images folder (images saved out of a .docx). Checked: a 400x300 JPEG with Orientation 6 and a PNG with Orientation 8 come back 300x400 portrait with no tag; a plain JPEG is returned unchanged.

## v0.85.5 · a long paste reaches the terminal whole and the socket stays up (2026-11-10)

- refers-to: app/services/pty.py :: Session.out, write, _flush, drop_pending; Terminals.restart, _exit, close
- kind: fix
- logged: v0.85.5 · 2026-11-10

`Session.write` did one `os.write` on the pty master, which `_start` makes non-blocking, and ignored the count: a paste past the pty buffer (about 12 KB on Linux; a 200,000-byte write returned 11,776) lost the rest silently, and the next keystroke raised BlockingIOError, which `routers/terminal.py` does not catch, so the socket handler died and the terminal disconnected. `Terminals.reuse` writes a long prompt the same way. `write` now appends to `out` and `_flush` writes what the pty takes; on BlockingIOError it registers `loop.add_writer(fd, _flush)` and removes the writer once `out` is empty, and any other OSError (the child is gone) drops the rest. `drop_pending` clears `out` and the writer before the fd is closed in `restart`, `_exit` and `close`, so no writer outlives its fd. The Windows path (winpty) is unchanged. Checked with a pty running `stty raw -echo; sleep 1; cat > file`: a 200,000-byte write and a 10-byte write right after it raise nothing and the file ends at 200,010 bytes. Not run on Windows.

## v0.85.5 · intake and PDF reading by agents no longer freeze the engine (2026-11-10)

- refers-to: app/routers/mcp.py :: _run, intake, read_pdf and pdf_page_image branches; app/routers/make.py :: make_quick
- kind: fix
- logged: v0.85.5 · 2026-11-10

The MCP `intake` tool (`P.ingest`, `R.rank`), `read_pdf` and `pdf_page_image` (`pdfread.info`, `search`, `page_text`, `structure`, `render`) and REST `/api/make/quick` (`pack_mod.ingest`) ran synchronous work (pdfium, python-docx, the per-pixel focal point loop) directly in async handlers, so a large intake stopped every other request for seconds to minutes: the window socket, other agents and the autosave timer. They now go through `asyncio.to_thread`, as REST `/api/intake` and `/api/intake/paths` already did. `pdfread` holds its own `threading.Lock` around every pdfium use, which is what makes the worker threads safe; `R.rank` only reads the template cache. Checked with TestClient: MCP intake of a .md file, `read_pdf` in all four modes and `pdf_page_image` on a one-page PDF answer as before. Not measured: `/api/health` latency during a large intake.

## v0.85.5 · review comments posted at the same time all stay, each with its own id (2026-11-10)

- refers-to: app/services/review.py :: _locks; Reviews.post, reply, resolve, remove (and _post, _reply, _resolve, _remove)
- kind: fix
- logged: v0.85.5 · 2026-11-10

The router loads `Reviews/{token}.json` once per request and hands that copy to `post`, `reply`, `resolve` or `remove`, which change it and save it after awaiting note work in the document (several core calls). Two reviewers posting together, or a post during the owner's reply, each saved their own copy: the later save dropped the other comment, both took the same `rec["next"]` id, and the lost comment's Note stayed in the document. Each of the four now holds a per-token `asyncio.Lock` (`_locks`), reads the record fresh inside it (falling back to the caller's copy if the file is gone), and changes and saves that; the bodies moved unchanged to `_post`, `_reply`, `_resolve` and `_remove`. Checked with note placement replaced by a 50 ms sleep: two posts from stale copies run together keep both comments, ids 1 and 2; a reply and a resolve run together both land; remove still works.

## v0.85.5 · a failing tool or a malformed MCP request gets a JSON-RPC answer (2026-11-10)

- refers-to: app/routers/mcp.py :: log, call_tool, mcp
- kind: fix
- logged: v0.85.5 · 2026-11-10

`call_tool` caught only CoreError, ValueError, KeyError, TypeError and OSError, so `set_slots {"values": ["a"]}` (AttributeError in `slots.set_slots`), a template spec with wrong-shaped parents or items, or a corrupt zip in intake (BadZipFile) escaped the route: the client got a bare 500 with no `id`, which most MCP clients report as a transport failure, and `A.record` was skipped. A top-level JSON-RPC body that was a string or a number, or `params` that was not an object, failed on `msg.get` the same way. `call_tool` now ends with a catch-all that logs the traceback (`formdesign.mcp`) and returns `ExcName: message` as an isError result; `/mcp` answers 400 with -32600 "Invalid request" when the message or its `params` is not an object, and treats a `_meta` that is not an object as empty. `A.identify` (services, outside this fix) still reads `params["_meta"]` itself. Checked with TestClient: `"hi"`, `5` and `params: "x"` answer -32600; a tool whose `_run` raises AttributeError answers isError with the message.

## v0.85.5 · names without Latin letters get their own file (2026-11-10)

- refers-to: app/services/queries.py :: slug, _file; profiles.py :: slug, _file; userprefs.py :: slug, _file; libraries.py :: slug, _path
- kind: fix
- logged: v0.85.5 · 2026-11-10

Each `slug()` kept only `a-z` and `0-9`, so every name without Latin letters fell back to one file name (`query`, `profile`, `untitled`, `library`): a second Cyrillic, Greek or CJK query, profile or workspace silently replaced the first, `delete` removed whichever was there, and a second such library could not be created ("already exists") while `_load` of either opened the same file. When nothing Latin is left, `slug()` now appends the first 8 hex digits of the SHA-1 of the stripped, lower-cased name to the fallback word (`query-1a2b3c4d`); names with Latin letters or digits keep their old file names, so nothing saved before moves. A file an earlier version saved under the bare fallback name is still found for its own name: `_file` (and `_path` in libraries) returns it when the hashed file does not exist and the old file's stored name matches, so it can be saved over, deleted or opened. Checked in a scratch data folder that held an old `query.json` and `library.fdlib`: two Cyrillic queries, two CJK profiles, two Cyrillic workspaces and two Cyrillic libraries get separate files and list back, the old query is updated and deleted by its name, and the old library still opens.

## v0.85.5 · downloads with non-Latin names arrive instead of answering 500 (2026-11-10)

- refers-to: app/routers/documents.py :: attachment, export; app/routers/books.py :: export; app/routers/workflow.py :: export_assignment, return_assignment
- kind: fix
- logged: v0.85.5 · 2026-11-10

These four downloads put the file name raw into `Content-Disposition: attachment; filename="..."`, and Starlette encodes headers as Latin-1, so a document titled `東京メニュー` (or a book or assignment with such a name; the workflow names keep Unicode letters because `str.isalnum` is Unicode-aware) exported fully and then failed with UnicodeEncodeError: a 500 and no file. A `"` in a title also broke the quoting. `attachment(name)` builds the RFC 6266 form, an ASCII `filename` fallback (non-ASCII as `?`, quotes, backslashes and control characters as `_`) plus `filename*=UTF-8''` with the percent-encoded name, which browsers and WebView2 prefer. Checked: the header for `Café 東京.pdf`, a plain name, and one with a quote and a backslash builds a Response. Not run through the window.

## v0.85.5 · an engine on a fallback port tells everyone its real port (2026-11-10)

- refers-to: formdesign_engine.py :: spare, main; app/routers/system.py :: capabilities (mcp); app/routers/mcp.py :: status
- kind: fix
- logged: v0.85.5 · 2026-11-10

When 8196 was busy at desktop launch (a dev engine or another copy running), `main` passed port 0 to uvicorn, which picked a port that the Tauri shell read from the log and loaded, but `FORMDESIGN_PORT` and `config.PORT` stayed 8196. `FORMDESIGN_ENGINE_URL` in the in-app terminal, the MCP endpoint written by Agent Connect installs and the review and publish links handed to agents all pointed at the other engine. `main` now picks the spare port itself (`spare` binds port 0 and reads the number back), sets `FORMDESIGN_PORT` before `app.main` (and so `config`) is imported, and passes the same port to uvicorn; the uvicorn startup line the shell reads is unchanged. Capabilities and `/api/mcp/status` report `config.PORT` in place of a literal 8196. Between `spare` closing its socket and uvicorn binding, another process could take the port; uvicorn would then fail to start, as it would for any bind failure. Checked with the wanted port held and `uvicorn.run` stubbed: the port passed, `FORMDESIGN_PORT`, `config.PORT` and the status endpoint agree. Not run in the desktop app.

## v0.85.5 · libraries, profiles, queries, workspaces and review sites save non-Latin names on Windows (2026-11-10)

- refers-to: app/services/libraries.py :: _save; profiles.py :: save; queries.py :: save; userprefs.py :: save; review.py :: Reviews.share, the five site files
- kind: fix
- logged: v0.85.5 · 2026-11-10

These saves wrote `json.dumps(..., ensure_ascii=False)` through `write_text` with no encoding, and Python on Windows (3.12 to 3.14, no `PYTHONUTF8` anywhere in the tree) writes text in the ANSI code page, so a Greek, Cyrillic or CJK name or snippet raised UnicodeEncodeError after `write_text` had already truncated the file: a library lost every item, and profiles, queries and workspaces answered 500. The review site's `index.html` declares utf-8 but was written in the code page (mojibake for accented titles, 500 for CJK). The saves now pass `encoding="utf-8"` and drop `ensure_ascii=False`, so the JSON is plain ASCII with `\u` escapes; that saves on any code page, and files earlier versions wrote in the Windows code page still load because the reads are left as they were (an explicit utf-8 read would refuse those). A library is written to `<name>.tmp` and moved into place with `os.replace`, as `Reviews.save` already did. The five review-site files are written as UTF-8. Checked under `LC_ALL=C` with `-X utf8=0` (default encoding ASCII): a library item in Greek, a Cyrillic query, a Japanese profile and an accented workspace name all save and list back intact. Not run on Windows.

## v0.85.5 · save_document asks before overwriting the file it will really write (2026-11-10)

- refers-to: app/engine/documents.py :: Documents.save_target, Documents.save; app/routers/mcp.py :: call_tool
- kind: fix
- logged: v0.85.5 · 2026-11-10

`gate_reason` checks whether the `path` argument exists, but `Documents.save` changed any extension other than `.fdoc` and `.idml` to `.fdes`, and with no path on a never-saved document wrote `PROJECTS_DIR/<title>.fdes`; both were decided after the gate. So `save_document {"path": "/x/Report"}` (or `Report.pdf`) with `/x/Report.fdes` present, or a `make_layout` document titled like an existing project, overwrote the file without asking a client set to Ask. `save_target` now makes that decision, `save` uses it, and `call_tool` passes the target to `gate_reason` (in place of the raw path) for `save_document` when a path is given or the document has no file yet; a plain save of an already-saved document stays ungated. If the target cannot be worked out (no core, a relative path) the gate sees the original arguments and the save itself fails as before. Checked with `documents` and `core.state` stubbed: the gate sees `Projects/Report.fdes`, `/x/Report.fdes` for `/x/Report` and `/x/Report.pdf`, and nothing for a saved document.

## v0.85.5 · an uploaded file name cannot leave the pack folder (2026-11-10)

- refers-to: app/routers/intake.py :: intake_upload
- kind: fix
- logged: v0.85.5 · 2026-11-10

`intake_upload` passed each part of a multipart file name through `_safe_name`, which keeps `.` and `..`, so a name like `../../../../home/u/.bashrc` (any local process can send one; folder uploads with relative paths take the same branch) resolved outside `INTAKE_DIR/<pack>/files` and the bytes overwrote that file. Empty, `.` and `..` parts are now dropped before joining, and the resolved target must have the pack's `files` folder among its parents, else 400. A part like ` .. ` still becomes `..` after `_safe_name` strips it; the containment check is what holds there. Checked with TestClient: uploads named `../../evil.txt`, `a/../b.md`, `..` and `sub/ .. /x.md` all land inside the pack folder.

## v0.85.5 · a website export never deletes files it did not write (2026-11-10)

- refers-to: app/services/website.py :: SITE_MARKER, write_dir, _only_export_files, export
- kind: fix
- logged: v0.85.5 · 2026-11-10

`export` refused a non-empty folder only when it had no `index.html`, taking "has index.html" to mean "an earlier FormDesign export", and `write_dir` replaces the whole target (rename to `.<name>.old`, then `rmtree`), so exporting into the person's own site folder deleted its `.git`, images and other pages. `write_dir` now also writes `.formdesign-site`, the list of paths it wrote (published sites get it too, harmlessly). `export` goes ahead in a non-empty folder only when that list is there and every entry in the folder is on it, or is a folder that holds listed files; otherwise it refuses and names the choices. A folder exported by an earlier version has no list, so a re-export there asks once for an empty folder. Checked with `write_dir` into a scratch folder: the check passes after an export and a re-export, and fails with an added notes.txt, an extra file in assets, an empty `.git`, or a hand-made folder with index.html and notes.txt. A full export through the layout core was not run.

## v0.85.5 · published sites and placed assets run sandboxed, away from the engine (2026-11-10)

- refers-to: app/routers/website.py :: SANDBOX, _SITE_HEADERS, serve; app/routers/core_api.py :: asset_bytes; app/core/local_guard.py :: LocalGuard._published_site
- kind: fix
- logged: v0.85.5 · 2026-11-10

Published sites (`/sites/<slug>/`) and placed files from `/api/core/asset` were served on the engine's own origin, which LocalGuard, `window_only` and the terminal guard trust as the window, so script in a published page (document text that broke out of a page's markup, see the JSON-LD entry) or an HTML or SVG asset opened directly could POST `/api/term`, set agent policy and write files. Both now send `Content-Security-Policy: sandbox` (sites add `allow-scripts allow-popups allow-forms`; assets get bare `sandbox`), so the page has an opaque origin and its fetches carry `Origin: null`, which LocalGuard refuses. A sandboxed page's own loads (its CSS, images, fonts, links between its pages) arrive as `Sec-Fetch-Site: cross-site` with no Origin or with `Origin: null`, which FB-046 refuses, so `_published_site` lets exactly those through for GET and HEAD under `/sites/` on a loopback Host; everything else is unchanged. Site responses carry `Access-Control-Allow-Origin: *` because `@font-face` loads are CORS requests from the opaque origin; a foreign Origin still gets 403 before it could read anything. The export writes only relative URLs (`assets/site.css`, `fonts/...`, page images, `index.html`), all under `/sites/<slug>/`. Checked with TestClient: the site page and its CSS answer 200 with the header under `Origin: null` and cross-site; `/api/term` and `/api/health` with `Origin: null`, a cross-site `/api/term`, a POST to `/sites/` and a foreign Origin on `/sites/` answer 403; `tests/test_f1_guard.py` passes. Not run in a real browser.

## v0.85.5 · a plug-in's fetch checks every redirect hop against its network list (2026-11-10)

- refers-to: app/services/plugins.py :: _CheckedRedirects, _fetch, fetch
- kind: fix
- logged: v0.85.5 · 2026-11-10

`fetch` checked only the first URL against the plug-in's declared `network` hosts, and `_fetch` opened it with urllib's default opener, which follows a redirect to any http or https host; an allowed host answering `302 Location: http://127.0.0.1:8196/api/...` (or a LAN address) let a plug-in read the engine's routes, the terminal token included, or other local services. `_fetch` now takes an optional `check`; with one it opens through `_CheckedRedirects`, which calls the check on each new URL before following it, and `fetch` passes `allowed_fetch` for the plug-in, so a hop to an undeclared host or to plain http raises PluginError (the route answers 502). Catalogue and download fetches pass no check and still follow redirects (release downloads usually redirect to a CDN). Checked with a local server answering 302 to another host: the hop is refused with PluginError, and followed when the check allows it. An https fetch through an installed plug-in was not run.

## v0.85.5 · document text stays inside a published page's JSON-LD block (2026-11-10)

- refers-to: app/services/website.py :: Builder.jsonld
- kind: fix
- logged: v0.85.5 · 2026-11-10

`jsonld` wrote headline, description and author with `json.dumps`, which leaves `</script>` as it is, so an article title, dek or byline holding `</script><script>...</script>` (an imported file, a template, a package from someone else) ended the `application/ld+json` block in a published page and the rest ran as script on the engine's own origin, with `/api` in reach. It now uses `motion.safe_json`, which escapes `<`, `>` and `&` as `\u` escapes and, since its output is ASCII, the line separators U+2028 and U+2029; JSON-LD readers get the same strings back. Checked: safe_json of a headline with `</script><script>`, `&`, U+2028 and an accented letter holds no `<`, `>` or `&`, and json.loads returns the original. A full publish was not run (no layout core here).

## v0.85.5 · run_commands asks for approval wherever run_command does (2026-11-10)

- refers-to: app/services/agents.py :: gate_reason, the run_commands branch
- kind: fix
- logged: v0.85.5 · 2026-11-10

The run_commands branch checked each step only against `GATED_COMMANDS` (package and print) and the several-pages delete, never against `commands.risky` or command_meta's `risky` flag, which the run_command branch uses. A client on Ask could send `file.saveAs` or `file.exportPdf` over an existing file, `file.revert`, `file.close`, `layer.delete` or `book.export` in a batch and run it with no approval. Each step now gets the same risky check as run_command; `layout.pages.delete` keeps its own rule (approval only for more than one page), as CLAUDE.md describes. Checked: gate_reason gives a reason for file.saveAs, file.exportPdf, layer.delete, file.print and a two-page delete, and none for text.insert or a one-page delete.

## v0.85.5 · a template is saved only inside the template library (2026-11-10)

- refers-to: app/routers/mcp.py :: _run, save_template branch; app/templates/registry.py :: user_path; app/routers/templates.py :: save_template
- kind: fix
- logged: v0.85.5 · 2026-11-10

The MCP `save_template` tool never checked the template id, and `user_path` joined it raw, so `{"id": "../../../.claude", ...}` wrote the spec (extra top-level keys kept) over `~/.claude.json`, and any `*.json` the user can write was reachable. Both the tool and REST `POST /api/templates` also took `_file` and `_user` from the posted spec, which point `user_path` at any path. The tool now refuses ids that fail the REST route's `ID` rule, and `user_path` resolves its target and raises ValueError unless `TEMPLATES_USER_DIR` is among its parents (a tool error over MCP, 422 over REST). Checked: `user_path` refuses `../../x` and a spec with `_user`/`_file` set to `/tmp/x.json`; `tools/call save_template` with id `../../x` answers isError.

## v0.85.5 · only the in-app token makes a client the in-app Claude Code (2026-11-10)

- refers-to: app/services/agents.py :: identify
- kind: fix
- logged: v0.85.5 · 2026-11-10

`identify` named a client from its own `clientInfo` or User-Agent, and `policy()` defaults "Claude Code in FormDesign" to Allow, so any MCP client that sent that name without the `x-formdesign-inapp` token skipped every approval; the names "assistant" and "plugin:<id>" let a client borrow the built-in assistant's or a plug-in's policy the same way. Without the token, such a name now gets " (unverified)" added, so the client has its own policy (Ask by default). Checked: identify with those names gives "... (unverified)" and policy ask; with the token header it gives the in-app name and allow.

## v0.85.2 · formdesign-core links the C runtime statically on Windows (2026-10-31)

- refers-to: setup/install_core.bat :: the cargo build line

`setup/install_core.bat` builds `formdesign-core.exe`, the layout core sidecar, with plain `cargo build`, and Rust's MSVC target links the Visual C++ runtime (`vcruntime140.dll`) dynamically by default, so on a Windows PC without the Visual C++ Redistributable the core would not start. The script now adds `-C target-feature=+crt-static` to `RUSTFLAGS` for that build; its `setlocal` keeps the flag from reaching anything else. The Tauri shell is not affected: `tauri build` already links its runtime statically through tauri-build's `static_vcruntime`. Not run on Windows here; `dumpbin /dependents formdesign-core.exe` on a build should no longer list VCRUNTIME140.dll.

## v0.85.2 · .js, .mjs and .css keep their types on Windows (2026-10-31)

- refers-to: app/main.py :: the three mimetypes.add_type calls after the last import

Python's `mimetypes` takes file types from the Windows registry (`HKEY_CLASSES_ROOT\.js`, value `Content Type`) over its own table, and some installers and editors set `.js` to `text/plain`. On those PCs the engine served the built window's scripts as `text/plain`, WebView2 refused every module script under strict MIME checking, and the desktop window stayed blank; macOS, Linux, dev mode (Vite serves the page) and Windows PCs with the default value never showed it. The three calls pin `.js` and `.mjs` to `text/javascript` and `.css` to `text/css`, which covers `StaticFiles`, `FileResponse` and any direct `guess_type`. They sit after the last import, not above the imports: a module that re-reads the registry while importing cannot undo them, and no import follows a statement (E402). They look redundant off Windows; keep them. Repro on a Windows PC: `reg add "HKCU\Software\Classes\.js" /v "Content Type" /d "text/plain" /f`, then launch the rebuilt app; restore with `/d "text/javascript"`. Checked here: with the table poisoned to `text/plain` for js, mjs and css through `mimetypes.init(files=...)`, the three calls return `text/javascript`, `text/javascript` and `text/css`; `py_compile` passes. Not yet run on a Windows PC.

## v0.85.1 · Windows desktop build exit status (2026-10-30)

- `setup/desktop_build.bat` ended with an `echo` after `tauri.cmd build %*`, so a failed bundle step still left the script at exit 0 and formcompiler's Windows job carried on to staging with no installers. The build line now ends `|| exit /b 1`, as the frontend and freeze lines above it already did. `setup/desktop_build` (bash) already stops through `set -e`.
- The terminal on Windows (`services/pty.py`, `from winpty import PtyProcess` inside the spawn path) needs no extra flag in `setup/freeze_backend.bat`: `--collect-submodules app` makes PyInstaller trace that import, so `winpty` and its extension come in with the app modules. The Unix freeze has no counterpart because the POSIX path uses the standard library's `pty`, not ptyprocess. If a frozen Windows engine ever reports the terminal missing, `--collect-all winpty` (formlit's flag) also brings the package's data files.

## v0.84.9 · fixes the family test suite found (2026-10-26)

- FB-045, agent access changed only by the window. `/api/agents/enabled`, `/api/agents/policy`, `/api/agents/approvals/{id}`, `/api/agents/install` and `/api/mcp/settings` used the terminal router's `_guard`, which lets a request with no Origin through, so an outside agent's plain HTTP call (curl, a script it wrote) could set its own policy to allow, turn agent access on, or approve the very request it was waiting on (the approval id is in `GET /api/agents`). These routes now call `window_only` (`routers/agents.py`), which needs an Origin that `local_guard.from_window` accepts: one of `config.window_origins()` or the engine's own origin, the rule LocalGuard uses. A browser always sends Origin on a POST, so the window is unaffected; `GET /api/agents` and the prompt hook's `POST /api/agents/turn` (no Origin, it only splits undo steps) stay open. A process that forges the window's Origin, or edits `agents.json` in the data folder, is outside what an HTTP check can stop; the rule stops an agent from granting itself access by an ordinary request.
- FB-045, no window to ask. A gated call from a client set to Ask waited `APPROVAL_WAIT` (300 s) for an approval no one could see when no window was open. `call_tool` now refuses at once when `core.listeners` is empty, saying so and naming Allow in Agent Connect (FAMILY_TEST_PLAN D2).
- FB-046, cross-site requests without an Origin. A browser leaves Origin off a cross-site GET (an image, a link, a form navigation), and LocalGuard let every request with no Origin through. `_permitted` now refuses one whose `Sec-Fetch-Site` is `cross-site` (read case- and space-insensitively); same-origin, same-site, `none` and clients that send no fetch metadata (the CLI, MCP clients) pass as before.
- FB-047, the Make report's page count. The layout core adds continuation pages itself when a threaded story runs past its last frame (`native/formdesign-engine/src/reflow.rs`, up to 40 pages), so `continue_flow` found nothing overset and the report kept the template's page count: a 60-paragraph long read on `magazine-longread-swiss` reported 2 pages for a 14-page document and no `addedPages`. `make` now reads `pageCount` from `core.inspect()` at the end, reports it as `pages`, and adds the difference to `addedPages`.
- FB-048, a wrong-shaped template spec. `spec.problems` assumed `pages` was a list of objects and `parents` and `flow` were objects, so `{"pages": "many"}` raised AttributeError: `POST /api/templates/validate` answered 500 and the `validate_template` tool failed outside its error handling. It now checks the shapes of `page`, `pages`, `parents` and `flow` first and returns those problems before reading inside them.
- FB-049, sample text that overflowed its slot. With the template font library installed, `registry.check_spec` found overset outside the flows in two shipped templates: `magazine-department-news` (`feature1.body`, 39 characters past the frame) and `magazine-reviews-culture` (`list`, its third line). Make keeps sample text in slots the content does not fill, so the overflow reached made layouts too. The last sample paragraph of each is removed; frames, styles and slot names are unchanged, so the harness references do not change. All 71 shipped templates now validate with no overset outside flows.
- The suite: `backend/tests` (F1 to F5, F9, the consent controls, the sockets, every shipped template, the Make pass, PDF and IDML export) and `native/formdesign-engine/tests` (layout core and native files), run by the CI `test` job after `./formdesign install-fonts` and `install-core`. Without the font library the templates fall back to the core's bundled faces and 39 of them overflow, so the font step is part of the job.

## v0.84.7 · LocalGuard: only this computer's own window and agents reach the engine (2026-10-25)

- The family stabilization plan's Phase 3a (`FAMILY_DEV_PLAN.md` v0.4.2, formai_dev sidecar; ledger FB-032). `app/main.py` had only CORS, which decides whether a page may read a response, not whether a request runs, and `curl -H 'Host: evil.example:8196' http://127.0.0.1:8196/api/health` answered 200. A DNS-rebinding page (a hostile name that resolves to 127.0.0.1 and sends its own Host) could therefore call every route as if it were the window: the terminal routes (`/api/term`, with a per-launch token only on its websocket), the Claude launcher, document open, save and export to paths, the package and publish routes, the plugin and agent routes, the workflow check-in and check-out routes and `/mcp`.
- `app/core/local_guard.py` (new) is a pure ASGI middleware in front of every http and websocket scope, added after CORS so it is the outermost layer. It refuses a Host that is not 127.0.0.1, localhost or ::1. An Origin, when present, must be one of `config.window_origins()` (`CORS_ORIGINS`, that is `FORMDESIGN_CORS_ORIGINS` or the dev window on 5196, plus the dev window on 5196 and its preview on 5197 as `localhost` and `127.0.0.1`, `tauri://localhost`, `http://tauri.localhost`) or exactly the engine's own origin, compared on parsed scheme, host and port, never by prefix, so `http://localhost.evil.com` and `null` fail. The desktop window loads `http://127.0.0.1:<port>` (`WebviewUrl::External`) with a port picked at launch, so "own origin" is the request's own Host. Requests with no Origin (the CLI, `setup/mcp_stdio.py`, Claude Code's MCP client) pass once the Host is local. HTTP refusals answer 403 with a JSON detail; websocket refusals close before accept.
- The existing terminal websocket check (`_local` origin and the token) stays; the guard is in front of it.
- Checked against the running engine on 8196 (`FORMDESIGN_DIST` the built window; the layout core is not built in this sandbox, so `/api/health` reports `ok: false`): `Host: evil.example:8196`, `Origin: http://evil.example`, `http://localhost.evil.com` and `null` answer 403; no Origin, the engine's own origin, `http://localhost:5196` and `tauri://localhost` answer 200, and so does a `/mcp` `initialize` POST. `/api/ws` refuses a foreign Origin and a rebound Host (403) and opens for the engine's own origin, the dev origin and no Origin. `setup/mcp_stdio.py` forwards an `initialize` and gets the server's answer. The terminal websocket refuses every origin here because its token is missing, as before. The built window served by the engine in Chromium at 4x CPU and network throttling: layout shift 0 in two runs and one throttled run; the only console errors are four 422 answers from `GET /api/workflow`, which is the engine saying the layout core is not running (a missing core binary in this sandbox, not the guard); no 403 in the engine log except the deliberate probes.

## v0.84.4 · bundle audit fixes (2026-10-23)

- `setup/fonts.py` (font library build): Abril Fatface, Josefin Sans, Lora, Merriweather, Playfair Display and Raleway declare a Reserved Font Name, and the WOFF-to-TrueType conversion of their Fontsource subsets (with renamed name records for SemiBold and Black) made Modified Versions that kept the reserved names. `FULL` maps them to `assets/fonts/full/<folder>`, the unmodified google/fonts files (commit e7db6ca, with OFL.txt); `copy_full` copies them byte for byte into `backend/app/fonts` and builds their `fonts.json` styles from the fvar named instances (Lora: Regular to Bold and the italics; Merriweather: Light to Black; Raleway: Thin to Black). The six `@fontsource` packages are uninstalled, and the build now clears the old `*.ttf` first so a converted copy cannot linger. Checked: 33 families, 192 faces; the layout core resolves a named instance by style name (`formdesign-text/src/fonts.rs`, `resolve` and `instance_face`).
- `services/pdfpost.py` reads `app/assets/CGATS001Compat-v2-micro.icc` beside the frozen module, but the freeze never added `backend/app/assets`, so PDF/X output intents failed in the desktop app. Both freeze scripts add it at `app/assets`.

## Org-navi-map

```
backend/
├── requirements.txt              pinned (Linux, Windows)
├── requirements_macos.txt        unpinned twin (macOS)
├── requirements_desktop*.txt     PyInstaller for the frozen engine
├── .env.example                  every FORMDESIGN_ variable
├── oc_install(.bat)              venv and requirements
├── boot_backend(.bat)            uvicorn with reload on 127.0.0.1:8196
├── HASHOUT_LOG.md                this log
├── formdesign_engine.py          PyInstaller entry for the desktop sidecar; --selfcheck
└── app/
    ├── __init__.py               __version__
    ├── main.py                   FastAPI app, lifespan starts the core, serves the built frontend
    ├── cli.py                    ./formdesign make, intake, export, templates, check-template
    ├── fonts/                    generated by ./formdesign install-fonts (git-ignored)
    ├── core/
    │   └── config.py             ports, data folders, core binary and font folder discovery
    ├── engine/
    │   ├── core.py               the formdesign-core child process: calls, binary replies, events, restart
    │   └── documents.py          .fdes packages, open, save, adopt, recent
    ├── intake/                   files and a brief into one content pack (brief.py reads the brief)
    ├── services/                 pdfread (pypdfium2: pages, text, outline, structure, images for intake and the MCP PDF tools); books (.fdbook chapters through a temporary core book); queries (Find/Change); profiles (preflight); libraries (.fdlib files); userprefs (workspaces, shortcut sets, ICC profiles); dictionaries (Hunspell); website (web export, Publish Online)
    ├── templates/                spec, compile, registry, brands, library/ (71 JSON templates)
    ├── autolayout/               content, rank, make, fill, tagged, slots: the Make pass
    └── routers/
        ├── system.py             version, health, capabilities
        ├── core_api.py           /api/core/* REST, /api/core/font and the /api/ws window socket
        ├── documents.py, intake.py, templates.py, make.py, books.py
        ├── find.py, preflight.py /api/find/queries and /api/preflight/profiles
        ├── libraries.py          /api/libraries, its items and place
        ├── userprefs.py          /api/prefs/workspaces, /api/prefs/shortcuts, /api/prefs/profiles
        ├── dictionaries.py       /api/dictionaries: list, install, add, remove
        ├── website.py            /api/website (published sites), /api/system/open and the /sites/{slug} pages
        └── mcp.py                /mcp, the 29 tools (book, find_change, library and publish_website among them) and the layout_strategy prompt
```

---

## Table of contents

- [app/core/config.py](#config)
- [app/engine/core.py](#core)
- [app/routers/core_api.py](#core-api)
- [app/main.py](#main)
- [formdesign_engine.py](#frozen)
- [app/intake/brief.py](#brief)
- [app/autolayout/make.py](#make)
- [app/services/queries.py, profiles.py](#presets)
- [app/services/libraries.py](#libraries)
- [app/services/userprefs.py](#userprefs)
- [app/services/dictionaries.py](#dictionaries)
- [app/services/website.py](#website)

---

## app/core/config.py {#config}

### config.py · core_binary
- refers-to: backend/app/core/config.py :: core_binary
- kind: explainer
- logged: v0.1.0 · 2026-10-07

Where the layout core is looked for, first match wins: `FORMDESIGN_CORE`; beside the frozen engine (the desktop sidecars sit together); `native/target/release`; `native/bin` (the prebuilt download of `./formdesign install-core` without Rust); `native/target/debug`.

### config.py · font_dirs
- refers-to: backend/app/core/config.py :: font_dirs
- kind: explainer
- logged: v0.1.0 · 2026-10-07

The core gets `FORMDESIGN_FONT_DIRS`: the template library (`app/fonts`, or `fonts` inside the frozen bundle), the user's `~/FormDesign/Fonts`, then anything already in the variable. System fonts are scanned by the core itself unless `FORMDESIGN_SYSTEM_FONTS=0`.

---

## app/engine/core.py {#core}

### core.py · Core.call
- refers-to: backend/app/engine/core.py :: Core.call
- kind: explainer
- logged: v0.1.0 · 2026-10-07

One pipe, one request at a time under an asyncio lock (FIFO, so window and agents interleave fairly). The stream limit is 1 GiB because `inspect` of a long book is one large line. A reply with `bin` is followed by exactly that many bytes, read with `readexactly`. Every reply's `rev` and `selRev` feed `_notify`, which pushes one `changed` event to every subscriber (the window sockets), tagged with the source (`window`, `rest`, `mcp`, `engine`) so a window can tell an agent's edit from its own.

### core.py · _restart_unlocked
- refers-to: backend/app/engine/core.py :: Core._restart_unlocked
- kind: rationale
- logged: v0.1.0 · 2026-10-07

A core that died is started again on the next call and asked to reopen whatever the recovery folder holds (`~/FormDesign/.cache/recovery`), so a crash costs at most the edits since the last recovery save.

### core.py · Core.call · late replies
- refers-to: backend/app/engine/core.py :: Core.call
- kind: rationale
- logged: v0.11.0 · 2026-10-08

When the coroutine waiting for a reply is cancelled (its window closed, an HTTP client went away) after the request was written, the lock is released with the reply still in the pipe, and before 0.11.0 the next call read it as its own: every answer after that was one behind (`commands` returned the tools, `state` the commands). Replies echo the request id, so the read loop now consumes and skips any reply whose id is not the one just sent, binary payload included, and logs it. A cancellation in the middle of a binary payload would still leave bytes behind; it has not been seen, because the payload follows its line at once.

### core.py · FORMDESIGN_DICT_DIRS
- refers-to: backend/app/engine/core.py :: Core.start, Core._restart_unlocked
- kind: explainer
- logged: v0.11.0 · 2026-10-08

The core is started with `FORMDESIGN_DICT_DIRS` (the data folder's `Dictionaries/` and any folders the person names in the same variable) beside `FORMDESIGN_FONT_DIRS`. The core checks the folders when a dictionary is first wanted, so a folder created later still counts.

### documents.py · the layout inside .fdes
- refers-to: backend/app/engine/documents.py :: INNER, Documents.open_bytes, Documents.serialize_fdes, Documents.save; backend/app/services/books.py :: _inner, _write_back, run
- kind: rationale
- logged: v0.17.0 · 2026-10-10

From 0.17.0 the layout inside a `.fdes` is the core's own `.fdoc` file, stored as `document.fdoc` (`INNER`), and a bare layout is saved and opened as `.fdoc`. A `.fdes` written before 0.17.0 holds the previous engine's file, which the core can no longer read; opening one fails with a message pointing at its IDML export rather than a parse error. A zip is recognised as `.fdes` by its `formdesign.json`. Books copy each chapter's `document.fdoc` into a temporary book (`book.fdbook`) and write changed bytes back into the same entry.

---

## app/routers/core_api.py {#core-api}

### core_api.py · ws
- refers-to: backend/app/routers/core_api.py :: ws
- kind: explainer
- logged: v0.1.0 · 2026-10-07

The window's socket. Text frames are JSON requests `{id, method, params}` answered by JSON `{id, ok, result, rev, selRev, ...}`; a binary reply is one binary frame: a 4-byte big-endian header length, the JSON header, then the payload. Each request runs as its own task so a slow tile render does not hold back the next pointer event in the socket; the core lock still orders them. Change events from any source are pushed as `{type: "changed", ...}`. Only the methods in `JSON_METHODS` and `BINARY_METHODS` pass.

### core_api.py · JSON_METHODS
- refers-to: backend/app/routers/core_api.py :: JSON_METHODS
- kind: rationale
- logged: v0.8.0 · 2026-10-08

The whitelist is the window's and REST's whole reach into the core; anything else (`fonts.add`, `fonts.data`) stays engine-internal. `fonts.styles` and `fonts.face` serve the Glyphs panel, `story.paras` the Story Editor's style column, `objectStyles` the Object Styles view. The font file itself goes out through `GET /api/core/font`, which calls `fonts.data` and answers `font/ttf` with a private one-hour cache, so the binary never rides the socket.

---

## app/main.py {#main}

### main.py · spa
- refers-to: backend/app/main.py :: spa
- kind: explainer
- logged: v0.1.0 · 2026-10-07

When `frontend/dist` exists (`./formdesign serve`, the desktop app), the engine serves the built app: `/assets` statically, any other path as a file inside dist or `index.html`. Paths are resolved and checked to stay inside dist.

---

## formdesign_engine.py {#frozen}

### formdesign_engine.py · main
- refers-to: backend/formdesign_engine.py :: main
- kind: explainer
- logged: v0.6.0 · 2026-10-07

The desktop sidecar's entry. It points `FORMDESIGN_DIST` at the frontend bundled inside the PyInstaller archive, sets `FORMDESIGN_DESKTOP`, and runs uvicorn on `FORMDESIGN_PORT` (8196) when that port is free, else on port 0 so the OS picks one. Uvicorn logs the real port, and the Tauri shell reads it from that line, so a busy 8196 never blocks a launch. `multiprocessing.freeze_support()` must stay ahead of everything on Windows.

### formdesign_engine.py · selfcheck
- refers-to: backend/formdesign_engine.py :: selfcheck
- kind: explainer
- logged: v0.6.0 · 2026-10-07

Run by `setup/freeze_backend` right after freezing: it imports the libraries PyInstaller tends to miss and reports whether the frontend, the template library and the fonts made it into the archive. A `false` there means a missing `--add-data` line, not a code bug.

---

## app/intake/brief.py {#brief}

### brief.py · parse
- refers-to: backend/app/intake/brief.py :: parse
- kind: explainer
- logged: v0.6.0 · 2026-10-07

Reads a plain-words brief without a model. Kinds are ordered by where their words appear, so "a magazine ad" is an ad before a magazine. The brand comes from an explicit field, else from "for/from/by/at [the] Capitalised Words [organisation noun]", else from "Name magazine/journal/...". A brand whose last word is an event noun (festival, sale, launch) is moved to `subject` instead, because on a poster that name is the headline, not the masthead. `subject` also comes from "for a/an ... event noun" and fills the title when no article supplies one. Weekdays and sentence-initial words are rejected as brands.

### brief.py · fields_from
- refers-to: backend/app/intake/brief.py :: fields_from
- kind: explainer
- logged: v0.6.0 · 2026-10-07

Key: value lines (and inline "CTA: ..." after a sentence) become fields through `FIELD_ALIASES`; the field names are slot names, so a field fills the slot of the same name directly. Then pattern fields fill only what is still missing: email, URL, phone, price, offer, `eventdate` (weekday and calendar forms), `eventtime` (clock times and ranges), `issue` (Spring issue, Issue 12). Dashes are written as `\N{}` names so the tracked-text scan stays clean.

---

## app/autolayout/make.py {#make}

### make.py · fit_text
- refers-to: backend/app/autolayout/make.py :: fit_text
- kind: explainer
- logged: v0.6.0 · 2026-10-07

Runs only when the frame is overset. Prose (body slots and long bases) first drops whole paragraphs, or sentences when there is one paragraph, by binary search; then shrinks size and leading together to 78 percent. Short fields (titles, dates, times, prices) skip the trimming and shrink further, to 55 percent, because a cut date or headline reads as broken while a smaller one still reads. Only after shrinking fails are words trimmed with an ellipsis. The returned dict goes into the report's `fit`.

---

## app/services/queries.py, profiles.py {#presets}

### queries.py, profiles.py · storage
- refers-to: backend/app/services/queries.py, backend/app/services/profiles.py
- kind: explainer
- logged: v0.9.0 · 2026-10-08

Both follow the brand-kit pattern: built-ins live in code and cannot be overwritten or deleted; the person's own are one JSON file each (`Queries/`, `Preflight/` in the data folder) named by a slug of the name, so saving under an existing name replaces it. `clean()` keeps only the known keys, so a query or profile saved by an agent cannot smuggle other parameters into the core call. A query's settings are exactly `findChange` parameters; a profile's `checks` are exactly the core `preflight` profile.

### mcp.py · find_change, preflight
- refers-to: backend/app/routers/mcp.py :: _find, call_tool preflight
- kind: rationale
- logged: v0.9.0 · 2026-10-08

`find_change` offers count, all, change_all and queries rather than the window's Find Next and Change, because an agent works on the whole document and a selection-driven loop would cost a call per match. Listing is capped at 200 matches with context; count asks for up to 20000. Pages in the reply count from 1, as `preview` does; zero values are kept (an earlier filter dropped page 0 as falsy). `preflight` now always calls the core's `preflight` method, with the named profile's checks and `min_ppi` laid over them.

## app/services/libraries.py {#libraries}

### libraries.py · storage
- refers-to: backend/app/services/libraries.py :: folder, _path, _load, _save, listing
- kind: rationale
- logged: v0.10.0 · 2026-10-08

One `.fdlib` JSON file per library, named by a slug of its name, in `Libraries/` under the data folder or in `FORMDESIGN_LIBRARIES_DIR`, read at each call so a folder shared between people (a synced drive) shows their additions without a restart. `_path` refuses any name whose slug would land outside the folder. Each item keeps its title, description, the snippet as base64 and the thumbnail as a PNG data URL, so the panel draws a library from one GET with no further requests; a file that will not parse is skipped in the listing rather than failing it.

### libraries.py · add, place
- refers-to: backend/app/services/libraries.py :: add, thumb, place
- kind: explainer
- logged: v0.10.0 · 2026-10-08

Add selects the given ids when an agent passes them (the window passes none and uses its selection), exports a snippet through the core and asks the core for `snippetThumb` at 192 px; a failed thumbnail leaves an empty one rather than losing the item. Place hands the snippet back to `snippet.place`: with x and y it lands there on the given spread, without them where it was taken from. Both run as source `engine`, so the window hears about the change through the usual document event.

### mcp.py · library
- refers-to: backend/app/routers/mcp.py :: _library
- kind: rationale
- logged: v0.10.0 · 2026-10-08

One tool with an `action` rather than seven, because the actions share the library name and an agent reaches for them together. `items` leaves out the thumbnails: a data URL per item would spend tokens on pixels the agent cannot use, and `preview` after `place` shows the result anyway. Library errors come back as tool failures with the message, never as an exception.

### mcp.py · the agent access switch
- refers-to: backend/app/routers/mcp.py :: agents_enabled, call_tool, settings; frontend/src/features/document/prefs.ts
- kind: rationale
- logged: v0.19.0 · 2026-10-10

The switch is a file (`agents.json` in the data folder), read on every call, so it holds across restarts and covers the stdio server too, which forwards to `/mcp`. A missing or unreadable file means on, as before the switch existed. When off, `tools/list` still answers so a client can show what it would get, and every tool call fails with a message that names Preferences > Agents; the status endpoint reports `enabled` for the window.

## app/services/userprefs.py {#userprefs}

### userprefs.py · workspaces and shortcut sets
- refers-to: backend/app/services/userprefs.py :: clean, _layout, _keys, listing, save, delete; backend/app/routers/userprefs.py
- kind: rationale
- logged: v0.11.0 · 2026-10-08

Workspaces and shortcut sets are the person's, not a document's, so they live as one JSON file each in `Workspaces/` and `Shortcuts/` of the data folder, where the browser window and the desktop app both reach them (browser storage would keep them per browser). The window still keeps the live arrangement and the active set's keys in its own storage, so it starts without waiting for the engine; the files are what a saved name means. Built-in names (the seven workspaces, Default) are reserved by the router. Everything written is rebuilt from known shapes: ids match one pattern, heights and widths are numbers in range, a command keeps at most four keys, so a crafted request cannot store anything else.

### userprefs.py · ICC profiles
- refers-to: backend/app/services/userprefs.py :: add_profile, load_profiles, profile_files; backend/app/main.py :: lifespan
- kind: rationale
- logged: v0.11.0 · 2026-10-08

The core keeps loaded ICC profiles only for the life of its process, so a proof set against a printer's profile came back empty after a restart. A profile is loaded into the core first (so one the core cannot read is refused before it is kept), then copied into `Profiles/` under a slug of its name; the engine loads every file there after the core starts. A core restarted after a crash does not reload them yet.

## app/services/dictionaries.py {#dictionaries}

### dictionaries.py · install, add, remove
- refers-to: backend/app/services/dictionaries.py :: CATALOG, to_utf8, install, add_files, remove, listing
- kind: rationale
- logged: v0.11.0 · 2026-10-08

FormDesign ships no dictionary: the Hunspell dictionaries for most languages are GPL, LGPL or MPL, so they are fetched on the person's request from the LibreOffice dictionaries repository into the data folder, where their licences travel with their source (the listing links each one's folder). `CATALOG` maps the core's codes to the repository's file stems, which do not follow one pattern (`de/de_DE_frami`, `fr_FR/dictionaries/fr`). Every pair is stored as UTF-8 named by code: the `SET` line names the encoding, Python decodes it (with aliases for the Windows code pages Hunspell spells its own way), and the line is rewritten, so the core never has to know an encoding. Install, add and remove end with the core's `spellReload` and return the fresh listing, so the window redraws from one reply.

## app/services/website.py {#website}

### website.py · roles, split and the reading of a page
- refers-to: backend/app/services/website.py :: role_of, Builder.split_mode, Builder.split, Builder._article_groups
- kind: rationale
- logged: v0.12.0 · 2026-10-08

The core's `web` gives blocks in reading order with their slot and first paragraph style; everything web-specific is decided here, where agents and template authors can read it. A block's role comes from its slot (the last dotted part names it: `feature1.title` is a title, `item3.date` meta, `stat1.label` the label of `stat1`), else from its first paragraph style's name (Kicker, Deck, Byline, Caption, Pull Quote, Title). `name` is a byline, not a title, because templates use it for a person under a quote. The split follows the document: Articles panel entries first, then web templates and web-intent documents one web page per page, then two or more titles one page per article, else one page. An article group starts at each title and takes the kickers just above it; a first group on page 1 with fewer than 150 words is a cover, so it becomes the home page's hero rather than an article. A group under 120 words joins the one before it, and fewer than two groups make one page: a sweep of the 71 templates turned a two-page newsletter's news items into 22 web pages before this rule. A one-page piece under 400 words is a composition (poster, card, ad), so it is laid out by its geometry like a web template. Furniture (folios, running heads, issue lines), navigation and table-of-contents text are dropped, because the site has its own.

### website.py · XY cuts for pages, a single flow for articles
- refers-to: backend/app/services/website.py :: Builder._split, Builder._merge_strips, Builder.cut, Builder.tree_html, Builder.content_html
- kind: rationale
- logged: v0.12.0 · 2026-10-08

Web layouts (landing, card, email, docs, and pages split by page) keep the document's geometry: blocks on a page are cut recursively, columns first and strips second, into rows of columns that stack on a phone in reading order. Columns first, because a card grid only reads correctly card by card; at page level a full-width title prevents a column cut, so the strips that follow are merged back when they sit within 2.5 percent of the page width of each other and share one column structure, and a strip that repeats the first strip's kinds starts a new group (the next row of cards). Without the gap rule a hero and the section below it merged into two columns and read column by column on a phone. Articles render as one column, but take their order from the same cuts, flattened column by column, so side-by-side news items read heading then text; the body story is one long flow, figures from later pages go among its paragraphs in proportion to their page, captions pair with images by slot number or nearness, stats take their labels by slot or by the block just under them.

### website.py · type, colour, fonts and what it reports
- refers-to: backend/app/services/website.py :: Builder.style_css, Builder.site_css, Builder.fonts_css, font_info, Builder.figure_html, Builder.build
- kind: rationale
- logged: v0.12.0 · 2026-10-08

Paragraph styles become classes sized against the body style (the style with the most body text), so a 9.5 pt print body and an 18 px web body both read at 1.125rem; display sizes get a fluid clamp between 360 and 1440 px viewports. A style colour that would not show on the paper colour (white type from a dark box) falls back to ink, and the palette comes from the template's swatch names (Ink, Paper Color, Accent, Muted, Tint). Only styles that appear on a page get rules and fonts. A face is embedded only when its licence allows it: a bundled family's OFL file, or a name table that says OFL or Apache (its copyright line is written above the OFL text); anything else, which includes most fonts a person installs, falls back to a system font with a warning. Images are WebP at 1600 px with a 720 px source in `srcset`; animated GIFs and SVG pass through. The report counts what blocks publishing rather than failing: missing alt text, buttons with no link (a CTA without a hyperlink, url slot or address), Make's placeholder art (`ph-` names) and fonts left out.

### website.py · Publish Online
- refers-to: backend/app/services/website.py :: Published, SESSION, write_dir; backend/app/routers/website.py :: serve, open_target
- kind: rationale
- logged: v0.12.0 · 2026-10-08

FormDesign has no hosted service, so a published site is a folder under `Published/` served by the engine at `/sites/{slug}/` and an `index.json` record. A document owns its site by its saved path, or, while unsaved, by its uid within this engine run (`SESSION`), so publishing again updates rather than duplicates and an old record cannot claim a new document after a restart. A rebuild writes a sibling `.partial` folder and swaps it in, so a reader never sees half a site. `/sites` resolves paths inside the site's folder only and answers the site's own 404 page otherwise; `/api/system/open` opens http and https links, or folders inside the data folder, nothing else.

## app/services/pty.py and app/routers/terminal.py {#terminal0210}

### pty.py · sessions, presets and where they start
- refers-to: backend/app/services/pty.py :: PRESETS, workdir, environment, Terminals.create, Session.feed
- kind: rationale
- logged: v0.21.0 · 2026-10-12

The family standard (FormEncoder, FormWeb 0.4.0) is a live terminal that starts Claude Code, Codex and Cursor already connected. A session starts in the repo folder because that is where `.mcp.json`, `.codex/config.toml` and `.cursor/mcp.json` live, so each CLI finds the `formdesign` server without setup; a frozen desktop engine has no repo and starts in the data folder instead (agents there need `./formdesign agents install` at user scope, phase 4). The dispatcher's folder goes first on PATH so `formdesign cmd` works in any shell. POSIX reads the master fd through the event loop's reader (no thread); Windows uses pywinpty in a reader thread that hands bytes back with call_soon_threadsafe. The 256 KB ring is what a reconnecting window replays; output fans out to one queue per attached socket, so two windows can watch one session. Closing sends SIGHUP to the session's process group (start_new_session), so a CLI's children go too.

### terminal.py · who may open a shell
- refers-to: backend/app/routers/terminal.py :: _local, _guard, term_ws
- kind: rationale
- logged: v0.21.0 · 2026-10-12

A shell reachable from a browser is the largest surface FormDesign has. The engine binds loopback only; every route refuses a foreign Origin (browsers always send it on cross-site requests and WebSocket handshakes); the socket also needs a per-boot token that GET /api/term returns, which a foreign page cannot read because the engine's CORS list holds only FormDesign's own origins. The token is compared in constant time and never logged or put in a URL the engine prints.

## app/live_cli.py {#livecli0210}

### live_cli.py · the terminal's way into every command and tool
- refers-to: backend/app/live_cli.py :: cmd_run, cmd_commands, cmd_tool, add_parsers
- kind: rationale
- logged: v0.21.0 · 2026-10-12

`make`, `export` and `check-template` start their own core; `cmd`, `commands`, `tools` and `tool` deliberately do not: they call the running engine over HTTP (`/api/core/exec`, `/api/core/commands`, `/mcp`), so what a person or an agent does in the terminal lands in the document the window shows, as one undo step per command, and repaints through the same change events. `tool` uses the MCP endpoint rather than its own code path, so the terminal and agents can never disagree about what a tool does.

## app/services/agents.py and app/routers/agents.py {#connect0220}

### agents.py · who is calling
- refers-to: backend/app/services/agents.py :: identify, display, KNOWN; backend/app/routers/mcp.py :: mcp; setup/mcp_stdio.py :: forward
- kind: rationale
- logged: v0.22.0 · 2026-10-12

The MCP endpoint is stateless, and only initialize carries clientInfo, so the engine now answers initialize with an Mcp-Session-Id header (clients send it back on every later request, as Streamable HTTP asks) and maps it to the client; unknown or missing ids are not refused, so stateless clients keep working and fall back to the 2026-07-28 `_meta` clientInfo or the user agent. The stdio bridge keeps the id between lines and adds `X-FormDesign-Transport: stdio`. Names are normalised (`claude-code` becomes Claude Code) so a policy set for an agent survives its version changes. Sessions live in memory: after an engine restart a client is known again at its next initialize.

### agents.py · policies, gates and approvals
- refers-to: backend/app/services/agents.py :: policy, gate_reason, ask, decide, record; backend/app/routers/mcp.py :: call_tool
- kind: rationale
- logged: v0.22.0 · 2026-10-12

The plan's risky steps are the ones a person cannot take back with Undo: publishing, writing over a file on disk, packaging, printing, and deleting several pages. Ask (the default for a new agent) parks such a call on a future that the window's Approve or Decline resolves; the HTTP request stays open for up to 300 seconds, inside the stdio bridge's 660 second timeout, and a decline or timeout returns an error that tells the agent to ask the person. Allow all skips the gates, Deny refuses every call; the master switch still overrides all three. The activity ring (200 calls) records the revision before and after each call so the feed can say whether the document changed. Policies and the switch share agents.json; the old settings route now writes through set_enabled so it no longer drops the policies.

### routers/agents.py · the action catalogue
- refers-to: backend/app/routers/agents.py :: actions, actions_window
- kind: rationale
- logged: v0.22.0 · 2026-10-12

One list of everything FormDesign can do, built on request from the core's `commands` and `tools`, the window's own actions (posted by the window once its menus exist, kept in memory) and the MCP tool list, each with where it is reachable. Menus and Search already read the same sources; this is the list agents, the CLI and the skills read, so a new command shows up everywhere without a hand-written reference.

## app/routers/mcp.py: reading, seeing and drawing (0.23.0 and 0.24.0) {#mcp0240}

### mcp.py · errors, outline, inspect, look, use_tool, prompts
- refers-to: backend/app/routers/mcp.py :: _hint, _all_commands, _outline, _inspect, _look, _use_tool, PROMPT_TEXT
- kind: rationale
- logged: v0.24.0 · 2026-10-12

Agents spend most of their tokens finding things and recovering from errors, so the tools now answer those two needs directly. `_hint` appends what to do to an engine error (nearest ids by difflib, the failing step's signature, the next call); the command list is cached once per engine run. `outline` is the find-things view: one line per item with the slot, story, text start and OVERSET flag, so an agent rarely needs `get_document` or `preview`. `inspect` projects fields so a measurement costs tens of tokens. `look` pairs the picture with a text list of visible problems, so the agent knows what to look at; an item that covers the whole page (a background or bleed panel) is not reported as off the page. `use_tool` takes the spread points outline and inspect report and maps them to canvas points through the layout's spread transform (pages sit at different canvas offsets), and refuses unknown tool ids, which the engine would otherwise ignore silently. The four task prompts are short procedures written against these tools.

## app/services: notes, pdfcomments, review, wordimport (parity pass 4c) {#review0250}

### notes.py · a page point to a Note
- refers-to: backend/app/services/notes.py :: locate, caret_at, add_note, restore
- kind: rationale
- logged: v0.25.0 · 2026-10-12

The core has no command from a page point to a story offset, but its Type tool does exactly that on a click, so a point is mapped by clicking with the Type tool at the page's canvas origin plus the point and reading the caret from the reply; a click outside every frame leaves the selection alone, so the nearest text frame on that page is clicked at the clamped point instead. The person's tool and selection are put back each time (shifted by one when a note landed before their caret). Offsets are characters in the pointer reply and bytes in `text.select`; `_byte` converts, and a Note counts as one character (U+FFFC, three bytes).

### pdfcomments.py · Import PDF Comments
- refers-to: backend/app/services/pdfcomments.py :: read_annotations, import_comments, fingerprint
- kind: rationale
- logged: v0.25.0 · 2026-10-12

pypdf (pinned 5.9.0) reads annotations; pypdfium2 cannot. Page n of the PDF is page n of the document, scaled by the page size ratio and flipped because PDF counts up. Highlight, underline, strike-out and squiggly click the first and last quad to find the marked text and quote it from the story; if the two ends do not land in one story the text under the quad is read from the PDF instead. Replies (`/IRT`) fold into their parent's Note. A Note takes room on its line and moves the text after it, so every position is found before any note is added, and notes are inserted from the end of each story backwards. A second import skips marks already imported by a fingerprint of author, kind and comment without the quote, because the quote changes once notes exist. A Note has no date field, so the date is a line of its text.

### review.py · Share for Review
- refers-to: backend/app/services/review.py :: Reviews.share, post, same_origin, throttle; backend/app/routers/review.py :: public_post
- kind: rationale
- logged: v0.25.0 · 2026-10-12

The review site is a folder under Published/ (so `/sites/{slug}/` serves it) holding page JPEGs and plain HTML, CSS and JS under a strict CSP; it is not listed in `index.json`, so Publish Online's list does not show it. Publish Online has no LAN option, so neither does this: the engine binds loopback, the public routes refuse a Host that is not loopback and an Origin that is not the page's own, answer 30 posts a minute per review, and cap sizes and counts. The token is in the link, which is the access model. Comments live in Reviews/{token}.json; each is also a Note at the clicked spot (the document is activated for the moment if it is not the active one), and replies and resolve rewrite that Note's text. Positions use the live layout, where earlier Notes widen their lines, while the page image the reviewer clicked has none; a comment may land a few characters off in a line that already carries notes.

### wordimport.py · Word Import Options
- refers-to: backend/app/services/wordimport.py :: build, apply_jobs, place, finish
- kind: rationale
- logged: v0.25.0 · 2026-10-12

The core reads a .docx's paragraph styles by `w:name`, bold and italic, and file.place takes no map. So the .docx is rewritten (style names in styles.xml changed to the target names, so existing and new styles map by name; Normal made explicit when mapped; direct formatting, tab stops and styles stripped as asked; quotes curled), saved under .cache/word-import and placed. Character styles and footnotes are applied afterwards by `style.character.apply` and `footnote.insert` in one batch, at offsets computed from the rewritten file and applied only if the placed story's text equals it. With no frame the text loads into the Place cursor; the jobs wait in memory and `/place/word/finish`, called by the window after each change, applies them when a story with that text appears. The last mapping and options are kept per document (path, or session and uid while unsaved) in word-import.json. .rtf is not offered: the core reads RTF as plain text, with no styles to map.


### claude_code.py, claude_chat.py, routers/claude.py · Claude Code inside the window
- refers-to: backend/app/services/claude_code.py :: cli, info, child_env, mcp_config, workdir, argv; backend/app/services/claude_chat.py :: send, _run, publish, sessions, adopt; backend/app/routers/claude.py
- kind: rationale
- logged: v0.26.0 · 2026-10-16

Ported from formlit 0.45.3 (same owner, Apache-2.0). The person's own Claude Code sign-in does the work: each chat turn runs `claude -p TEXT --output-format stream-json --verbose --include-partial-messages --mcp-config JSON --allowedTools mcp__formdesign --append-system-prompt NOTE [--resume ID]` through a login shell (GUI-launched apps lack the profile's PATH; Windows passes argv as is), in the agent folder (preference, `FORMDESIGN_AGENT_DIR`, or the terminal's folder), with every `CLAUDE*` variable removed except the four that carry sign-in and provider choice, so a FormDesign started from inside a Claude Code session does not nest into it. The MCP config is HTTP to this engine with the `x-formdesign-inapp` header carrying a per-launch secret; `services/agents.py` names that client "Claude Code in FormDesign" and its policy defaults to Allow, since the person started it here, while Deny and the agent switch still apply. Only `mcp__formdesign` is pre-allowed, so the headless agent cannot run shell commands or edit files. Stream events are parsed line by line (subagent events skipped) and pushed to every window over `/api/ws` as `{type: claude, kind: start|delta|tool|toolDone|usage|done|reset|adopted}`; the parser runs in a thread, so delivery goes through `loop.call_soon_threadsafe` onto the asyncio queues. The conversation lives in the engine (200 messages), so a reload or a second window replays it from `GET /api/claude/chat`. Sessions in `~/.claude/projects/<folder with non-alphanumerics as dashes>` can be adopted, and a chat can be continued in the terminal with `--resume`. Images from tool results are saved under `Chat/` in the data folder and served by name only.

### pty.py · the Claude Code terminal is connected
- refers-to: backend/app/services/pty.py :: Terminals.create, presets; backend/app/routers/terminal.py :: NewTerminal
- kind: rationale
- logged: v0.26.0 · 2026-10-16

The Claude Code preset used to start a bare `claude`, which reached FormDesign only if the person had trusted the project's `.mcp.json` or run `agents install`. It now starts with the same MCP config, allowed tools and note as the chat (`claude_code.argv`), the cleaned environment, the agent folder, an optional first message (preference) and an optional `resume` id, so a chat moves into the terminal mid-conversation. Availability comes from `claude_code.info`, which also finds installs off PATH.

### services/commands.py, routers/commands.py, command_meta.json · one command catalogue
- refers-to: backend/app/services/commands.py :: catalogue, describe, run, mirror, reply, risky; backend/app/routers/commands.py; backend/app/command_meta.json; backend/app/routers/mcp.py :: list_commands, run_command; backend/app/live_cli.py :: cmd_cli; setup/line_counts.py :: command_words
- kind: rationale
- logged: v0.27.0 · 2026-10-16

formvis's catalogue shape on formdesign's terms. The engine's Rust specs stay the source of ids, labels, menus, shortcuts and signatures (the catalogue JSON now carries each spec's help); `command_meta.json` adds the agent words formvis keeps in command-meta.ts (group, a one-sentence description, a "Use when" sentence, risky), written for all 342 engine commands; the window mirrors its own commands with their words to `POST /api/commands/catalogue` whenever its registry changes (debounced) and on every reconnect. `GET /api/commands`, `GET /api/commands/{id}` and `POST /api/commands/{id}` serve the merged list; an engine command runs through the same routed exec as REST and MCP, a window command is pushed to open windows as `{type: command, id, params, rid}` and the window answers on `/api/commands/reply/{rid}` (30 s), so agents reach panels, view and workspace commands too. Risky means Undo cannot reverse it (saving, exporting, printing, packaging, closing): REST and `./formdesign cli run` need `yes`, MCP `run_command` waits for Agent Connect's approval under Ask. `./formdesign counts --commands --check` fails while any engine command lacks a description, as formvis's does.

### services/layoutimport.py · .qxd and .qxp, checked before and after LibreOffice
- refers-to: backend/app/services/layoutimport.py :: QXP_VERSIONS, qxp_signature, import_soffice
- kind: rationale
- logged: v0.72.0 · 2026-10-17

The .qxd and .qxp route (LibreOffice's libqxp, through the same ODG import as .pub and .pmd files) had been in place since the import work, but its only failure message said the LibreOffice build had no reader for them. That was false here: LibreOffice 24.2 carries libqxp linked into `libwpftdrawlo.so` (the `libqxp::QXPDocument` symbols and the `draw_QXP_Document` filter), so `ldd` shows no separate libqxp. Two checks replace it. Before converting, the header must carry MM or II and then XPR from byte 2, as the system's file(1) magic has it (MMXPR3, IIXPR3, and XPRa for Korean); random bytes named `.qxp` had otherwise gone through another LibreOffice filter into an empty drawing and "nothing to import". After converting, an empty result or a load failure on a file with the signature is reported as libqxp's range (format versions 3.1 to 4.1); the header has no version field the magic reads, so the message names the range rather than the file's version. A file with only the signature reaches libqxp itself (LibreOffice answers "source file could not be loaded" instead of falling back). No real .qxd or .qxp document was reachable from the build machine (LibreOffice's test corpus has none, libqxp has no public GitHub mirror, and the tarball host is outside the network allowlist), so the reader's fidelity on a real file is unverified here; the same route opened LibreOffice's .pub (`fdo59355-1.pub`, 2 pages) and .pmd (`shapes.pmd`, its rectangles, ellipse and text) test documents.

### services/agents.py, engine/core.py · agent turns
- refers-to: backend/app/services/agents.py :: TURN_IDLE, turn_key, end_turns; backend/app/engine/core.py :: turn_var, TURN_METHODS, Core.call; backend/app/routers/mcp.py :: call_tool; backend/app/routers/agents.py :: agents_turn_end; backend/app/services/ai.py :: _loop; .claude/settings.json
- kind: rationale
- logged: v0.73.0 · 2026-10-17

MCP has no notion of a turn, so the backend infers one per agent: the key is the MCP session id (two Claude Code windows never share a turn), or the client name for callers without one (plugins), plus a counter that moves on after `TURN_IDLE` (120 s) without a call from that agent or when `POST /api/agents/turn` arrives. The idle gap is the fallback for clients with no hook and errs toward splitting: a model that thinks for more than two minutes between calls gives two Undo steps, never one step spanning two requests. The exact boundary for Claude Code is the repository's `UserPromptSubmit` hook, which posts the endpoint when the person sends a prompt (curl, one second, `|| true`, so it is silent when FormDesign is not running); the in-app assistant ends its own turn at the start of each message it is sent. The key travels as a `ContextVar` set around `_run` in `call_tool` and reset in `finally`, and `Core.call` adds it to the mutating methods only, so the dozens of `core.call(..., source="mcp")` sites did not change and a window request running concurrently in another task never sees it. The endpoint uses the same local-origin guard as the other Agent Connect routes; it can only split steps.

### services/dxfimport.py, services/layoutimport.py · DXF drawings
- refers-to: backend/app/services/dxfimport.py :: build, import_dxf; backend/app/services/layoutimport.py :: DXF_EXTS, DWG_EXTS, dwg_message, open_layout, probe, emit; backend/app/routers/mcp.py :: open_document; frontend/src/ui/open-layout.ts; setup/freeze_backend
- kind: rationale
- logged: v0.77.0 · 2026-10-19

A `.dxf` opens through ezdxf (MIT): `build` walks model space into the same Sheet the PDF route fills, on one page sized to the drawing's extents plus a margin, scaled to fit with y flipped, and `import_dxf` hands it to `emit` and `finish_report`. The core has no command that makes a path from points, so polylines, arcs, splines and non-rectangular hatch outlines become chains of `line.create` objects; axis-aligned rectangles and full ellipses become frames (`emit` now reads an item's `shape`). Off, frozen and invisible layers, 3D faces, meshes, pattern hatches and points are counted in the report's notes, never dropped silently; `maxObjects` (20,000) caps the chain count. DWG has no permissively licensed reader (the converters are proprietary or GPL), so `dwg_message` refuses it with a request for a DXF export. The frozen engine collects ezdxf with `--collect-all`; numpy arrives with it.

### services/pty.py, routers/terminal.py, services/claude_code.py, routers/claude.py · reuse, restart and the agent folder
- refers-to: backend/app/services/pty.py :: Session, Terminals.reuse, Terminals.restart, Terminals._start, Terminals._exit; backend/app/routers/terminal.py :: NewTerminal, term_new, term_restart; backend/app/services/claude_code.py :: workdir, save_prefs; backend/app/routers/claude.py :: prefs
- kind: rationale
- logged: v0.78.0 · 2026-10-19

The dock opens Claude Code from the menu, the keyboard and the chat, so asking for it twice must not stack tabs. `POST /api/term` with `reuse` and the claude preset returns the first live claude session and types the prompt into it (the text, a 50 ms gap, then a carriage return, so the program sees a typed line and submits it rather than treating text ending in a return as a paste); with no prompt it only returns the session. A request carrying `resume` never reuses: moving a chat into the terminal needs a program started on that conversation, and typing into a running one would lose it. Every response is `{id, preset, title, reused}`; a session's title is fixed when it starts ("Terminal" for a shell, "Claude Code", "Claude Code (from Chat)" when resumed, otherwise the preset label) and the list reports `{id, preset, title, alive}` (`exited` stays beside it for older callers).

`POST /api/term/{sid}/restart` runs the stored argv, folder and environment again under the same id. The scrollback is cleared and a terminal reset is fed to attached sockets so they redraw; the size is the last one the window sent. A resumed claude session restarts with its resume flag and first message, as the reference does, because the argv is not rebuilt. `Session.gen` makes a stale reader or Windows pump from the old program unable to write an exit line into the new run. When a program ends, one line is written once into the stream and `alive` turns false: "[Claude Code exited · Restart runs it again]" for claude, "[Process exited with code N · Restart runs it again]" for the rest. Unknown ids answer 404, and the route has the same local-origin guard as the others. MCP has no terminal tool, so none of this is reachable from it; the only terminal paths an agent has are the window commands it can run through `run_command` (they open a tab but cannot type or read).

The agent folder is the `claudeFolder` preference, else `FORMDESIGN_AGENT_DIR`, else `<data dir>/Agent`, created when missing; it was the repository root before, which put a person's session inside the source tree. The three claude preferences are type-checked: a value that is not text answers 400 instead of being coerced, and each keeps the 2000-character cap.

### core/config.py, engine/documents.py, services/books.py, services/workflow.py · the formdesign rename
- refers-to: backend/app/core/config.py :: _data_dir, _move_legacy_data_dir; backend/app/engine/documents.py :: META, META_MEMBERS, _meta_member; backend/app/services/books.py :: BOOK_FORMAT, BOOK_FORMATS; backend/app/services/workflow.py :: FORMAT, FORMATS
- kind: rationale
- logged: v0.80.0 · 2026-10-20

Every name the engine owns moved from the short form to `formdesign` (environment variables `FORMDESIGN_*`, the MCP server, the CLI prog, loggers, the User-Agent, PDF Creator and Producer, the `/api/version` app name, the frozen entry `formdesign_engine.py`). Four places still know the short form, each in one constant or function, because a person's saved data carries it: the data folder (`~/Formdes` is renamed to `~/FormDesign` once, only when `FORMDESIGN_DATA_DIR` is unset and the new folder does not exist), the `.fdes` metadata member and its `format` value (written as `formdesign.json` and `formdesign`, either member read), the book file's `format` and the assignment package's `format` (written new, both values read). The `.fdes` `mimetype` member is written but never checked on open, so it needs no fallback. Environment variables have no fallback by the owner's choice.
