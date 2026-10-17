# Layout Core

`formdes-core` is a Rust binary that owns every open document. It is a thin shell over formdes's own crates in `native/` behind a JSON-lines protocol on standard input and output: formdes-model (the document model and the `.fdoc` file), formdes-text (shaping, the Knuth and Plass composer, hyphenation, flow through frames, tables, footnotes, drop caps, small caps), formdes-render (raster and PDF output), formdes-interchange (IDML in and out, text import, HTML and EPUB) and formdes-engine (the session with undo and batches, 388 commands, the 41 tools, overlays, tiles). There are no git dependencies; the first build compiles these crates, which takes a few minutes. Source Serif 4, Source Sans 3, Inter and JetBrains Mono are embedded (`native/formdes-core/fonts`, SIL OFL 1.1), as is a SCOWL en_US Hunspell dictionary (`native/formdes-core/dict`). The engine service starts it, restarts it if it dies, and is its only client.

## Source

| Path | What lives there |
| --- | --- |
| `native/formdes-core/src/main.rs` | entry: `--version`, `--selfcheck`, font folders from the environment, then the serve loop |
| `native/formdes-core/src/proto.rs` | the wire format, panics caught per request, binary replies |
| `native/formdes-core/src/core.rs` | method dispatch, documents, `rev` and `selRev`, requests, fonts |
| `native/formdes-core/src/damage.rs` | damage rectangles from which items and stories changed, by `Arc` identity; structural changes repaint everything |
| `native/formdes-core/src/slots.rs` | `slots`: frames labelled `slot:NAME` or `flow:NAME` with their content |
| `native/formdes-core/src/find.rs` | `findChange`: text, GREP, glyph and object modes over story, table cell and footnote text, for the Find/Change window and the `find_change` MCP tool |
| `native/formdes-core/src/spell.rs` | `spellMarks`, `spellCheck`, `spellSuggest`, `spellLanguages`, `spellReload`: spelling in each text's language, built-in US English from the bundled dictionary and the rest from Hunspell dictionaries through spellbook |
| `native/formdes-core/src/textloc.rs` | addressing story, table cell and footnote text; line spans for find and spelling |
| `native/formdes-core/src/preflight.rs` | `preflight`: formdes-engine's `preflight.run` filtered and extended by a profile |
| `native/formdes-core/src/thumb.rs` | `snippetThumb`: a library item's PNG thumbnail, rendered on its own |
| `native/formdes-core/src/web.rs` | `web` and `web.asset`: the document as structured content for the engine's website export |
| `native/formdes-engine/src/view.rs`, `input.rs`, `overlays.rs`, `tools/` | `layout`, `tiles`, `page.png`, `overlays`, `pointer`, `key`, `text`, `cursor`, `tool` and `tools`, answered by formdes-engine |

## The wire format

One request per line: `{"id": 7, "method": "exec", "params": {...}}`. One reply per line: `{"id": 7, "ok": true, "result": ..., "rev": 12, "selRev": 9, "changed": true, "damage": [[x0, y0, x1, y1], ...]}`. A failed call answers `{"ok": false, "error": "..."}` and leaves the document as it was. When the reply carries `"bin": N`, exactly N raw bytes follow the line (tiles, page images, serialized documents, font files).

`rev` moves on every document change and `selRev` on every selection change; `damage` lists the canvas rectangles a change touched, which is what lets the window redraw only those tiles.

## Methods

