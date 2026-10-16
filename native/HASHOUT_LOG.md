# HASHOUT_LOG · native · formdes

Living log for the layout core. Source files stay comment-free; what they would have said lives here, indexed. Read it before editing `native/`.

Current build version: v0.18.0
Last updated: 2026-10-10 · 58 engine commands: clipboard, guides, paths, references, contents, index, data merge, Pathfinder, QR, outlines

---

## Org-navi-map

```
native/
├── Cargo.toml                    workspace of formdes's own crates; no git dependencies
├── Cargo.lock                    resolved dependency graph (committed: a binary crate)
├── HASHOUT_LOG.md                this log
└── formdes-core/
    ├── Cargo.toml                the binary crate, over formdes-engine
    ├── fonts/                    Source Serif 4, Source Sans 3, Inter, JetBrains Mono (OFL), embedded at build
    ├── dict/                     the SCOWL en_US Hunspell dictionary and its notice, embedded at build
    └── src/
        ├── main.rs               flags (--version, --selfcheck), font loading, serve
        ├── proto.rs              JSON-lines framing, length-prefixed binary replies, stderr logger
        ├── core.rs               method dispatch, revision tracking, batches, documents, fonts, object styles
        ├── damage.rs             repaint rects from the items and stories a call changed
        ├── textloc.rs            story, table cell and footnote text addressing; line spans on the canvas
        ├── slots.rs              frames labelled slot:NAME or flow:NAME
        ├── find.rs               findChange: text, GREP, glyph and object find and change
        ├── spell.rs              spelling marks, checks and suggestions in each text's language
        ├── preflight.rs          preflight with profile checks
        ├── thumb.rs              snippetThumb: a library item rendered on its own as a PNG
        └── web.rs                web and web.asset: the document as structured content for website export
└── formdes-model/                formdes's own document model (engine phase E1)
    ├── Cargo.toml
    └── src/
        ├── lib.rs                module list, re-exports, ModelError
        ├── ids.rs                typed u64 ids for items, stories, assets, layers, pages, spreads, parents
        ├── geom.rs               edges, rect helpers, units and page size names
        ├── attrs.rs              the attribute key registry, validation, the resolve stack, CharProps and ParaProps
        ├── color.rs              colours, gradients, swatches, paints
        ├── style.rs              named paragraph, character, object, table and cell styles
        ├── story.rs              paragraph-major stories of text and inline segments
        ├── item.rs               shapes, strokes, text frames, graphics, groups, states, wrap; assets
        ├── page.rs               pages, spreads, parents, sections, layers
        ├── doc.rs                the document arena, spread arrangement, lookups and checks
        └── format.rs             the .fdes zip: mimetype, doc.json, assets/<id>
└── formdes-text/                 formdes's own text engine (phase E2)
    ├── Cargo.toml
    └── src/
        ├── lib.rs                module list and re-exports
        ├── fonts.rs              FontBook: system and folder fonts, family and style matching, fallback per character
        ├── shape.rs              OpenType shaping of one run through harfrust
        ├── lang.rs               language names to BCP 47 tags and hyphenation languages
        ├── kp.rs                 the Knuth and Plass total-fit line breaker and a first-fit breaker
        ├── compose.rs            a paragraph: shaping, breaking, justification, tabs, drop caps, bidi, carets
        ├── para.rs               a story paragraph into composer input: styles, lists, nested and GREP styles, inlines
        ├── flow.rs               a story through threaded frames: columns, span and split, wrap, keeps, widows, notes, tables, carets
        ├── region.rs             path geometry for wrap and shaped frames: spans per band, arc length
        ├── onpath.rs             type on a path: one composed line placed glyph by glyph along a path
        └── layout.rs             a document's stories laid out: frame specs, wrap exclusions, markers and variables
└── formdes-render/               formdes's own output (phase E3)
    ├── Cargo.toml
    └── src/
        ├── lib.rs                Renderer: fonts, outline and image caches; page PNG and PDF entry points
        ├── scene.rs              a page as drawing operations: parents, layers, items, text, decorations, links
        ├── outline.rs            glyph outlines as paths, cached per face and glyph
        ├── images.rs             placed images decoded and placed PDFs rasterised, cached per asset and scale
        ├── raster.rs             the operations drawn with vello_cpu; PNG and JPEG encoding
        ├── view.rs               canvas tiles: placed spreads drawn into one view at any zoom
        └── pdf.rs                the operations written with krilla: embedded subset fonts, CMYK, images, placed PDFs, links
└── formdes-interchange/          file formats in and out (phase E4)
    ├── Cargo.toml
    └── src/
        ├── lib.rs                module list
        ├── text.rs               text import: InDesign tagged text, plain, Markdown, HTML, RTF, Word, CSV
        ├── publish.rs            HTML page and EPUB (reflowable or fixed layout) from the document
        └── idml/
            ├── mod.rs            IDML constants, base64
            ├── dom.rs            a small XML tree over quick-xml for the reader
            ├── read.rs           IDML package to a document: preferences, swatches, styles, parents, spreads, stories
            └── write.rs          a document to an IDML package
└── formdes-engine/               the session, commands, tools and canvas views (phase E4)
    ├── Cargo.toml
    └── src/
        ├── lib.rs                module list, EngineError
        ├── session.rs            open documents, selection, undo history, batches with $N references, state and commands JSON
        ├── registry.rs           command specs (id, label, menu, shortcut, parameter summary, needs) and the builder
        ├── args.rs               parameter readers whose errors say "missing `name`"
        ├── target.rs             ids or the selection; text targets; UTF-8 byte offsets to story positions
        ├── wire.rs               window attribute keys to model keys and back; swatch and style display names; colours
        ├── walk.rs               every paragraph of a document, nested in footnotes and table cells
        ├── assets.rs             placed files: type, pixel size, resolution, modification time
        ├── ops.rs                item operations shared by commands: add, transform, duplicate, arrange, group, thread
        ├── canvas.rs             spreads stacked on the canvas, page rects, hit slots
        ├── view.rs               layout JSON, tiles and page images for the window
        ├── input.rs              pointer, key, text, cursor, tool and tools entry points; key routing; shortcut parsing
        ├── overlays.rs           guides, margins, columns, bleed, baseline grid, frame edges, selection, text and tool feedback
        ├── transplant.rs         copying items and paragraphs between documents with what they depend on
        ├── tools/                the toolbox: one gesture at a time over the session
        │   ├── mod.rs            tool list and shortcuts, gestures, live preview and commit, dispatch by tool
        │   ├── geo.rs            hit testing, handles, anchors, snapping, page and column geometry
        │   ├── select.rs         Selection, Direct Selection, Free Transform, Rotate, Scale, Shear; nudges
        │   ├── draw.rs           frames and shapes, line, pen, pencil, smooth, erase, anchor tools, scissors
        │   ├── text.rs           Type, Type on a Path; caret movement and editing keys
        │   └── misc.rs           eyedropper, colour theme, measure, gradient, note, page, gap, content conveyor, place gun
        └── cmd/                  the command catalogue, one file per area
            ├── mod.rs            registry assembly
            ├── edit.rs           undo, redo, selection, script.run
            ├── object.rs         frames, objects, transforms, states, media, buttons, form fields, threading
            ├── text.rs           typing, stories, character and paragraph formatting, paragraph, character and object styles
            ├── layout.rs         inspect, preferences, document setup, pages, parents, sections, guides, layers, transitions
            ├── file.rs           new, open, save, place, exports, booklet, package, recovery
            ├── swatch.rs         swatches, colour conversion and profiles, links, preflight, snippets
            ├── review.rs         track changes, conditions, notes, user dictionary, hyphenation exceptions
            ├── table.rs          tables, rows, columns, cells, table and cell styles
            ├── find.rs           find and change in text and objects
            ├── xml.rs            tags, structure, DTD, XML export and import
            ├── book.rs           books: chapters, numbering, synchronised styles, one PDF
            ├── clip.rs           copy, cut, paste, paste in place, paste into, step and repeat
            ├── guides.rs         guide list, delete, move, lock and colour; Create Guides
            ├── paths.rs          join, open, close, reverse, compound paths, Convert Shape
            ├── refs.rs           hyperlinks, bookmarks, text variables, footnotes, alt text, captions, placeholder text, rotate spread
            ├── generate.rs       table of contents, index, text anchors and cross-references, data merge
            └── art.rs            Pathfinder, QR codes, Create Outlines, anchored objects
```

---

## Table of contents

