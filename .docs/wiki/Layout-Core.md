# Layout Core

`formdes-core` is a Rust binary that owns every open document. It is a thin shell over formdes's own crates in `native/` behind a JSON-lines protocol on standard input and output. The engine service starts it, restarts it if it dies, and is its only client.

## Source

| Path | What lives there |
| --- | --- |
| `native/formdes-core/src/main.rs` | entry: `--version`, `--selfcheck`, font folders from the environment, then the serve loop |
| `native/formdes-core/src/proto.rs` | the wire format, panics caught per request, binary replies |
| `native/formdes-core/src/core.rs` | method dispatch, documents, undo for formdes's own edits, change metadata, fonts |
| `native/formdes-core/src/view.rs` | `tiles` (RGBA tiles of the canvas) and `page.png` |
| `native/formdes-core/src/overlays.rs` | frame edges, handles, ports, threads, guides, caret and selection geometry |
| `native/formdes-core/src/input.rs` | `pointer` and `key`: tools, snapping, text input |
| `native/formdes-core/src/slots.rs` | `slots`: frames labelled `slot:NAME` or `flow:NAME` with their content |
| `native/formdes-core/src/find.rs` | `findChange`: text, GREP, glyph and object find and change for the Find/Change window and the `find_change` MCP tool |
| `native/formdes-core/src/spell.rs` | `spellMarks` and `spellSuggest`: dynamic spelling marks and ranked suggestions |
| `native/formdes-core/src/preflight.rs` | `preflight`: the engine's checks filtered and extended by a profile |
| `native/formdes-core/src/thumb.rs` | `snippetThumb`: a library item's PNG thumbnail, rendered on its own |

## The wire format

One request per line: `{"id": 7, "method": "exec", "params": {...}}`. One reply per line: `{"id": 7, "ok": true, "result": ..., "rev": 12, "selRev": 9, "changed": true, "damage": [[x0, y0, x1, y1], ...]}`. A failed call answers `{"ok": false, "error": "..."}` and leaves the document as it was. When the reply carries `"bin": N`, exactly N raw bytes follow the line (tiles, page images, serialized documents, font files).

`rev` moves on every document change and `selRev` on every selection change; `damage` lists the canvas rectangles a change touched, which is what lets the window redraw only those tiles.

## Methods

| Method | Purpose |
| --- | --- |
| `ping`, `hello` | liveness and version |
| `exec`, `batch` | run one command, or a list of `{command, params}` through the engine's batches, where a later step can refer to an earlier result as `$N`; a failing batch rolls the document back unless `atomic` is false |
| `commands` | the catalogue of 432 commands with menu paths, labels, shortcuts and parameter summaries |
| `inspect`, `state`, `layout` | the document as data, the session (documents, selection, tool, `wantsText`), spreads and pages in canvas coordinates |
| `tiles`, `page.png`, `overlays`, `cursor` | pixels and overlay geometry for the window |
| `pointer`, `key`, `text`, `tool`, `tools` | input and the tool catalogue |
| `serialize`, `docs.activate`, `docs.close`, `docs.saved` | document bytes and session housekeeping |
| `slots`, `frame.thread`, `frame.unthread` | formdes additions for templates and the Make pass |
| `fonts.add`, `fonts.families`, `fonts.styles`, `fonts.face`, `fonts.data` | the font catalogue; `fonts.data` returns a face's file for the Glyphs panel |
| `story.paras`, `objectStyles`, `pathType` | a story's paragraphs with their styles (Story Editor), object style listing and editing, a type-on-path item's options |
| `findChange` | `{mode: text, grep, glyph or object, op: all, next, change, changeAll, changeFind or select, find, change, findFormat, changeFormat, scope, within, include, glyph, toGlyph, object, objectChange}`; GREP runs on fancy-regex with InDesign's codes translated |
| `spellMarks`, `spellSuggest` | misspelled words in the visible frames as underline segments (cached per document revision); suggestions for one word |
| `preflight` | `{profile: {overset, fonts, links, emptyFrames, rgb, transparency, minPpi, maxPpi, bleed, safety, minType, spotMax, pageMultiple}}` to `{errors, warnings, issues}` |
| `snippetThumb` | `{base64, size}` (a snippet from `snippet.export`) to a PNG with a transparent background, longest side `size`; the engine calls it when a library item is added |

## Environment

| Variable | Effect |
| --- | --- |
| `FORMDES_FONT_DIRS` | extra font folders, loaded at start (the engine passes the font library) |
| `FORMDES_SYSTEM_FONTS` | `0` keeps the core from cataloguing system fonts |

Warnings and errors go to standard error prefixed `[formdes-core]`; the engine forwards them to its own log.

## Contract

- Anything that changes a document goes through the engine's session or `edit_undoable`, so undo covers it and the change event carries damage.
- New methods answer in milliseconds or move their work into a command; the engine serialises calls, so a slow method blocks every window.
- A method that returns bytes uses `Reply { bin }`, and the engine adds it to `BINARY_METHODS` in `backend/app/routers/core_api.py`; a JSON method is added to `JSON_METHODS` there before any client can reach it.
- formdes-specific behaviour belongs in the engine unless it needs the document model directly. Engine changes go into `native/formdes-*` directly.
- Read `native/HASHOUT_LOG.md` before editing.