| Method | Purpose |
| --- | --- |
| `ping`, `hello` | liveness and version |
| `exec`, `batch` | run one command, or a list of `{command, params}` through formdes-engine's `Session::batch` (one undo step), where a later step can refer to an earlier result as `$N`; a failing batch rolls the document back unless `atomic` is false |
| `commands` | the catalogue of 388 commands with menu paths, labels, shortcuts and parameter summaries |
| `inspect`, `state`, `layout` | the document as data, the session (documents, selection, tool, `wantsText`), spreads and pages in canvas coordinates |
| `tiles`, `page.png`, `overlays`, `cursor` | pixels and overlay geometry for the window; overlays also list artboards (with the Artboard tool's active one) and, under the Mesh tool, the selected meshes' points |
| `artboard.png` | `{artboard: index or name, scale?, maxSide?, format?}` to one artboard as PNG or JPEG: the spread's items under its rect, pasteboard included; served as `GET /api/core/artboard.png` |
| `pointer`, `key`, `text`, `tool`, `tools` | input and the tool catalogue |
| `serialize`, `docs.activate`, `docs.close`, `docs.saved` | document bytes and session housekeeping |
| `slots`, `frame.thread`, `frame.unthread` | formdes additions for templates and the Make pass |
| `fonts.add`, `fonts.families`, `fonts.styles`, `fonts.face`, `fonts.data` | the font catalogue; `fonts.data` returns a face's file for the Glyphs panel |
| `story.paras`, `objectStyles`, `pathType` | a story's paragraphs with their styles (Story Editor), object style listing and editing, a type-on-path item's options |
| `findChange` | `{mode: text, grep, glyph or object, op: all, next, change, changeAll, changeFind or select, find, change, findFormat, changeFormat, scope, within, include, glyph, toGlyph, object, objectChange}`; GREP runs on fancy-regex with InDesign's codes translated |
| `spellMarks`, `spellSuggest` | misspelled words in the visible frames as underline segments (cached per document revision); suggestions for one word, from the dictionary of `lang` when given |
| `spellCheck`, `spellLanguages`, `spellReload` | `{story?}` to `[{story, start, end, word, lang}]`; the languages with a dictionary found in `FORMDES_DICT_DIRS`; forget loaded dictionaries after one is added or removed |
| `tiles` `proof` | `{target: workingCmyk, cmyk:NAME, srgb, legacyMacRgb, monitorRgb, protanopia or deuteranopia, intent, simulatePaper}` is approximated in formdes-engine's `view.rs`: a CMYK round trip with a 300 percent ink limit and a perceptual squeeze, paper simulation, legacy Mac gamma, and protanopia or deuteranopia simulation; there is no ICC transform yet |
| `preflight` | `{profile: {overset, fonts, links, emptyFrames, rgb, transparency, minPpi, maxPpi, bleed, safety, minType, spotMax, pageMultiple}}` to `{errors, warnings, issues}` |
| `snippetThumb` | `{base64, size}` (a snippet from `snippet.export`) to a PNG with a transparent background, longest side `size`; the engine calls it when a library item is added |
| `web` | `{}` to `{title, intent, facing, pageWidth, pageHeight, language, pages, blocks, articles, hotspots, styles, swatches}`: placed stories and graphics in reading order (page, then top, then left), each with its page, bounds and slot; stories as paragraphs of runs carrying only what differs from the paragraph (bold, italic, colour, size, position, character style, link); pages with their spread; exported Articles; item hyperlinks as hotspots; paragraph and character styles resolved to plain values |
| `web.asset` | `{asset}` to the placed file's bytes with its type and pixel size |

## Environment

| Variable | Effect |
| --- | --- |
| `FORMDES_FONT_DIRS` | extra font folders, loaded at start (the engine passes the font library) |
| `FORMDES_SYSTEM_FONTS` | `0` keeps the core from cataloguing system fonts |

Warnings and errors go to standard error prefixed `[formdes-core]`; the engine forwards them to its own log.

## Contract

- Anything that changes a document goes through formdes-engine's session, so undo covers it and the change event carries damage.
- New methods answer in milliseconds or move their work into a command; the engine serialises calls, so a slow method blocks every window.
- A method that returns bytes uses `Reply { bin }`, and the engine adds it to `BINARY_METHODS` in `backend/app/routers/core_api.py`; a JSON method is added to `JSON_METHODS` there before any client can reach it.
- formdes-specific behaviour belongs in the engine unless it needs the document model directly. Changes to the layout engine itself go into `native/formdes-*` directly.
- Read `native/HASHOUT_LOG.md` before editing.