- [Cargo.toml](#cargo)
- [formdes-core/src/main.rs](#main)
- [formdes-core/src/proto.rs](#proto)
- [formdes-core/src/core.rs](#core)
- [formdes-core/src/damage.rs](#damage)
- [formdes-core/src/textloc.rs](#textloc)
- [formdes-core/src/find.rs](#find)
- [formdes-core/src/spell.rs](#spell)
- [formdes-core/src/preflight.rs](#preflight)
- [formdes-core/src/thumb.rs](#thumb)
- [formdes-core/src/web.rs](#web)
- [formdes-model](#model)
- [formdes-text](#text)
- [formdes-render](#render)
- [formdes-interchange](#interchange)
- [formdes-engine](#engine)

---

## Cargo.toml {#cargo}

### Cargo.toml · workspace.dependencies
- refers-to: native/Cargo.toml :: [workspace.dependencies]
- kind: rationale
- logged: v0.17.0 · 2026-10-10

Every crate the core builds on is formdes's own and lives in this workspace; nothing is fetched from git, so a build needs only the crates.io registry (or `cargo vendor` for an offline tree). The release profile uses thin LTO and 16 codegen units; dependencies build at opt-level 2 in dev so a debug core stays usable. Beyond the workspace crates the core takes fancy-regex (MIT, GREP) and spellbook (MPL-2.0, Hunspell spelling, used unmodified so the MPL's file-level terms ask nothing of formdes's own files).

---

## formdes-core/src/main.rs {#main}

### main.rs · main
- refers-to: native/formdes-core/src/main.rs :: main
- kind: explainer
- logged: v0.1.0 · 2026-10-07

`--version` prints the crate version and exits before the logger or the font scan start (the doctor and the installers call it). `--selfcheck` makes a Letter document, a text frame and a page render, and exits 0 when all three work; the desktop freeze step runs it on the copied sidecar. Fonts load before serving: the four embedded families first (the templates' fallback roles name them, so they must exist without an install step), then `FORMDES_FONT_DIRS`, then the system fonts unless `FORMDES_SYSTEM_FONTS=0`. Otherwise the core serves JSON lines on standard input and output until standard input closes.

---

## formdes-core/src/proto.rs {#proto}

### proto.rs · serve
- refers-to: native/formdes-core/src/proto.rs :: serve
- kind: explainer
- logged: v0.1.0 · 2026-10-07

One request per line: `{"id", "method", "params"}`. One reply line per request: `{"id", "ok", "result" | "error", "rev", "selRev", "changed", "damage"?, "requests"?}`. A reply with a binary payload carries `"bin": N` and the N raw bytes follow the newline immediately (tiles as premultiplied RGBA, page images as PNG or JPEG, serialized documents). Standard output carries only protocol; the logger writes warnings and errors to standard error, which the engine service forwards to its own log. A panic in a method becomes an error reply instead of a dead process (`catch_unwind`).

---

## formdes-core/src/core.rs {#core}

### core.rs · settle
- refers-to: native/formdes-core/src/core.rs :: Core::settle
- kind: explainer
- logged: v0.1.0 · 2026-10-07

After every method the core compares (active document uid, document Arc pointer, document revision) with what it saw last. A difference bumps `rev` and asks `damage.rs` for the rectangles to repaint, mapped from spread to canvas coordinates: a list the window repaints, or `null` for everything. `selRev` moves when the document, the tool, the selection or the engine's own selection counter changes, so the window refreshes overlays without re-rendering pixels. UI requests the engine queued (dialogs, file pickers, view changes) ride out in the reply's `requests`. The canvas methods (`layout`, `tiles`, `page.png`, `overlays`, `pointer`, `key`, `text`, `cursor`, `tool`, `tools`) are answered by formdes-engine's `view` and `input` modules; the core adds only the protocol around them.

### core.rs · batch
- refers-to: native/formdes-core/src/core.rs :: Core::batch
- kind: rationale
- logged: v0.17.0 · 2026-10-10

Batches run through formdes-engine's `Session::batch`: one undo step, `$N` and `$N.field` references, and a failure restores the document and selection from before the batch, with the core adding the suffix the window strips. `atomic: false` runs the steps one by one with the same references and stops at the first failure without rolling back, for callers that want the earlier steps kept. A `script` parameter is accepted when it is a JSON array of steps.

### core.rs · fonts.styles, fonts.face, fonts.data
- refers-to: native/formdes-core/src/core.rs :: Core::call
- kind: explainer
- logged: v0.7.0 · 2026-10-08

The Glyphs panel draws characters in the document's own fonts, which the window cannot otherwise reach. `fonts.styles` lists a family's styles; `fonts.face` answers the face's character map as JSON (every code point the cmap maps); `fonts.data` answers the font file as binary, served by the engine at `/api/core/font` with a private one-hour cache.

### core.rs · object_styles
- refers-to: native/formdes-core/src/core.rs :: Core::object_styles
- kind: explainer
- logged: v0.17.0 · 2026-10-10

With no target it lists the styles (fill, stroke and paragraph style read from the style's own attributes) and the selection's applied styles. `edit` becomes one `style.object.edit`: fill and stroke are turned into the model's paint and stroke values (a stroke patch merges over the style's current stroke, so a weight alone keeps the swatch), `null` clears an attribute, `redefine` copies fill, stroke and text frame options from the first selected item, `to` renames; the engine re-applies the style to every item using it in the same undo step. `delete` is `style.object.delete`, whose items keep their look.

### core.rs · serialize and documents
- refers-to: native/formdes-core/src/core.rs :: Core::call
- kind: rationale
- logged: v0.17.0 · 2026-10-10

`serialize` answers the active document as formdes-model's `.fdoc` bytes with the title `<title>.fdoc`; the engine stores them as `document.fdoc` inside `.fdes`. `docs.saved` marks the document clean by pointing its saved copy at the current one, which is what the dirty flag compares. An unknown method answers an error instead of being passed on.

---

## formdes-core/src/damage.rs {#damage}

### damage.rs · damage
- refers-to: native/formdes-core/src/damage.rs :: damage
- kind: rationale
- logged: v0.17.0 · 2026-10-10

Items and stories sit behind `Arc`s that clone on write, so an unchanged one is the same pointer in the old and new document and the diff costs one pointer compare per object. A changed item damages its old and new bounds (inflated by its stroke, or by 48 points when it has effects such as a shadow); a changed story damages every frame of it in both documents, and its anchored objects; a change to an item with text wrap reflows every story, so all text frames are damaged. Changes the canvas cannot localise answer `null`: settings, styles, swatches, layers, sections, conditions, variables, document preferences, assets, parents (they show on many pages) and any change of pages or guides. A spread whose item list reordered damages those items.

---

## formdes-core/src/textloc.rs {#textloc}

### textloc.rs · locations and marker text
- refers-to: native/formdes-core/src/textloc.rs :: Loc, Sub, marker_text, view, put, locs, spans
- kind: rationale
- logged: v0.17.0 · 2026-10-10

Find and spelling see text as the window does: one character per model position, paragraphs joined by `\n`, and each inline object as the private-use marker the window's codes expect (page numbers U+E000, U+E007, U+E008, section U+E001, column, frame and page breaks U+E002 to U+E004, indent here U+E005, right tab U+E006, footnote U+E009, table U+E00A, anchored object U+E00E, note U+E00F, variable U+E010). Every marker is three bytes in UTF-8, as the model's own object character is, so byte offsets agree with the story text the window gets from `story.get`. A location is a story, or a table cell (addressed by the table's position in the story, row and column) or the n-th footnote of it; `view` copies that text into a standalone story so the model's editing methods work on cells and footnotes too, and `put` writes the paragraphs back. `spans` turns a character range into per-line segments in canvas space from the composed flow (line carets, baseline, ascent, descent), for match rects and spelling marks.

---

## formdes-core/src/find.rs {#find}

### find.rs · patterns
- refers-to: native/formdes-core/src/find.rs :: text_pattern, grep_pattern, expand, special
- kind: explainer
- logged: v0.9.0 · 2026-10-08

Text mode escapes everything except InDesign's `^` codes, which become characters or wildcard classes; GREP mode passes the expression through but translates InDesign's `\r` (return, the story's `\n`), `\n` (forced line break, U+2028), `\h`, `\v`, `\l`, `\u`, `\<`, `\>` and `~` codes, inside or outside a character class. Story markers are the private-use characters of textloc.rs. Nonbreaking spaces swap between the modes as InDesign has them (`^s` and `~S` are both U+00A0). The change string expands `$0` to `$9` in GREP and the same codes in both modes, and never writes a structural marker (U+E009 to U+E010), because inserting one without its footnote, table or anchor would break the story. fancy-regex (MIT) is used because InDesign's GREP has lookbehind, lookahead and backreferences.

### find.rs · targets and hits
- refers-to: native/formdes-core/src/find.rs :: places, targets, hits, fmt_ok
- kind: explainer
- logged: v0.9.0 · 2026-10-08

Stories are taken in reading order (spread, then the first frame's top and left), each followed by its table cells (row by row) and footnotes, all addressed through textloc.rs (a cell is sent as `cell: {table, row, col}` with `table` the table's position in the story, a footnote as `note: n`). Parent-page, hidden and locked stories are left out unless included; locked ones are found but never changed. A hit must have the find format at its start, at every segment boundary and at every paragraph start inside it; the format's character keys go through the window-to-model attribute mapping (so an unknown attribute is an error) and compare against the resolved style stack (paragraph style chain, paragraph overrides, character style chain, run overrides); paint values compare as paints, numbers within a thousandth. An empty pattern with a format finds maximal runs within each paragraph. Empty regex matches are skipped, as InDesign does.

### find.rs · ops
- refers-to: native/formdes-core/src/find.rs :: run, next, change_current, change_all, apply
- kind: rationale
- logged: v0.9.0 · 2026-10-08

`next` starts after the selection when it lies in a target, goes to the end, then round once to just before where it started, and selects the hit without bumping the document revision (a selection is not an edit). `change` acts only when the selection is exactly a match, so a stale selection never rewrites the wrong text. Edits are applied back to front within each text location so earlier offsets stay valid, in one undo step (`Session::edit_now`); cell and footnote text is edited as a standalone story and written back into its table or footnote. A selection set by `next` or `select` on a cell or footnote hit covers the anchor character in the story, because the engine's text selection addresses stories only. Replacing text across a structural marker is refused and counted as skipped rather than deleting the footnote or table with it. The match rect for scrolling is the union of the line quads in canvas space, or the anchor character's for cells and footnotes.

---

## formdes-core/src/spell.rs {#spell}

### spell.rs · marks
- refers-to: native/formdes-core/src/spell.rs :: found, marks
- kind: explainer
- logged: v0.9.0 · 2026-10-08

Misspellings are computed per story and cached in a static keyed by (document uid, document Arc pointer, revision), so the window's frequent overlay fetches cost nothing until the text changes. Marks are line segments a little below the baseline from textloc.rs's spans, in canvas space, only for stories with a frame overlapping the requested rect.

### spell.rs · suggest
- refers-to: native/formdes-core/src/spell.rs :: suggest, rank, distance
- kind: rationale
- logged: v0.9.0 · 2026-10-08

Hunspell's suggestions come in its own order ("rivr" put "riv" before "river"). They are re-ranked by optimal string alignment distance with accents folded (a swapped pair is one edit, so "teh" ranks "the" first), then by whether the typed letters appear in order in the suggestion (a dropped letter is the commonest typo), then Hunspell's order. For US English, capitals follow the word: all caps stay all caps, a capitalised word gets capitalised suggestions.

### spell.rs · other languages
- refers-to: native/formdes-core/src/spell.rs :: LANGS, files, dictionary, misses, check, languages, reload
- kind: rationale
- logged: v0.11.0 · 2026-10-08

Every language is checked with Hunspell dictionaries through spellbook (MPL-2.0, used unmodified, pure Rust). US English is built in: the SCOWL en_US dictionary is embedded in the binary (permissive licence, its full notice beside it in `dict/`) and also serves English text whose own dictionary is not installed. A run's language is its resolved `language` attribute (the model keeps BCP 47 tags such as `en-GB`; InDesign names are accepted too); text with no language, tracked deletions and hidden conditional text are not checked; a dictionary is found as `CODE.aff` and `CODE.dic` in a folder of `FORMDES_DICT_DIRS`, or failing that any dictionary of the same language (Swiss German uses de_DE), read as UTF-8 (the engine converts when it installs; Latin-1 is decoded byte for byte as a last resort). Dictionaries load on first use and are kept until `spellReload`, which the engine calls after adding or removing one; a non-English language with no dictionary is left unchecked. Runs are grouped into spans of one language before words are cut, so a word whose formatting changes halfway is still one word. Words are cut on letters and inner apostrophes, the typographic apostrophe is straightened for the dictionary (French elisions are written with it), one-letter words and short all-capital words are skipped, and the document's own words (`userWords`) count in every language. `spellCheck` is what the window's Check Spelling uses because it carries each word's language (`null` for the built-in US English).

### spell.rs · suggestions in other languages
- refers-to: native/formdes-core/src/spell.rs :: suggest, fold
- kind: rationale
- logged: v0.11.0 · 2026-10-08

spellbook's suggestions are good but put an accent fix after unrelated words ("trés" offered "tirés" before "très"), and split forms such as "Tag g". They are re-sorted by: no space unless the word had one, then edit distance with accents folded away (so a wrong accent costs nothing), then spellbook's own order.

---

## formdes-core/src/preflight.rs {#preflight}

### preflight.rs · run
- refers-to: native/formdes-core/src/preflight.rs :: run, Profile
- kind: explainer
- logged: v0.9.0 · 2026-10-08

formdes-engine's `preflight.run` gives overset (the message ends with the number of overset characters), missing font and missing or modified link issues, kept as the profile allows; its resolution issue is dropped because the core computes resolution itself. The core adds empty text frames (a first frame whose story is blank, unless it is a labelled slot), resolution both ways regardless of intent, RGB swatches when the profile asks (by default only in print documents, which matches the old behaviour with no profile). Added checks: painted top-level objects (a fill other than [None], or a graphic) that reach a spread's outer trim without the bleed asked for; text frames nearer the trim than the safety distance on a page's outer edges, top and bottom; the smallest resolved type size per story; spot inks in use by item fills, strokes and text against the allowed number; opacity under 100 percent or a blend mode other than Normal; the page count as a multiple. Vector graphics and media are told apart by the asset's MIME type.

### core.rs · pathType
- refers-to: native/formdes-core/src/core.rs :: Core::call
- kind: rationale
- logged: v0.9.0 · 2026-10-08

A read-only look at a type-on-path item's start, flip and align for the options dialog; calling `type.pathOptions` with no changes would have answered the same values but left an undo step.

## formdes-core/src/thumb.rs {#thumb}

### thumb.rs · snippet
- refers-to: native/formdes-core/src/thumb.rs :: snippet, unbase64
- kind: rationale
- logged: v0.10.0 · 2026-10-08

A library item is a snippet (`snippet.export`, base64 of a `.fdoc` holding only the exported items). The thumbnail loads that snippet as its own document, lays it out, and renders the first spread with items, cropped to the union of their bounds, fitted to `size` with a 4 percent margin, on a transparent background with no paper and printing-only content, so the panel decides the ground it sits on. Rendering it inside the open document instead would have needed a place, render and undo, touching the person's undo history and revision. The decoder accepts both base64 alphabets and ignores padding and whitespace, which spares a crate for one call; the engine calls this once per Add, never per frame.

## formdes-core/src/web.rs {#web}

### web.rs · content and asset
- refers-to: native/formdes-core/src/web.rs :: content, asset, placed, block, story_paras, run_attrs, para_style_json, char_style_json
- kind: rationale
- logged: v0.12.0 · 2026-10-08

The core hands the engine data, not HTML: the site builder lives in Python where its rules can change without a core build. Placed items are walked through groups and kept when visible and on a page (the pasteboard is not content); a story appears once, where its first frame is, with the pages all its frames touch. Runs carry only what differs from their paragraph's resolved character properties (weight over 600, italic, underline, strike, caps, sup and sub, colour, family, a size ratio, the character style and a link), merged when equal, so a long story stays small. Text hyperlinks split runs at their character offsets; item hyperlinks become the item's `href` and page hotspots. Variables are written out with their values for the block's page, right tabs become tabs and then spaces, other inline objects (anchors, footnote, table and note marks) are dropped, forced line breaks become `\n`, deleted tracked changes and hidden conditional text are skipped. Styles are resolved through `basedOn` here because only the core can, and colours go out as hex from the document's paint resolution (tints applied). Style and swatch names go out in the window's spelling (`[Basic Paragraph]`, `[Black]`). Assets are fetched one at a time by `web.asset` so the `web` reply stays JSON.

## formdes-model {#model}

### formdes-model · shape of the model
- refers-to: native/formdes-model/src/doc.rs :: Doc; story.rs :: Story, Para, Seg; attrs.rs :: KEYS, Stack
- kind: rationale
- logged: v0.13.0 · 2026-10-10

The first piece of formdes's own engine, designed from the IDML and PDF specifications and published typesetting literature rather than from any other engine's source. Items live in one flat arena keyed by id, each behind an `Arc` so an undo snapshot of the whole document is a cheap clone and an edit copies only the item it touches (`item_mut` is `Arc::make_mut`). Spreads, parents and groups hold ordered id lists, which are the z-order. Stories are paragraph-major: a story is a list of paragraphs, each a style, its overrides and a list of segments that are either text runs or inline things (anchored items, footnotes, tables, variables, markers, breaks), so nothing hides in private-use characters and a paragraph's formatting is never inferred from a separator. Formatting is a key-value bag checked against one registry of keys (`KEYS`), resolved through a stack of default, style chain, paragraph and run into typed `CharProps` and `ParaProps`; a new attribute is one registry line and one field. The native file is a zip with an uncompressed `mimetype` first, `doc.json` and one `assets/<id>` per embedded file, and `load` also takes bare JSON so documents can be written by hand or by an agent.

## formdes-text {#text}

### formdes-text · fonts and shaping
- refers-to: native/formdes-text/src/fonts.rs :: FontBook, Face; shape.rs :: shape
- kind: rationale
- logged: v0.13.0 · 2026-10-10

fontdb indexes system fonts and any folder in `FORMDES_FONT_DIRS`; a face's bytes are read on first use and kept behind an `Arc`, with its harfrust `ShaperData` built once. skrifa 0.46 is pinned because it shares read-fonts 0.43 with harfrust 0.13, so one parse of the bytes serves metrics, the cmap and shaping. A style name ("Semibold Italic") is matched first against the face's own typographic subfamily name and then by weight, width and slope; a character the face lacks falls back through a fixed family list and then any installed face, cached per face and character. Shaping returns advances in points in logical order (right-to-left output is reversed), so cluster indices rise through a paragraph and widths of any byte range can be summed.

### formdes-text · the composer
- refers-to: native/formdes-text/src/kp.rs :: breaks, greedy; compose.rs :: shape_para, prepare, Composer::compose, build_line
- kind: rationale
- logged: v0.13.0 · 2026-10-10

A paragraph is shaped once per run and font, then cut into boxes, glue and penalties at the UAX 14 break opportunities (`unicode-linebreak`); hyphenation points from `hypher` inside alphabetic words become flagged penalties carrying the hyphen's width, and soft hyphens and hard hyphens break the same way. The breaker is Knuth and Plass's total-fit algorithm (Breaking Paragraphs into Lines, 1981): badness from the adjustment ratio cubed, demerits with flagged and fitness-class penalties, a limit on consecutive hyphens, and active nodes keyed by line number only while line widths differ, because wrap and drop caps make widths vary per line. Ragged alignments get Knuth's end-of-line stretch of three ems instead of stretchable spaces, so the composer still evens the rag. When no break set fits the tolerance a second pass raises it with emergency stretch, and the first-fit breaker is the last resort and the composer for paragraphs with tabs, whose widths depend on position. Justification spends word-space stretch first, then letter spacing within the paragraph's limits, then spaces again. Bidirectional paragraphs are reordered per line with `unicode-bidi`'s visual runs. Drop caps are shaped at the size whose cap height spans the drop lines, and the first lines narrow by their width.

### formdes-text · flow through frames
- refers-to: native/formdes-text/src/flow.rs :: Flower::flow, plan, place_para, place_table, finish
- kind: rationale
- logged: v0.13.0 · 2026-10-10

Line slots are planned ahead down the chain of columns and frames from an estimated leading: each band's free intervals are the column minus any wrap exclusions, so text runs on both sides of an object as two slots on one baseline, a jump object skips the band and a next-column wrap ends the column. The paragraph is composed against those slots, then placed with each line's real leading; if a line no longer fits, the rest of the paragraph is recomposed from that line in the next column, so a change of width between columns is never stretched over. Keep together and keep first lines move the paragraph's start; keep last lines cuts the paragraph earlier and recomposes the tail in the next column. Space before is dropped at the top of a column, as in InDesign. Footnotes are composed at the column width and stack up from its foot, shortening the column. Tables are laid out row by row with header rows repeated after a break. Vertical justification and balanced columns run last, per frame.

### formdes-text · spanning, splitting, shaped frames and contour wrap
- refers-to: native/formdes-text/src/flow.rs :: intervals, place_split, balance, span handling in Flower::flow; region.rs
- kind: rationale
- logged: v0.14.0 · 2026-10-10

A paragraph that spans columns first balances the text above it in that frame (rows of lines that share a baseline move together, so wrapped bands stay intact), is then placed across the frame's full width, and sets a region top from which the following columns start; this is InDesign's span behaviour. A split paragraph is composed once at the sub-column width and laid out row by row inside its column as one block. Free space in a band is computed on polylines flattened from paths: a shaped frame keeps only the spans inside the shape at every sample height of the band, a contour wrap removes the union of the object's spans widened by its offset, an inverted wrap keeps only what lies inside the object, and "largest" keeps the wider side. Frames with wrap or a shape are not rebalanced, because their line widths depend on position.

### formdes-text · optical margins, glyph scaling, balanced rag, carets, type on a path
- refers-to: native/formdes-text/src/compose.rs :: optical, hang, Composer::compose (run_breaks), build_line; flow.rs :: Flow::caret, Flow::hit; onpath.rs :: lay
- kind: rationale
- logged: v0.14.0 · 2026-10-10

Optical margin alignment hangs quotes, hyphens, dashes and stops outside the measure by a share of their advance, widening the line by what hangs on each side. Justification uses the paragraph's glyph scaling range after word and letter spacing, and records the factor on the run so outlines are scaled to match their advances. Balance ragged lines breaks once, then again at the paragraph's average line length plus six percent, and keeps the second result only when the line count is unchanged. A flow keeps each paragraph's character map and text, so story positions convert to a line and an x for the caret, and a point converts back to the nearest caret on the nearest line band. Type on a path composes the story as one line and places each glyph at the arc-length position of its centre, rotated to the path's direction; flip walks the path backwards.

## formdes-render {#render}

### formdes-render · scenes and two back ends
- refers-to: native/formdes-render/src/scene.rs :: page_scene, Ctx::emit; raster.rs :: rasterize; pdf.rs :: export
- kind: rationale
- logged: v0.14.0 · 2026-10-10

A page becomes a list of drawing operations in page points (fills, strokes, glyph runs, images, clips, opacity and blend groups, links) and two back ends consume the same list, so the screen and the PDF cannot disagree about what is on a page. Layers are listed top first, as in the Layers panel, and drawn bottom first; within a layer the parent page's items go under the page's own, a parent based on another draws its base first, and overridden items are skipped. Colours stay in their document model until a back end needs them: the raster converts to sRGB, the PDF writes CMYK as CMYK and gray as gray, so process colour survives export. Text runs keep their source text and per-glyph text ranges, so the PDF's text is searchable and copies as written. Glyph outlines come from skrifa and are cached per face and glyph; the PDF embeds subset fonts through krilla. Placed PDFs are drawn as vector pages in PDF output and rasterised through hayro on screen; JPEG, PNG, GIF and WebP pass through to the PDF without recompression. Frame fill, then text, then stroke follows InDesign's stacking within a text frame. Anchored objects are drawn where the flow placed them, not at their stored position.

### formdes-render · parent text per page, table cells, stop opacity
- refers-to: native/formdes-render/src/scene.rs :: Ctx::text, page_scene, spread_ops, table drawing; native/formdes-text/src/layout.rs :: layout_doc, layout_story_at, DocLayout::frame_on
- kind: rationale
- logged: v0.15.0 · 2026-10-10

A parent text frame holding a page number, a next or previous page number or a text variable is laid out once for every document page that uses its parent (directly or through a based-on parent), and the scene asks for the layout of the page it is drawing, so folios read 1, 3, 5 rather than `#`. Stories with no such marker keep their single layout. Table cells take their fill from the cell, or from the table's alternating rows for body rows, their stroke weight and colour from the table border, and their insets and vertical justification from the cell. Gradient stops carry their own opacity, multiplied into the brush in both back ends.

### formdes-text · hidden text and hyphenation exceptions
- refers-to: native/formdes-text/src/para.rs :: Context::hidden, Context::exceptions, build; compose.rs :: parse_exceptions, syllables
- kind: rationale
- logged: v0.15.0 · 2026-10-10

Text marked as a tracked deletion, and text whose conditions are all hidden, is left out of the composed paragraph while its story positions still map to the next visible character, so carets and offsets stay valid. A document's hyphenation exceptions (`ex~am~ple`) are parsed once per story layout; a listed word breaks only at its tildes and a word listed without tildes never breaks. Notes take no space in the line.

## formdes-interchange {#interchange}

### formdes-interchange · reading IDML
- refers-to: native/formdes-interchange/src/idml/read.rs :: read, build_item, paragraph_range, table_of, styles, graphic; dom.rs :: parse
- kind: rationale
- logged: v0.15.0 · 2026-10-10

The package is read into a small element tree (`dom.rs`) and walked: Preferences give page size, facing pages, bleed, slug, margins and the baseline grid; Graphic gives named colours, tints and gradients as swatches and unnamed colours as direct paints, keyed by their `Self` so `FillColor` references resolve; Styles gives paragraph, character and object styles with their attributes mapped back to model keys. Spread coordinates in IDML have their origin at the spine and the vertical centre, so each spread's offset is the smallest page origin and every top-level item's `ItemTransform` is composed with it; group children keep their transforms relative to the group. Paths become rectangles, ellipses and lines when they are exactly that and paths otherwise. Stories are parsed after the frames: `Br` ends a paragraph, `<?ACE 18?>` is a page number, notes, footnotes, tables and anchored frames become inlines, and frames are threaded by following `PreviousTextFrame` and `NextTextFrame`. Embedded images come from `Contents`; linked ones are read from their path when it exists and are otherwise kept as missing links with their size from `GraphicBounds`.

### formdes-interchange · text import, HTML and EPUB
- refers-to: native/formdes-interchange/src/text.rs :: import, tagged, docx, rtf, html, markdown, csv; publish.rs :: html, epub
- kind: rationale
- logged: v0.15.0 · 2026-10-10

Placed text arrives as paragraphs with style names; a paragraph with no style takes the frame's first paragraph style, and names the document lacks are created empty so the text keeps its structure. Tagged text follows the format the template compiler writes (`<ParaStyle:>`, `<CharStyle:>` runs, `<0xHHHH>` escapes, backslash escapes). Word files read `word/document.xml` with style ids resolved to names through `word/styles.xml`; bold and italic runs become font style overrides. HTML and EPUB take stories and images in reading order: articles when the document has them, otherwise first frames and images by page, then by row, then left to right. Paragraph styles become CSS classes from their resolved attributes and pick a heading element from their export tag or their name. The fixed-layout EPUB draws each page as an image and keeps the page's text, transparent, over it.

## formdes-engine {#engine}

### formdes-engine · the session, undo and batches
- refers-to: native/formdes-engine/src/session.rs :: Session::execute, Session::group, Session::batch, substitute, state_json, commands_json
- kind: rationale
- logged: v0.15.0 · 2026-10-10

Documents are immutable behind an `Arc`; a command that changes one calls `Ctx::doc_mut`, which clones on write, so the snapshot taken before the command is the undo step and costs only the parts that changed. A command that fails after changing the document is rolled back to that snapshot. Queries never call `doc_mut`, so they never bump the revision or add history, which the window's caches depend on. A batch runs its steps inside one group: every step's own history entries are replaced by one entry labelled with the steps' common label (or "Script"), and any failure restores the document and selection from before the batch, naming the step that failed. A string parameter that is exactly `$N` or `$N.field` is replaced by step N's result or that field of it, counting from 0, anywhere in the parameters.

### formdes-engine · the command catalogue and the window's wire format
- refers-to: native/formdes-engine/src/cmd/*.rs :: register; wire.rs :: char_in, char_out, para_in, para_out; target.rs :: items, text_targets, story_char
- kind: rationale
- logged: v0.15.0 · 2026-10-10

Every command the window, the backend, the template compiler and the agent recipes send is registered with a label, a menu path, a shortcut, a one-line parameter summary and what it needs (a document, objects, text), so menus can be built and enabled from the catalogue. Object commands take `ids` or fall back to the selection; text commands act on the text selection or on whole stories of selected text frames. The window speaks UTF-8 byte offsets and InDesign attribute names (`fontFamily`, `firstLineIndent`, `leftJustified`, leading as `{kind, value}`); the model counts characters and keeps its own keys, so `target.rs` converts offsets at the boundary and `wire.rs` maps attributes both ways, which lets Redefine Style send back exactly what `type.selectionAttrs` returned. Character changes at a bare caret are kept as pending formatting for the next typed text. Missing parameters fail with "missing `name`", which opens the window's parameter box. Page indices are 0-based except `layout.pageSize`, `layout.section` and `button.set`, which take page numbers, as the window sends them.

### formdes-engine · pages, parents, overrides and books
- refers-to: native/formdes-engine/src/cmd/layout.rs :: insert_pages, delete_pages, pages_move, duplicate_spread, override_all; book.rs
- kind: rationale
- logged: v0.15.0 · 2026-10-10

Page changes go through the model's repaginate, which carries each spread item with the page under its centre and re-forms spreads, so inserting, deleting, moving and resizing pages keeps objects on their pages. Deleting a page deletes its objects and removes their frames from threads, leaving the story overset rather than losing text. Override All Parent Page Items copies the parent's items for that page onto the page, records the parent item on the copy and hides the original on that page; removing overrides deletes those copies, and detaching forgets the link. A book holds its chapters as loaded documents; Update Numbering starts each chapter after the previous one's last page and Synchronize copies the style source's paragraph and character styles and swatches; both write the chapter files back in place, as the backend expects.

### formdes-engine · gestures, live preview and one undo step
- refers-to: native/formdes-engine/src/tools/mod.rs :: Session::begin_edit, preview, edit_now, commit, restore; Gesture, G
- kind: rationale
- logged: v0.16.0 · 2026-10-10

A pointer down starts at most one gesture, which remembers the slot it began in (the spread under the pointer, with its canvas transform), the start point in spread space and, for gestures that change the document, the document and selection before them. Every drag rebuilds the change from that saved document rather than adding to the last frame, so a drag never accumulates rounding and the window sees one revision per drag batch. Releasing commits one undo step labelled for the gesture (Move, Resize, Rotate, Edit Path, Gap...); a release without movement, or Escape, restores the saved document and adds nothing. Clicks that change the document at once (eyedropper, convert to text frame, place) go through `edit_now`, one step each. Tool switches finish an open pen path, drop the loaded thread and turn a text selection back into its frame.

### formdes-engine · what each tool does with a click and a drag
- refers-to: native/formdes-engine/src/tools/select.rs, draw.rs, text.rs, misc.rs
- kind: rationale
- logged: v0.16.0 · 2026-10-10

Selection checks, in order: a loaded thread, the out port of a single selected text frame (14 screen pixels above its bottom right corner, so it does not collide with the corner handle the window draws there), the eight handles, the rotate zone just outside a corner, then the topmost selectable item (unfilled closed shapes count only on their edge, as in InDesign); Alt drags a copy, Shift adds or removes, empty space starts a marquee. Clicking the out port loads the text; the next click threads into the frame under it or makes a new frame the size of the source, and a drag makes one that size. Double-click enters text, a group's child, or the image inside a graphic frame. Direct Selection grabs an anchor or handle of a selected path, or of the path under the pointer, so a corner can be dragged in one gesture. Drawing tools snap to page edges, margins, columns, guides and other objects' edges and centres (6 screen pixels) unless the view turns snapping off; Shift squares or constrains to 45 degrees, Alt draws from the centre, and a click with no drag asks the window for the size dialog. The Type tool sets the caret, drags a selection across threaded frames, turns an empty shape into a text frame, or draws a new one. The place gun takes the first loaded file: a click into an empty frame fills it, a click on empty space places at natural size (text fills the column under the click), a drag sets the frame.

### formdes-engine · keys
- refers-to: native/formdes-engine/src/input.rs :: key, Combo; tools/text.rs :: key; tools/select.rs :: key
- kind: rationale
- logged: v0.16.0 · 2026-10-10

Key routing keeps the contract's order: the tool first while typing or mid-gesture, then a command whose shortcut matches the exact modifiers (not while typing unmodified), then a tool shortcut (not while typing, not with Cmd or Ctrl), then the tool, then typing. Command shortcuts are written with `Mod` for the platform command key, which matches `cmd` in the event. In text, arrows move by character or, with Alt or Cmd, by word; Up and Down keep the caret's x across lines; Cmd+Up and Down go by paragraph; Home and End go to the line's ends (the story's with Cmd); Shift extends. Backspace and Delete go through `text.delete`, so tracked changes are respected; typing runs coalesce into one "Typing" undo step until the selection moves. Outside text, arrows nudge by 1 point, 10 with Shift.

### formdes-engine · overlays
- refers-to: native/formdes-engine/src/overlays.rs :: overlays, furniture, selection_json, text_json
- kind: rationale
- logged: v0.16.0 · 2026-10-10

Overlays are the contract's shape, in canvas coordinates rounded to 0.01 and flattened at a quarter of a screen pixel. Furniture is limited to spreads within 400 points of the visible rect. The baseline grid is sent from 75 percent zoom up. Frame edges cover items without a stroke and every text and graphic frame; empty graphic frames add the cross. Parent items appear per page, from the parent's matching side, through its based-on chain, minus items overridden on that page. Text selections become one quad per line, extended to the line end when the selection runs on; a caret in an empty story sits at the frame's top left. Selected table cells become quads too. The pen path in progress is sent as tool feedback and its anchors with the selection's, the last one picked. Guides carry their own colour beside the contract's fields.

### formdes-engine · tiles, ink limit and proof
- refers-to: native/formdes-engine/src/view.rs :: tiles, view_opts, proof_of, proof_px, layout_json
- kind: rationale
- logged: v0.17.0 · 2026-10-10

Tiles render every placed spread's cached drawing operations into the requested device rect, pasteboard colour behind, page paper and shadow outside Preview. `plate` renders one separation as greys (ink as darkness). `inkLimit` renders the four plates, sums the coverage per pixel and paints red where the total passes the limit, skipping pasteboard pixels. `proof` is an approximation until a colour engine with ICC profiles is in: CMYK targets round-trip each pixel through CMYK with a 300 percent total ink limit (perceptual intent first pulls saturation in by 6 percent), paper simulation (or the absolute intent) maps white to a warm paper and black to a lifted ink black, `legacyMacRgb` applies the 1.8 to 2.2 gamma difference, and protanopia and deuteranopia use the Vienot matrices in linear light; `srgb` and `monitorRgb` change nothing. Unknown targets and intents are errors, as the contract asks. A plate or the ink limit wins over a proof. Not yet drawn: overprint preview, note and tracked-change markers, and the highlight modes. `layout` sends each slot's spread as `{kind, index}`, the shape `guide.add` takes back.

### formdes-engine · start page and sides
- refers-to: native/formdes-engine/src/cmd/layout.rs :: document_setup; cmd/file.rs :: new_doc
- kind: rationale
- logged: v0.17.0 · 2026-10-10

In a facing document an even start page number makes the first page a left page, as in InDesign, so `startPage` sets the document's first side and re-forms the spreads (the template compiler sends `startPage: 2` for templates whose first page is a left page, and their items assume pages 1 and 2 share a spread).

### formdes-engine · line.create
- refers-to: native/formdes-engine/src/cmd/object.rs :: line_create
- kind: explainer
- logged: v0.17.0 · 2026-10-10

`{spread?, a, b, weight?}` draws a graphic line between two spread points with a 1 point black stroke (or `weight`) on the target layer, selects it and answers its id; the template compiler draws rules with it and styles the stroke next.

### formdes-engine · the clipboard and transplant
- refers-to: native/formdes-engine/src/cmd/clip.rs :: Clip, take, do_paste, paste_items, paste_into_text, paste_into; transplant.rs :: Mapper
- kind: rationale
- logged: v0.18.0 · 2026-10-10

The clipboard lives on the Session, not a document, so it survives switching documents, and it holds an `Arc` of the source document as it was at copy time plus the ids (or the sliced paragraphs): copying is free and later edits to the source never change what pastes. One `Mapper` copies items and paragraphs into any document, the same one or another, bringing their stories, anchored objects, pictures (shared when the same asset data is already there), swatches, styles with their based-on chain and text variables; copying within a document goes through the same path so there is one behaviour to reason about. A selected child of a group pastes at its place on the page (its world transform), and a group whose child is also selected is copied once. Objects pasted onto the spread they came from move 12 points so the copy is visible; `at` centres them on a point (the window sends the view's centre); Paste in Place never moves them. Objects pasted into text become inline anchored objects. `text` is the system clipboard's text: it wins only when it differs from what formdes copied last, so formatting survives a round trip through the system clipboard. Paste Into is the picture case only: the frame takes the copied picture's asset at the same place on the page; arbitrary objects inside a frame need a clipping container the model does not have.

### formdes-engine · guides and Create Guides
- refers-to: native/formdes-engine/src/cmd/guides.rs :: where_of, create
- kind: explainer
- logged: v0.18.0 · 2026-10-10

Guides are addressed by index on a spread or parent spread (`guide.list` gives the indices). Delete without `index` clears the spread, as View > Grids & Guides > Delete All Guides on Spread does. Create Guides divides each page's margin box (or the page) into rows and columns with gutters, one guide at each side of a gutter, a single guide when the gutter is 0, all limited to their page.

### formdes-engine · paths and Convert Shape
- refers-to: native/formdes-engine/src/cmd/paths.rs :: subpaths, join, compound, release, convert; formdes-render/src/scene.rs :: emit (fillRule)
- kind: rationale
- logged: v0.18.0 · 2026-10-10

Every shape kind turns into a `Shape::Path` through `to_path` before a path edit, so rectangles, ellipses and polygons can be opened, reversed and joined like drawn paths. Join takes exactly two open single-run paths, picks the pair of end points closest together, reverses either path to suit, and adds a straight segment when the ends are more than half a point apart. A compound path is one `Path` whose sub-paths keep their own winding and whose `extra.fillRule` is `evenodd`, which the scene passes to the fill so holes show however the parts were drawn; Release splits the sub-paths back into copies of the item. Convert Shape keeps the bounding box and maps the corner shapes onto the model's corner kinds (rounded, bevel, inset, inverse).

### formdes-engine · references
- refers-to: native/formdes-engine/src/cmd/refs.rs; formdes-render/src/pdf.rs :: export (outline)
- kind: explainer
- logged: v0.18.0 · 2026-10-10

Hyperlinks use the model's `hyperlinks` (text ranges in story positions, or an object) and are drawn as PDF links by the scene; a bare web address gets `https://`. Bookmarks live in `prefs.bookmarks` as `{name, page, level, y}` sorted by page; PDF export turns them into the outline, a level-2 bookmark nesting under the one before it. Text variables, footnotes and their editing use the model's `Inline::Variable` and `Inline::Footnote`; a date variable defined without a date takes today's. Fill with Placeholder Text lays the document out with more and more sentences (doubling, then a binary search) and keeps the most that fit without overset. Rotate Spread is a view setting on the document state (`rotations`), not a document change, so it bumps the revision without an undo step.

### formdes-engine · contents, index, cross-references and data merge
- refers-to: native/formdes-engine/src/cmd/generate.rs :: page_of_pos, toc_build, collect_index, xref_text, data_merge, fill_record; formdes-model/src/story.rs :: Inline::Index, Anchor, Xref
- kind: rationale
- logged: v0.18.0 · 2026-10-10

Page numbers come from the laid-out text: `page_of_pos` finds the line holding a story position and the page of that line's frame. The table of contents and the index remember their settings and target story in `prefs.toc` and `prefs.index`, so Update rebuilds the same frame and the generated stories are never collected into themselves. Entries sort by page, then reading order (page, top, left of each story's first frame). TOC entry styles get a right tab at the column width with the leader, so page numbers align right. Index markers and text anchors are inlines that take no room, so they move with the text they mark through every edit; a cross-reference stores its text, shown like a variable, and `xref.update` recomputes it after the layout changes (the same model as InDesign's out-of-date cross-references). Data merge builds a new document: the first record fills the original pages, and for each further record the pages are appended and the items copied onto them through `Mapper` and moved by `repaginate`'s carried page index; `<<Field>>` in text and frames labelled or named `<<@Field>>` (a picture path) are filled. Threaded stories are copied per frame, so a story threaded across pages does not stay threaded in the merged document.

### formdes-engine · Pathfinder, QR codes, outlines and anchored objects
- refers-to: native/formdes-engine/src/cmd/art.rs :: pathfinder, qr_path, qr_code, create_outlines, anchored_insert, anchored_release
- kind: rationale
- logged: v0.18.0 · 2026-10-10

Pathfinder flattens each shape in page coordinates (0.05 point tolerance), normalises an even-odd item through i_overlay first, and combines them in paint order (back to front); the result replaces the back object for Subtract, the front one otherwise, and is written back in that object's own coordinates. Results are straight segments, which is why the tolerance is fine. A QR code is one path of module runs (a rectangle per run of dark modules in a row), centred and square in the box, with the text kept in `extra.qr` so running the command on it again re-encodes in place; an empty selected frame is replaced, any other selected frame is covered. Create Outlines renders the frame alone in a copy of the document (its spread holding only the frame, parent pages off, its own fill and stroke cleared) and turns the glyph runs into paths with the renderer's own glyph transform, one item per text colour, grouped when there are several, keeping the frame's place in the stacking order. Releasing an anchored object puts it on its frame's spread where the text placed it, using the same placement the scene draws with.

### formdes-model · joined pages
- refers-to: native/formdes-model/src/page.rs :: Page.joined; doc.rs :: arrange
- kind: rationale
- logged: v0.19.0 · 2026-10-10

A page with `joined` stays in the spread of the page before it, so `arrange` closes a spread only when the next page is not joined. That is how Add Page to Spread builds gatefolds of up to ten pages without a separate spread list to keep in step with page moves, inserts and deletes; Allow Spread to Shuffle off is the same flag set on every page after the first in the spread.

### formdes-engine · production commands
- refers-to: native/formdes-engine/src/cmd/prod.rs :: ink_options, type_open_type, change_case, attrs_catalog, type_attrs, font_replace, style_load, file_print, export_svg, list_define, page_rule, liquid_object, create_alternate, adapt_page
- kind: rationale
- logged: v0.19.0 · 2026-10-10

Ink settings live in `prefs.inks` (`toProcess`, `alias`) and `$allToProcess`, which preflight and the separations read, so the document's swatches never change. `type.attrs` sets any character or paragraph attribute by its model name and `type.attrsCatalog` lists the names, kinds and current values; the Paragraph and Character Options dialogs are built from it, so a new model attribute appears in them without window code. Load Styles reads a .fdes, .fdoc or .idml into a scratch document and copies the chosen kinds through the same `Mapper` paste uses. Print hands the exported PDF to `lp` or `lpr` when either exists and otherwise reports where the PDF was written. `adapt_page` moves and resizes a page's items when its size or margins change: `scale` scales everything, `recenter` keeps sizes and centres, `guide` maps positions from the old margin box to the new one, and `object` follows each item's pins (`liquid.object`); Document Setup, Margins and Columns with adjustLayout, Resize Pages and Create Alternate Layout all call it.

### formdes-engine · canvas marks and Overprint Preview
- refers-to: native/formdes-engine/src/marks.rs :: spread_marks; view.rs :: tiles; formdes-render/src/scene.rs :: SceneOpts.overprint
- kind: explainer
- logged: v0.19.0 · 2026-10-10

Note flags, inserted-text highlights and deleted-text carets are drawn as extra ops after the spread's own, from the laid-out lines, so they follow the text and never print. Overprint Preview wraps overprinting fills and strokes in a multiply blend, which is how ink on ink looks; the session's op cache keys on the overprint flag so turning it off redraws.

### formdes-engine · ICC soft proofing
- refers-to: native/formdes-engine/src/view.rs :: IccProof, proof_of
- kind: rationale
- logged: v0.19.0 · 2026-10-10

A proof is two moxcms transforms (working RGB to the output CMYK profile and back) built once per profile and intent and cached for the session, applied to the rendered tile's pixels. `cmyk:<name>` resolves through the profiles the session found; without one the working CMYK from Color Settings is used. Paper simulation maps white to the profile's paper white, so the proof greys as the press sheet would.

### formdes-engine · typing in table cells
- refers-to: native/formdes-engine/src/celltext.rs :: CellText, hit, edit, key
- kind: explainer
- logged: v0.19.0 · 2026-10-10

The cell cursor is a `CellText` on the selection (story, table position, row, column, anchor and focus as offsets in the cell's own paragraph text). The Type tool's click hits a cell before the frame's text; typing, deleting and pasting edit the cell text; arrows move inside it, Tab and Shift+Tab move between cells and Tab in the last cell adds a row, Escape returns to the table's anchor in the story.

### formdes-engine · Smart Text Reflow
- refers-to: native/formdes-engine/src/reflow.rs :: smart_reflow; session.rs :: execute
- kind: rationale
- logged: v0.19.0 · 2026-10-10

After typing, pasting, placing, placeholder text or Replace Text, a story still overset whose last frame fills at least 60 percent of its page's margin box gets new pages, each with a copy of that frame threaded on, until it fits or forty pages are added. The pages join the same undo step as the edit, so one Undo takes both away. Smaller frames (captions, pull quotes) are left overset, as in InDesign, where reflow follows the primary text frame.

### formdes-engine · tables, endnotes and selection
- refers-to: native/formdes-engine/src/cmd/more.rs :: text_to_table, table_to_text, select_container, endnote_insert, renumber_endnotes
- kind: rationale
- logged: v0.19.0 · 2026-10-10

Endnotes are cross-reference inlines with format `endnote`, numbered in page order and gathered into one endnote frame with Endnote Title and Endnote styles; `xref.update` leaves them alone so a cross-reference update never renumbers them. Convert Text to Table splits the selected paragraphs by tab, comma or semicolon; Convert Table to Text joins cells with tabs and rows with paragraph breaks.

### formdes-render · tagged PDF and the standards
- refers-to: native/formdes-render/src/pdf.rs :: PdfOpts, configuration, TagTree; scene.rs :: Op::Tag, heading_level
- kind: rationale
- logged: v0.19.0 · 2026-10-10

The scene marks paragraphs and pictures with `Op::Tag` (a style named Title is level 1, a heading style with a digit takes that level, any other heading style level 2), and the PDF writer builds krilla's tag tree from them, with alt text on figures and links. krilla validates PDF/A-2a, 2b, 2u, 3b, 4 and PDF/UA-1; it has no PDF/X, so an X standard exports a plain PDF with a warning and the window no longer offers one. PDF/A needs an output intent, so CMYK goes out with the Color Settings profile or is converted to RGB when there is none; PDF/UA needs a title, an outline (the bookmarks, or the headings when there are none) and alt text on every link.

### formdes-interchange · tables from CSV and Excel
- refers-to: native/formdes-interchange/src/text.rs :: table_of, csv_rows, xlsx
- kind: explainer
- logged: v0.19.0 · 2026-10-10

A .csv or the first sheet of an .xlsx (shared strings, inline strings, and a formula's cached value; nothing is recalculated) places as a table with a header row; the engine scales its column widths to the frame column. `csv_as_tabs` keeps the old tab-separated paragraphs for callers that want text.

## Package fonts and autocorrect (0.23.0) {#parity0230}

### file.rs · Package copies fonts
- refers-to: native/formdes-engine/src/cmd/file.rs :: package, fonts_used, font_restricted
- kind: rationale
- logged: v0.23.0 · 2026-10-12

Package writes each used face into `Document fonts/` from the bytes the renderer's FontBook already holds (the face that actually drew the text, so the packaged file is the right one), named by its PostScript name with the extension its header implies (OTTO, ttcf, else ttf). InDesign skips fonts whose licence restricts embedding; so does this, by the OS/2 fsType low nibble being 2 (restricted licence), found by walking the table directory (inside a collection, from the face's own offset table). A face resolved to a different family is a substitute and is reported missing rather than copied, so a package never carries the wrong font under the right name. `fonts_used` now returns family and style pairs, since resolving needs both.

### input.rs and edit.rs · autocorrect
- refers-to: native/formdes-engine/src/input.rs :: text, autocorrect, match_case; native/formdes-engine/src/cmd/edit.rs :: autocorrect
- kind: rationale
- logged: v0.23.0 · 2026-10-12

Correction happens in the engine's typing path, not the window, so typing from any client (the window, MCP `text`, the CLI) behaves the same. It runs only when one separator character is typed (space, tab, return, closing punctuation and quotes) with a collapsed caret and `raw` off, reads the word before the caret in character offsets (the same offsets the selection uses), selects it and inserts the replacement through `text.insert`, so it joins the typing undo step and tracked changes record it like typing. An exact key wins; otherwise a lower-case key is matched and the typed word's case (Capitalised or ALL CAPS) is applied to the correction. Settings live in `Session.prefs.autocorrect` (session scope, set by `edit.autocorrect`); the window keeps the person's list and sends it on every engine connection.

## Decorations, list levels and parent options (0.24.0) {#parity0240}

### attrs.rs and scene.rs · underline and strikethrough options
- refers-to: native/formdes-model/src/attrs.rs :: Deco, CharProps.underline_style, strike_style; native/formdes-render/src/scene.rs :: decorations in the run painter
- kind: rationale
- logged: v0.24.0 · 2026-10-12

InDesign's Underline Options and Strikethrough Options set weight, offset and colour with tint. The model keeps them as optional keys so existing documents read unchanged: an empty weight or offset falls back to the face's own underline and strikeout metrics scaled to the run size, an empty colour to the run's fill brush. The renderer now draws a decoration even when the text itself has no fill (InDesign allows an underline under paper-coloured text). Offsets are entered as positive distances (below the baseline for underline, above for strike) and signed in the renderer. IDML does not carry the new keys yet.

### flow.rs and para.rs · list levels
- refers-to: native/formdes-text/src/flow.rs :: list counters; native/formdes-text/src/para.rs :: ListNo, build
- kind: rationale
- logged: v0.24.0 · 2026-10-12

Counters are keyed by list (its name, or the paragraph style when unnamed) and level; numbering a paragraph at level L clears the counters of every deeper level, which is InDesign's restart-after-higher-level default, and collects the current numbers of the levels above for `^1` to `^9`. `build` takes a `ListNo` (number and ancestors) instead of a bare number; callers that never numbered still pass None.

### layout.rs · Parent Page Options
- refers-to: native/formdes-engine/src/cmd/layout.rs :: parents_options
- kind: rationale
- logged: v0.24.0 · 2026-10-12

Based-on parents were already in the model, the layout and the renderer; what was missing was a way to set one after creation. The cycle check walks the proposed base's chain (16 steps at most) and refuses any chain that reaches the parent being edited.

## Effects, mixed inks, marks and separations (0.25.0) {#parity0250}

### scene.rs · drop shadow blur and gradient feather
- refers-to: native/formdes-render/src/scene.rs :: shadow, feather
- kind: rationale
- logged: v0.25.0 · 2026-10-12

Drop Shadow Size is the blur: a luminance mask of vector bands (white outside the edge, black inside) over one flat fill, so the PDF keeps a vector soft mask; Spread moves the 50 percent point outward. Gradient Feather is a mask whose fill is a grey gradient of the opacity stops (linear by angle over the bounding box, or radial), drawn through the same Op::PushMask as Basic Feather.

### color.rs and doc.rs · mixed inks and plates
- refers-to: native/formdes-model/src/color.rs :: Color::Ink, SwatchValue::Mixed; native/formdes-model/src/doc.rs :: mix_eval, swatch_ink_color, plate_names, plate_density; native/formdes-render/src/sep.rs
- kind: rationale
- logged: v0.25.0 · 2026-10-12

A spot or mixed ink swatch reaches the renderer as `Color::Ink` (swatch index, tint, display RGB, process equivalent) so plates can split it into its inks; it is never stored in a document. A mixed ink shows as the product of its components' transmittances and its process equivalent is their union per channel. krilla 0.8 writes Separation colour spaces but not DeviceN, so a mixed ink goes into the PDF as its process CMYK equivalent. Plates are `Doc::plate_names` (C, M, Y, K, then spots that neither print as process nor alias another); Separations Preview, the ink limit, file.exportSeparations and file.print with separations all use `Doc::plate_density`.

### marks.rs · printer's marks
- refers-to: native/formdes-render/src/marks.rs :: Marks; native/formdes-render/src/pdf.rs :: export
- kind: rationale
- logged: v0.25.0 · 2026-10-12

Marks are vector ops in page coordinates drawn in a slug (offset raised to the bleed, plus 20 points) that enlarges the PDF media box; TrimBox and BleedBox stay on the page and bleed. A plate PDF or PNG recolours scene and marks together, so registration colour lands at full density on every plate.

## Variable fonts, clipping paths and placed layers (0.26.0) {#parity0260}

### fonts.rs, shape.rs, outline.rs · variable font instances
- refers-to: native/formdes-text/src/fonts.rs :: Face.variations, Face.location, FontBook::varied, instance_face; native/formdes-model/src/attrs.rs :: fontVariations; native/formdes-engine/src/cmd/prod.rs :: font_axes
- kind: rationale
- logged: v0.26.0 · 2026-10-12

A variable font stays one file; each distinct set of axis values is its own `Face` (new id, same bytes, `root` pointing at the file's default face) cached by `FontBook::varied`, so shaping (harfrust `ShaperInstance`), advances and metrics (skrifa `Location`), glyph outlines (keyed by face id) and the PDF font cache (keyed by face id) all follow without a variation parameter anywhere else. A named instance that matches `fontStyle` resolves to the instance's coordinates (`styles_of` lists them too); the char attribute `fontVariations` ({tag: value}, clamped to each axis range) is applied on top. `type.fontAxes` is the query for the Character panel: axes (hidden ones left out), named instances and the current value of each axis. PDF: krilla 0.8 embeds a variable font at a location through `Font::new_variable`, which writes a static subset with the instance baked in (a TrueType subset with ToUnicode, so text stays extractable); nothing is drawn as outlines. Automatic optical size is not applied (opsz stays at the style's value until set). The bundled check font is the system Lora variable (wght 400 to 700 only), so multi-axis behaviour is by construction, not by test.

### clippath.rs, psd.rs · clipping paths
- refers-to: native/formdes-render/src/clippath.rs :: from_decoded, invert_path; native/formdes-render/src/psd.rs :: photoshop_paths; native/formdes-engine/src/cmd/imaging.rs :: clipping_path, clip_to_frame, wrap_contour; native/formdes-model/src/item.rs :: Item::image_path, contour_path
- kind: rationale
- logged: v0.26.0 · 2026-10-12

A clipping path is `item.extra.clipping` = {type, options, d} where `d` is an SVG path in the picture's natural point space (the same space the graphic's content transform places), so the scene clips with `xf * content * path` (one more `Op::PushClip` inside the frame clip) and the PDF writer's existing clip handles it. Detect Edges treats the background as the most common colour on the picture's border (or alpha when the picture has any transparency) and counts a pixel as picture when it is more than `threshold` away from it; outside pixels are found by flood fill from the border, so without Include Inside Edges interior holes are filled; `inset` is a signed distance (Euclidean distance transform on a field of at most 1100 pixels), `tolerance` drives Douglas-Peucker and speckle removal, and the outline is marching squares on the coverage field. Invert adds the picture rectangle wound opposite to the contours so the non-zero fill rule clips the inside out. Photoshop paths are read from JPEG APP13 "Photoshop 3.0" 8BIM blocks, the TIFF tag 34377 and a PSD's image resources (ids 2000 to 2997, clipping path name 2999); path records are 8.24 fixed point, vertical first. Alpha Channel uses transparency, or a PSD alpha channel by name or index from the merged image data. Text wrap reads `item.extra.wrapContour` ({type: graphicFrame | sameAsClipping | detectEdges | alphaChannel | photoshopPath, ...}) through `Item::contour_path` in `exclusions`. Convert Clipping Path to Frame turns the path into the frame's `Shape::Path` (the content stays put). A clipping path is not recomputed when the link updates.

### pdflayers.rs, psd.rs · Object Layer Options
- refers-to: native/formdes-render/src/pdflayers.rs :: layers, apply, for_export; native/formdes-render/src/psd.rs :: Psd::composite; native/formdes-engine/src/cmd/imaging.rs :: apply_layers, relinked
- kind: rationale
- logged: v0.26.0 · 2026-10-12

hayro already honours a PDF's default optional content state when it draws (`OcgState` from `/OCProperties /D`), but it has no way to override it, and krilla's page embedding drops OCGs and copies every marked-content section, so a layer that is off by default would have appeared in an exported PDF. `pdflayers::apply` therefore rewrites the page's content: it tokenizes the decoded content stream, drops `/OC /Name BDC ... EMC` sections (and `Do` of XObjects with an `/OC` entry) whose group is hidden, turns the shown ones into plain `/Layer BMC` so no consumer consults the catalog again, and appends an incremental update (new content stream, the page object with `/Contents` replaced, a classic xref section with `/Prev`) that hayro reads on screen and krilla embeds. Optional content groups used inside Form XObjects are not filtered. `for_export` applies the file's own default state at the one place the PDF writer embeds a placed PDF. The override lives on the item as `extra.layers` = {source, view, overrides: {id: bool}, update: keep | file}; `view` is a derived asset (the patched PDF, or for a PSD the PNG composite) that the scene substitutes for the graphic's asset, so no `Op` changed; Links lists skip view assets. PSD layers come from `psd::Psd` (8-bit RGB or grey, raw and RLE channels, layer groups from `lsct`, hidden groups hide their children, normal blend and opacity only; zip-compressed channels and other blend modes are not composited); with no overrides the file's own merged image is used. PSD placing is new: header size and resolution come from the file, the picture decodes to its composite. Link updates call `relinked`: keep rebuilds the view from the new file with the same overrides, use file's drops them.

### formdes-model corners.rs, cmd/paths.rs · corners at anchor points
- refers-to: native/formdes-model/src/corners.rs :: round_vertices, anchor_count, CornerAt; native/formdes-model/src/item.rs :: Corner.each, Shape::to_path; native/formdes-engine/src/cmd/paths.rs :: round_corners, expand_corners; native/formdes-engine/src/tools/geo.rs :: editable_anchors, set_anchors
- kind: rationale
- logged: v0.26.0 · 2026-10-16

Designed from Illustrator's live corners and InDesign's Corner Options as Adobe documents them (helpx live-shapes and apply-corner-effects pages): a corner per anchor point, kinds round, inverted round, chamfer and inset (InDesign's fancy draws as round). `Corner.each` holds one entry per anchor in the order `geo::anchors_of` numbers them (subpath by subpath; a closed subpath's repeated end point is not an anchor), so Direct Selection's picked anchors map straight onto it. The shape keeps its base geometry and the rounding is applied when the path is built, so it stays live: moving anchors keeps the corners while the anchor count is unchanged; adding or deleting an anchor clears them. Each joint trims its two segments by arc length (kurbo's inverse arclength), so a corner beside a curve meets the curve exactly, and the trim is capped at half of each segment so neighbouring corners cannot cross. Round uses the circular-arc cubic for the turning angle; inverted round is an arc centred on the anchor; inset is the square notch. A uniform Corner Options radius now rounds polygons, stars and paths too (it used to apply to rectangles only), which is what InDesign does. Expand Corners bakes the geometry and clears the corner, the same as the other path commands that rebuild a shape.

### cmd/art.rs · Offset Path and Outline Stroke
- refers-to: native/formdes-engine/src/cmd/art.rs :: offset_path, outline_stroke, outline_of, stroke_style, seam_mid
- kind: rationale
- logged: v0.26.0 · 2026-10-16

From Illustrator's Object > Path > Offset Path and Outline Stroke as Adobe documents them. Both stroke the path with kurbo's stroker (caps, joins, miter limit, dashes) and clean the outline with i_overlay, the crate Pathfinder already uses. An offset is the shape united with a stroke twice the distance wide, or minus it for a negative offset; an open path gets the closed outline. Offset makes a copy above the original by default (`copy: false` changes the shape in place). Outline Stroke puts the stroke's outline in a new shape above the object when the object keeps something (a fill, text or a picture) and removes the stroke from the original; a plain unfilled path is replaced. The stroker drew a wrong join where a closed subpath starts (a sliver at a star's first tip, a rectangle's top edge offset unevenly), so `seam_mid` restarts every closed subpath halfway along its first segment before stroking, putting the seam where no join is drawn. Results are flattened to straight segments at 0.05 pt, as Pathfinder's are.

### cmd/art.rs, cmd/paths.rs, tools · Pathfinder pieces, Simplify, Smooth, Average, the corner widget
- refers-to: native/formdes-engine/src/cmd/art.rs :: pathfinder_pieces; native/formdes-engine/src/cmd/paths.rs :: simplify, smooth, average; native/formdes-engine/src/tools/geo.rs :: corner_handles, radius_for, apply_corners; native/formdes-engine/src/tools/select.rs :: direct (G::Corner); native/formdes-engine/src/overlays.rs (corners)
- kind: rationale
- logged: v0.27.0 · 2026-10-16

From Illustrator's Pathfinder panel, Object > Path and live corners as Adobe documents them. Divide builds the arrangement incrementally: each new shape splits every existing piece into its inside and outside part and adds whatever of it no piece covered, so each face's owner is the topmost shape over it and takes that shape's fill. Trim keeps what of each shape is not covered by the shapes above it; Merge then unites trimmed pieces whose fills are equal; Crop intersects every shape with the front one and drops the front one; Outline is Divide with each face's fill moved to a 0.5 pt stroke. Every piece is a copy of its owner, so styles and layers follow; strokes are removed except by Crop and Outline. Simplify fits Béziers with kurbo's simplifier (angle threshold from the corner angle) and keeps the original when the fit would not reduce the anchor count; Straight Lines uses the Pencil's RDP. Smooth gives picked (or all) corner anchors handles along the neighbour chord, a sixth of it. Average moves picked (or all) anchors to their mean in spread coordinates, horizontal keeping x and vertical keeping y. Corner handles sit on each corner's arc midpoint (r / sin(half interior) minus r from the vertex along the bisector), at least 10 pt and 14 screen pixels inside; dragging inverts that distance into a radius applied to the dragged corner, or to every picked corner when the dragged one is picked, previewed from the saved document and committed as one undo step; Alt-click without moving cycles round, inverted round, chamfer.

### cmd/draw.rs · Spiral, Arc, grids, Reflect, Shear, Transform Each
- refers-to: native/formdes-engine/src/cmd/draw.rs :: spiral, arc, grid, polar_grid, reflect, shear, each
- kind: rationale
- logged: v0.27.0 · 2026-10-16

From Illustrator's Spiral, Arc, Rectangular Grid and Polar Grid tools and Object > Transform as Adobe documents them, as commands (their options dialogs come from the signatures). A spiral is quarter-circle cubics (the 0.5523 circle constant), each radius the decay of the one before, its centre walked so consecutive quarters meet tangentially. An arc bows from a to b toward the corner chosen by the slope's sign, the control arms scaled by the slope; closed adds the two straight sides to the opposite corner. Grids are one open path each (lines, or rings and spokes), so a single stroke edit restyles them. Reflect mirrors across an axis through the selection's centre at any angle (rotate, flip, rotate back); Shear is an x or y skew about the centre; Transform Each applies scale, rotation, reflection and a move about each object's own centre. All three can work on copies.

### formdes-render scene.rs · miter limit, arrowheads, aligned dashes
- refers-to: native/formdes-render/src/scene.rs :: stroke, join_parts, aligned_dashes, path_end, arrowhead; native/formdes-render/src/raster.rs :: stroke_of; native/formdes-render/src/pdf.rs :: kstroke; native/formdes-engine/src/cmd/prod.rs (SVG); native/formdes-model/src/item.rs :: Stroke.miter, start_scale, end_scale, align_dashes
- kind: rationale
- logged: v0.28.0 · 2026-10-16

From InDesign's and Illustrator's Stroke panels as Adobe documents them. The renderer builds a stroke style in fifteen places, and only the item stroke has a person's miter limit, so the limit travels inside the join value ("miter 10") from that one site, and `join_parts` splits it where a style becomes a raster stroke, a PDF stroke or an SVG attribute; every other style keeps the plain join and the default of 4. Arrowheads were stored on strokes but never drawn: they are now drawn at the ends of open paths along the end tangent, sized by the stroke width times the start or end scale, filled in the stroke's paint (triangle, wide triangle, barbed, curved, solid circle and square) or stroked (simple, wide simple, circle, square, bar); the path is not shortened under them. Aligned dashes scale the pattern so a closed path holds a whole number of periods and an open path ends on a dash; Illustrator aligns per segment between corners, so this is the whole-path approximation.

### tools/draw.rs, cmd/art.rs · Curvature tool and Knife
- refers-to: native/formdes-engine/src/tools/draw.rs :: curvature, resmooth; native/formdes-engine/src/tools/mod.rs :: Pen.corners; native/formdes-engine/src/cmd/art.rs :: knife
- kind: rationale
- logged: v0.28.0 · 2026-10-16

From Illustrator's Curvature and Knife tools as Adobe documents them. The Curvature tool shares the Pen's in-progress anchors and its finish rules (Enter or Escape ends an open path, clicking the first point closes it, changing tool or spread ends it); every click re-smooths all interior points with the Pencil's chord-based handles, so the curve changes shape as points are added, and Alt-click marks a corner kept without handles. Knife subtracts a 0.05 pt slit stroked along the given polyline from each crossed closed shape and turns every resulting piece into a copy of the original; it is a command with a points list (agents and the dialog) rather than a freehand canvas tool for now.

### table cells · fill opacity
- refers-to: native/formdes-engine/src/cmd/table.rs :: set_cell; native/formdes-render/src/scene.rs (cell fills)
- kind: rationale
- logged: v0.28.0 · 2026-10-16

From the InDesign UserVoice request "Opacity of a cell fill" (106 votes). Cell Options passes `fillOpacity` (0 to 100) into the cell's attributes with its other pass-through keys, so cell styles carry it the same way, and the renderer uses it as the solid brush's alpha; alternating row and column fills take it from the cell as well.

### path.shapeBuilder · regions under points
- refers-to: native/formdes-engine/src/cmd/art.rs :: shape_builder, inside
- kind: rationale
- logged: v0.29.0 · 2026-10-16

From Illustrator's Shape Builder as Adobe documents it: a drag across regions merges them, Alt erases them, and the merged shape takes the style of the region where the drag began. The command takes the drag as a list of points. The regions are Divide's arrangement in paint order (each overlap owned by the upper shape), split into single faces; a face is hit when any point falls inside it by the even-odd test; hit faces are unioned (or dropped with mode erase) and every other face stays its own shape. The merged shape copies the owner of the face under the first point, which is what makes it red rather than blue in the test where the drag starts in the lower square.

### object.blend · steps between two objects
- refers-to: native/formdes-engine/src/cmd/draw.rs :: blend, mix, mix_paint
- kind: rationale
- logged: v0.29.0 · 2026-10-16

From Illustrator's Blend (Specified Steps). Steps are copies of the first object at t = k / (steps + 1). When both paths have the same subpath and anchor counts the anchors and handles interpolate in world space (a true morph); otherwise the first object's outline moves and scales between the two bounding boxes, so a square to circle blend steps as squares (Illustrator resamples to match anchors; that is open). Colours mix in CMYK when both are CMYK, in gray when both are gray, otherwise in RGB; stroke weight and opacity interpolate. The steps are spliced into the stacking list between the two objects, nearest-to-top last, as Illustrator stacks a blend; the blend is not live (no spine, no editing the ends) and is undone as one step.

### object.blend · unlike shapes resampled (0.30.0)
- refers-to: native/formdes-engine/src/cmd/draw.rs :: cubics, fractions, cut_at, matched
- kind: rationale
- logged: v0.30.0 · 2026-10-16

When the anchor structures differ but both are single subpaths, both open or both closed, each becomes a list of cubics. Closed ones are first given the same winding (signed area) and the second is restarted at its point nearest the first's start, mapped through the two bounding boxes (a square's top-left corner meets a circle at its upper-left diagonal, not its top). Each is then cut at the other's anchor arc-length fractions, so both have the same segments at the same fractions and corners land at the same fraction on the other shape; the steps interpolate the cubic control points. Splitting the longest segment until the counts matched (the first try) paired a square's sides with uneven arcs and the steps came out twisted. Several subpaths fall back to moving and scaling.

### widthProfile · variable-width strokes
- refers-to: native/formdes-model/src/widths.rs; native/formdes-render/src/scene.rs :: stroke; native/formdes-engine/src/cmd/draw.rs :: width_profile; native/formdes-engine/src/cmd/art.rs :: outline_stroke
- kind: rationale
- logged: v0.30.0 · 2026-10-16

From Illustrator's Width tool and width profiles. The profile lives in the item's `extra.widthProfile` (a preset name, or [at, left, right] entries with the sides as multiples of the stroke weight) so files without it are unchanged and no StrokeStyle literal had to change. The renderer draws such a stroke as a filled outline: each subpath flattened (tolerance 1 percent of the weight), widths interpolated by arc length with a smoothstep between entries, normals averaged at vertices with a miter factor capped at 4, open subpaths closed into one polygon, closed ones as an outer and an inner ring filled even-odd. Because it is a fill, raster, PDF and SVG all draw it with no exporter changes; dashes, caps, joins and stroke alignment do not apply to a width-profiled stroke. A flattened subpath whose last point repeats the first (Ellipse paths carry no ClosePath) drops the repeat, which removed a spike at the seam. Outline Stroke turns a profiled stroke into the same outline, filled even-odd. The canvas Width tool (dragging width points) is not built yet; the command sets profiles.

### effects · live distort and transform effects
- refers-to: native/formdes-model/src/effects.rs; native/formdes-render/src/scene.rs :: emit; native/formdes-engine/src/cmd/draw.rs :: effect, expand_appearance
- kind: rationale
- logged: v0.31.0 · 2026-10-16

From Illustrator's Effect > Distort & Transform and Stylize > Round Corners. The stack lives in the item's `extra.effects` as an ordered list of `{kind, ...params, on?}` so the shape itself never changes and every value stays editable; the renderer applies the list to the local path before fill, stroke, shadows and width profiles, so every exporter draws the result. Round Corners reuses corners.rs; Zig Zag puts ridges + 1 alternating normal offsets on each segment (corner or smooth through Catmull-Rom); Roughen displaces points spread at `detail` per inch by a seeded hash so a document renders the same every time; Pucker & Bloat moves anchors toward the centre and handles away by half the amount (negative gives Illustrator's spikes); Twist rotates densely sampled points by the angle scaled down linearly with distance from the centre; Transform repeats a scale, rotate and move about the centre `copies` times, adding each copy to the path. Expand Appearance writes the applied path into the shape and empties the stack. Hit testing, bounds and text wrap still use the plain shape (Illustrator's visual bounds include effects); that is open.

### appearance · extra fills and strokes
- refers-to: native/formdes-render/src/scene.rs :: appearance; native/formdes-engine/src/cmd/draw.rs :: appearance, expand_appearance
- kind: rationale
- logged: v0.31.0 · 2026-10-16

From Illustrator's Appearance panel, where an object carries several fills and strokes, each with its own opacity and effects. The main fill and stroke stay where they are (every other tool and exporter already knows them); `extra.appearance` adds entries `{kind: fill|stroke, paint (the serialized Paint), opacity, weight, cap, join, dash, effects, above, on}`. Entries without `above` draw before the main fill, in list order, which is where the common wide under-stroke goes; entries with `above` draw after the main stroke. Each entry's effects apply after the object's own effect stack. Expand Appearance makes one plain object per entry (the stroke kept as a stroke), splices them under or over the original in the stacking list, groups them with it, and then bakes every remaining effect stack in the group. A stroke entry has no width profile or arrowheads; those stay on the main stroke.

### masks · opacity masks, clipping masks, isolated blending
- refers-to: native/formdes-render/src/scene.rs :: emit (group branch); native/formdes-engine/src/cmd/draw.rs :: mask_group, opacity_mask, clipping_mask, isolate_blending
- kind: rationale
- logged: v0.32.0 · 2026-10-16

From Illustrator's Make Opacity Mask, Make Clipping Mask (and Draw Inside, which makes one) and Isolate Blending. All three are group options in the group's extra (`opacityMask` and `clipMask` hold the mask child's id, `maskClip`, `isolate`) so ungrouping, saving and the other tools need nothing new. Making one groups the selection in paint order and takes the topmost object as the mask. The renderer skips the mask child when drawing the children: a clip mask becomes a PushClip of its path (with its effects), so its own fill and stroke never draw; an opacity mask is drawn into its own op list and pushed as the existing luminance PushMask that raster and PDF already support (built for feathers and glows), with a white page-sized fill under it when clip is off so what lies outside the mask stays visible. Isolation is a PushOpacity(1.0) layer, which both backends turn into a transparency group. Release removes the option and leaves the group, so the mask becomes an ordinary visible child. Invert mask and editing the mask in place in the window are open.

### freeform · freeform gradients (points)
- refers-to: native/formdes-render/src/scene.rs :: freeform; native/formdes-engine/src/cmd/draw.rs :: freeform_gradient
- kind: rationale
- logged: v0.32.0 · 2026-10-16

From Illustrator's freeform gradient in Points mode. The command resolves each point's colour (a swatch or #rrggbb) to RGB once and stores `extra.freeform` as [u, v, r, g, b] in the object's bounds, so a later swatch edit does not change it (Illustrator's freeform stops are fixed colours too). The renderer replaces the ordinary fill with a grid of solid cells clipped to the path: inverse-distance weighting with power `freeformSpread` (default 2), a cell size of about 3 device points capped at 96 cells a side, each cell grown by 30 percent on every side so antialiased edges overlap and no seams show. Plain fills keep raster, PDF and SVG output identical without a mesh-shading writer; the cost is size (up to 9,216 small fills per object in PDF). A PDF type 6 or 7 mesh shading is the better encoding and is open, as are Lines mode and on-canvas point editing.

### warp · Effect > Warp styles
- refers-to: native/formdes-model/src/effects.rs :: warp, dense, WARPS
- kind: rationale
- logged: v0.33.0 · 2026-10-16

From Illustrator's Effect > Warp (and Object > Envelope Distort > Make with Warp, which uses the same styles). Thirteen of the fifteen styles as one `warp` kind in the effect stack, with `bend` -100..100 and `horizontal`; fish and twist are left out (Twist already exists as its own effect). The path is sampled every 2 points and each point mapped in the bounds normalised to -1..1: Arc is a true polar bend (the width becomes an arc of bend times 180 degrees), Arch, Arc Lower and Arc Upper move points vertically by a parabola weighted by row, Bulge and the Shells scale distance from the centre line, Flag and Wave move by a sine of the column, Rise shears, Fisheye scales by the distance from the centre normalised so the corners sit at 1 (the plain unit circle missed every edge of a rectangle), Inflate and Squeeze scale both or one axis. Vertical mode swaps the axes before and after. The result is a polyline, so Expand Appearance gives many points; fitting curves back (Simplify) is the follow-up. Warping applies per object with its own bounds; a warp on a group as one envelope is open.

### repeat · Repeat radial, grid, mirror
- refers-to: native/formdes-render/src/scene.rs :: repeat_xfs, emit (group branch); native/formdes-engine/src/cmd/draw.rs :: repeat
- kind: rationale
- logged: v0.33.0 · 2026-10-16

From Illustrator's Object > Repeat. A Repeat is a group whose `extra.repeat` holds the kind and its values; the renderer draws the group's children once per placement from `repeat_xfs`, composing each placement in spread space before the children's own transforms, so editing the original changes every copy at once. Radial turns `count` copies about a centre `radius` below the art (Illustrator's default puts the art at the top of the ring); Grid steps by the art's size plus the gaps with optional mirroring of alternate rows or columns; Mirror adds a reflection across an axis at `angle` through a point `gap / 2` beyond the right edge. One object is grouped by grouping it with a temporary duplicate and removing the duplicate, since group() refuses a single item. Expand writes the copies as real children with the placement folded into their transforms through the group's world transform. Selecting and hit testing see only the original; on-canvas handles for the ring and grid are open.

### symbols · masters and instances
- refers-to: native/formdes-engine/src/cmd/symbol.rs; native/formdes-render/src/scene.rs :: symbol_master, emit (instance), Ctx.depth
- kind: rationale
- logged: v0.34.0 · 2026-10-16

From Illustrator's Symbols panel (New Symbol, place, Break Link, Redefine, delete). The document keeps `prefs.symbols` as name to master id; a master is the selection (grouped when it is several objects) detached from its spread with its world transform folded into its own, so it stays in `doc.items`, saves with the document and draws nowhere by itself (only snippet export prunes unplaced items, and it never sees masters). An instance is a Plain item with no paint whose rectangle is the master's bounds in the master's own coordinates and whose transform places it; the renderer draws the master through the instance's transform, so moving, scaling and rotating an instance carry the art, and a redefinition shows in every instance at once. Redefine centres the new art on the old one and refits every instance's rectangle; it refuses art that contains an instance of the same symbol, and the renderer stops at depth 6 for any other loop. Break Link duplicates the master into the instance's home with the instance's transform composed in. Not yet: effects and appearance on an instance apply to its invisible rectangle rather than to the drawn art, the Symbols panel in the window, symbol sprayer tools, 9-slice scaling.

### object.recolor · Recolor Artwork
- refers-to: native/formdes-engine/src/cmd/draw.rs :: recolor, hsl_adjust
- kind: rationale
- logged: v0.34.0 · 2026-10-16

From Illustrator's Recolor Artwork, reduced to its two common uses: swapping named colours and shifting the whole palette. Every object under the selection (groups walked) has its fill, stroke, extra appearance paints and freeform points resolved to RGB; a colour within `tolerance` percent per channel of a map entry's `from` takes the `to` paint as given, so mapping to a swatch name keeps the object linked to that swatch; otherwise, when a hue, saturation or brightness change is set, the colour moves through HSL and is written as a plain RGB colour (a swatch link is lost, as with Illustrator's recolour of unlinked colours). The plan is built from a read-only pass and applied in one undo step. Gradient paints and gradient swatches are skipped (no single colour to resolve); harmony rules, the colour wheel and recolouring placed images are open.

### brushes · calligraphic and scatter
- refers-to: native/formdes-model/src/widths.rs :: nib, outline_by; native/formdes-render/src/scene.rs :: stroke (brush), along; native/formdes-engine/src/cmd/draw.rs :: brush; native/formdes-engine/src/cmd/art.rs :: outline_stroke
- kind: rationale
- logged: v0.35.0 · 2026-10-16

From Illustrator's Calligraphic and Scatter brushes. `extra.brush` holds the kind and its values. The width-profile outliner became `outline_by`, which asks a closure for each point's two half widths given its position and normal; profiles use it with the profile and the nib with the ellipse's extent along the normal (size across at `angle`, the other axis `roundness` percent), so a 45 degree flat nib draws thick downstrokes and thin crossings like a pen. Scatter draws a symbol's master at every `spacing` along the device-space path (flattened at 0.1, restarted per subpath, capped at 5,000 copies), centred on the path, scaled by `scale` and the view, turned to the path's direction unless `rotate` is false; it reuses the symbol depth guard. Applying a brush gives an unstroked object a 1 pt black stroke, because the stroke paints calligraphic strokes and gates drawing. Outline Stroke turns a calligraphic stroke into its filled outline (even-odd) and drops the brush. Pattern brushes, pressure, random size and spacing for scatter, and Expand Appearance for scatter are open.

Art brushes (`kind: art`, same release) stretch a symbol's art along the first subpath of the stroke: `Spine` flattens that subpath with its running length, and every non-group piece of the master is flattened, resampled at about 1/200 of the art's width (the closing edge too, so closed pieces stay bent), and mapped so x across the art's bounds becomes distance along the spine and y from the art's centre line becomes offset along the normal, times `width` percent and the view scale (`flip` mirrors it across the path). Each piece is drawn with its own fill and stroke, so a multi-colour symbol keeps its colours; gradients are laid over the bent bounds rather than bent.

### object.imageTrace · Image Trace
- refers-to: native/formdes-engine/src/cmd/trace.rs
- kind: rationale
- logged: v0.36.0 · 2026-10-16

From Illustrator's Image Trace (Black and White, and Limited Colors) followed by Expand. The placed image is decoded with the renderer's decoder (PDF and vector placements are refused), box-averaged down to `detail` pixels on the long side, and turned into masks: brightness under `threshold`, or one mask per colour of a k-means palette (12 rounds, seeded farthest-point first from a 4,000-pixel sample; seeding from evenly spaced pixels picked several near-whites on a mostly white image and fused the red and blue regions into one purple). Each mask's outline is followed along pixel edges into closed loops directed so holes come out reversed (nonzero fill works); loops under `noise` pixels of area are dropped; each loop is moved to its edge midpoints, smoothed twice with a 1-2-1 kernel, split at its farthest point (a closed polyline gives the simplifier a zero-length chord, which first collapsed every loop to a point), simplified by Ramer-Douglas-Peucker at `paths` pixels and closed with Catmull-Rom curves. Shapes are copies of the image frame turned into plain paths in the frame's local space through the image's content transform, stacked light to dark, grouped, with a clipping mask of the frame's shape when the image was cropped by it; the image is removed unless `keep`. Not yet: abutting rather than stacked colour regions, grayscale and photo presets, live (re-editable) tracing, outlines-only mode, colour by an existing swatch group.

### path.livePaint · Live Paint Bucket
- refers-to: native/formdes-engine/src/cmd/art.rs :: live_paint, faces_of
- kind: rationale
- logged: v0.36.0 · 2026-10-16

From Illustrator's Live Paint Bucket, where the regions between overlapping shapes and crossing lines can be filled without building the shapes first. Shape Builder's arrangement became `faces_of`, which can also subtract cuts; Live Paint passes the selection's open paths (lines, arcs, pen paths) as 0.05 pt slits, the same blade Knife uses, so a line crossing a shape divides its region. Each point's face becomes a new plain shape filled with the swatch, spliced into the stacking list just under the lowest selected object so the outlines stay visible above the fills. It is a one-time fill, not a live paint group: moving a line later does not reflow the fills. Gap detection, painting edges (Live Paint's stroke bucket) and a live group are open.

### envelope · Envelope Distort with Top Object and Mesh
- refers-to: native/formdes-model/src/effects.rs :: envelope, coons_grid, envelope_of; native/formdes-render/src/scene.rs :: emit (envelope); native/formdes-engine/src/cmd/draw.rs :: envelope
- kind: rationale
- logged: v0.37.0 · 2026-10-16

From Illustrator's Object > Envelope Distort (Make with Top Object, Make with Mesh, Release, Expand). Every leaf object under the selection gets the same `extra.envelope`: the frame (the art's combined bounds in spread coordinates), the grid size and the warped grid points in spread coordinates, so a group of separate objects bends as one piece; the renderer maps the object's path (after its effects) to spread space, moves every densely sampled point by bilinear interpolation in its grid cell, and maps it back. Top Object builds the grid as a Coons patch over the top shape's outline, its four sides running between the outline points nearest the corners of its bounds (so an ellipse's sides meet at its diagonals, as in Illustrator), sampled 16 by 16, and consumes the top shape; Mesh starts with an even grid moved by `moves`, and `moves` on an existing envelope edits it. Release drops the envelope; expand writes the bent outline into each path. Because the frame is stored in spread coordinates, moving an object after enveloping leaves it outside its frame; moving the whole envelope with its art and a canvas editor for mesh points are open, as are bending text glyphs and images (only their frames bend).

Puppet Warp is a third envelope kind, `puppet`, from Illustrator's Puppet Warp tool: pins are [x, y, dx, dy] in spread points (a zero move holds the pin), and every point of a 16 by 16 grid over the art moves by the moving-least-squares rigid deformation of Schaefer, McPhail and Warren (2006), in complex form: weights 1 / distance squared, weighted centroids of the pins before and after, the best rotation normalised from the sum of conj(p) times q, then rotation about the centroid plus the moved centroid. One pin translates, two pins rotate rigidly about the still one, three or more bend the art between them. It shares the envelope's storage, so release, expand and moves edits apply; dragging pins on the canvas is open.

### character styles · several on the same text
- refers-to: native/formdes-model/src/style.rs :: CHAR_STACK, char_stack_names, char_stack; native/formdes-model/src/story.rs :: stack_char_style; native/formdes-text/src/para.rs (run style chain); native/formdes-engine/src/cmd/text.rs :: char_style_add, char_style_remove, style from selection; native/formdes-interchange/src/idml/write.rs :: para (character ranges)
- kind: rationale
- logged: v0.38.0 · 2026-10-16

From the InDesign UserVoice request to apply several character styles to the same text (503 votes, the fourth most voted). A run keeps one style name, so the stack is written into it bottom first, joined by " + " (`CHAR_STACK`); no model type, file field or existing caller changes, and a run with one style is exactly what it was. Everywhere a run's style is resolved goes through `char_stack_names`, which splits the name, so each style's chain is layered in order and a later style wins only for what it sets (the paragraph's style below, the run's own overrides above). The composer resolved the run's style with its own direct `chain(cs)` lookup, which found no style named "Emph + Hot" and drew stacked text plain until it was changed. Add moves a style already present to the top, Remove keeps the others, Apply still replaces the whole stack with one style. InDesign cannot stack, so IDML export writes the top style as applied and folds the lower styles into the range's local attributes, which keeps the look in InDesign. The window's Character Styles panel shows the combined name; a panel control to reorder the stack is open.

### underline and strikethrough · no colour drew nothing
- refers-to: native/formdes-model/src/attrs.rs :: Deco::resolve
- kind: fix
- logged: v0.38.0 · 2026-10-16

Found while checking stacked styles: no underline or strikethrough was drawn anywhere, from a style or set directly. `underlineColor` and `strikeColor` have no default, so the stack lookup returned null, `Paint::from_value(null)` is `Paint::None`, and the renderer drew the line with a paint that has no brush. A missing or None colour now means the text's own colour, which is what the renderer's fallback was written for.

### tables · rounded corners, alt text and summary
- refers-to: native/formdes-engine/src/cmd/table.rs :: options; native/formdes-render/src/scene.rs :: text (table cells); native/formdes-interchange/src/publish.rs (table HTML)
- kind: rationale
- logged: v0.39.0 · 2026-10-16

From the InDesign UserVoice requests for rounded table corners (233 votes) and alt text and summaries on tables (accessibility). Table Options takes `cornerRadius`, `alt` and `summary` into the table's own attributes (not table-style keys, so the style validator is untouched; a radius of 0 or less, or an empty string, clears them). The renderer clips the table's part in each frame to a rounded rectangle around that part's cells (the radius capped at half the smaller side), draws fills, edges, images and text inside the clip, then strokes the rounded outline in the table's border weight and colour, so the corner cells' square edges never show. A table split across frames rounds every part on all four corners; rounding only the outer corners of the first and last parts is open. HTML, EPUB and website exports write `alt` as the table's aria-label and `summary` as its caption; a Table structure element with the summary in tagged PDF is open.

### footnotes · stacked notes and document footnote options
- refers-to: native/formdes-text/src/flow.rs :: place_note, note placement at the frame bottom; native/formdes-render/src/scene.rs :: text (notes); native/formdes-text/src/para.rs :: Context::footnote_label; native/formdes-text/src/layout.rs :: footnote_number, footnote_label; native/formdes-engine/src/cmd/refs.rs :: footnote_options
- kind: fix and rationale
- logged: v0.39.0 · 2026-10-16

Fix: two or more footnotes in one column were all placed at the same top (the frame bottom minus the column's total note height), so they printed over each other; each note now starts where the one before it ended, and the renderer draws the separator rule once per column instead of above every note (once the notes no longer overlapped, the extra rules showed as underlines). Options, from InDesign's Document Footnote Options and the UserVoice request for more footnote options (801 votes): `prefs.footnotes` holds the numbering style (the page-number formats plus a symbol sequence * dagger double-dagger section parallel pilcrow, doubled after six), the first number, and the prefix, suffix and separator around the number where the note begins (the reference in the text stays the bare number); `footnote_label` joined the layout context so the note label and the reference number can differ. Restarting numbers per page or section, footnotes in their own column count or spanning the frame, and run-in notes need the paging pass to know the page of each reference and stay open.

### tables · rows that split across frames
- refers-to: native/formdes-text/src/flow.rs :: place_table (split_rows); native/formdes-engine/src/cmd/table.rs :: options (splitRows)
- kind: rationale
- logged: v0.40.0 · 2026-10-16

From the most-voted InDesign UserVoice request, "Split table rows across pages and frames" (1,021 votes). With the table's `splitRows` on, a row that does not fit the space left in the frame is split at a line of text when at least one line of its cells fits: each cell keeps the lines whose leading, descent and bottom inset fit the room, the first part is placed as a row cut at the frame's foot, the remaining lines replace the cell's lines with the row's height re-measured, and the loop places the same row again after the frame break, under repeated header rows, so a row may run over several frames. Without the option the earlier behaviour stands (a row moves whole, and a row taller than a frame is lost to overset), which is why it is an option and not a default. Rows covered by a cell spanning several rows and rows with rotated cells are never split. Cell fills and strokes are drawn per part, so a split row shows a closing edge at the frame foot and an opening edge in the next frame, as InDesign's own split rows do; a per-row option and keep-lines-together settings are open.

The same request asks for continued headers. The table's `continued` text is composed in Basic Paragraph and placed as a full-width row above the repeated header rows in every frame after the first (`continued_row`). It is recorded with row index usize::MAX, so table styles, alternating fills, cell edges and cell hit testing (celltext.rs skips it) never treat it as a real row, and it draws as a plain bordered box. A paragraph or cell style for the label and an automatic table number are open.

### markdown import · pipe tables
- refers-to: native/formdes-interchange/src/text.rs :: markdown, pipe_table, pipe_cells
- kind: fix
- logged: v0.41.0 · 2026-10-16

Found while testing split rows: Markdown pipe tables were placed as lines of text with their pipes. The importer now recognises a pipe row followed by a separator of dashes (colons and spaces allowed) and the pipe rows after it, and builds the table with the same `table_of` the CSV importer uses (first row as header when there are two or more rows). Bold and underscore markers inside cells are dropped rather than styled; column alignment from the colons is not applied yet.

### anchored objects · text wrap where the flow places them
- refers-to: native/formdes-text/src/layout.rs :: layout_story_at, anchored_wraps
- kind: rationale
- logged: v0.41.0 · 2026-10-16

From the InDesign UserVoice request to make text wrap work for anchored objects placed before their anchor (299 votes). Wrap exclusions are built before the flow from objects' stored positions and skip the story's own anchored objects, whose real position is only known once the flow places their anchors, so a custom-positioned anchored object with wrap pushed no text aside. `layout_story_at` now flows once, takes every custom-positioned anchored object with a wrap mode, sets its bounds at the anchor point in the frame's coordinates (the same corner the renderer uses) grown by its offsets, adds them to the frames' exclusions and flows again, up to three passes and stopping when the anchored positions stop moving (text before the anchor can reflow and move the anchor itself). Text both before and after the anchor wraps, which is what the request asks for. Contour wrap on anchored objects uses the bounds; inline and above-line anchored objects are unchanged; vertical frames are skipped.

