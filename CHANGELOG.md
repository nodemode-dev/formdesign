# Changelog

Every release of FormDesign (formdes until 0.19.0), newest first. Versions follow semver; tags read `v*.*.*`.

## 0.20.1 · 2026-10-12

- Folding or unfolding a panel column, or resizing the window, no longer paints an empty canvas frame, and the page stays centred where it was.
- The left panel column has a resizer on its right edge (double-click for the default width); its width is remembered and kept in workspaces.

## 0.20.0 · 2026-10-12

- The product is called FormDesign: window, splash, home, About, status bar and the desktop bundle. The `formdes` command, the `formdes` MCP server, `FORMDES_*` settings and `.fdes` files keep their names.
- A left panel column between the tools and the canvas, with Pages, Layers and Links and Make and Slots by default; drag any panel group into or out of it, per workspace.
- The left column folds into the tool rail as a minirail of icons, each opening its panels as a flyout; the Properties column and the dock fold into icon strips. Window > Collapse Left Panels to Minirail (Alt+Shift+Mod+L) and Collapse Right Panels to Icons (Shift+Tab). The folded state is remembered and kept in workspaces.
- Edit > Search (Mod+K): every command, panel and tool, ranked as you type, with its menu path and keys.
- Every drop-down in the window uses the themed list, so no system list with a scrollbar appears.

## 0.19.0 · 2026-10-10

- Every menu command that needs settings opens a dialog built from what the command accepts, with the document's styles, swatches, anchors, variables, fonts, inks and printers to pick from, and Parameters as JSON for the rest. Single commands no longer show as one-item submenus, and menus update which items are available as the selection changes.
- Type > Paragraph Options and Character Options: the composer and justification spacing, hyphenation zone and limits, keeps and start positions, drop caps, rules and shading, span and split columns, optical margin alignment, balanced ragged lines, direction, lists, nested and GREP styles. Type > OpenType, Change Case, Clear Overrides, Find/Replace Font, Load Styles, New Style Group and Define Lists.
- Drop caps, synthesised small caps, text inside ellipses and polygons, contour text wrap and Invert, balanced columns and table header rows that repeat across frames all compose and draw.
- Layout: Liquid Layout rules and object pins, Create Alternate Layout, Adjust layout in Margins and Columns, Add Page to Spread and Allow Selected Spread to Shuffle for gatefolds, Smart Text Reflow that adds pages as a story overflows (one Undo removes them with the edit).
- Colour: Window > Output > Ink Manager (spots to process, ink aliases), New Tint Swatch, New Gradient Swatch and New Color Group; View > Overprint Preview is back; Proof Colors proofs through the chosen ICC profile with paper simulation.
- Text and tables: typing inside table cells with Tab between cells; Convert Text to Table and Table to Text; Table and Cell Style Options; endnotes numbered through the document into an endnote frame; CSV and Excel files place as tables; Object > Select Container, Content and Next Object; note flags and tracked changes marked on the canvas again; Object > Effects > Transparency with blend modes.
- Output: tagged PDF with headings, reading order and alt text; PDF/A-2a, PDF/A-2b and PDF/UA-1; buttons as PDF links; File > Export > SVG; File > Print with a printer list through lp or lpr; Print Booklet downloads its PDF. PDF/X is no longer offered, because the PDF writer cannot produce it.
- Preferences > Agents has an agent access switch; with it off, every MCP tool call is refused.
- 311 commands. Parity against InDesign and Affinity re-traced: 104 of 133 rows (78 percent), up from 66 percent. Not yet: PDF/X, form fields, media and page transitions in exported PDF, graphic cells, and layer visibility for placed PDFs.

## 0.18.0 · 2026-10-10

- Copy, Cut, Paste, Paste without Formatting, Paste in Place and Paste Into (a copied picture into a frame), across documents, with the system clipboard's text; objects pasted into text become anchored objects. Step and Repeat, including grids.
- Guides: delete one or all on a spread, move, lock and recolour them, and Layout > Create Guides with rows, columns and gutters.
- Paths: Join, Open, Close and Reverse Path, Make and Release Compound Path (holes show), Convert Shape, and Pathfinder (Add, Subtract, Intersect, Exclude Overlap, Minus Back).
- Hyperlinks on text or objects to web and email addresses, pages and text anchors; PDF bookmarks; text variables you define and insert; footnotes inserted and edited; alt text; static captions; Fill with Placeholder Text; Rotate Spread.
- Layout > Table of Contents and Update Table of Contents, index page references and Generate Index, text anchors and cross-references with Update Cross-References, and data merge from CSV or tab-separated data into a new document, with pictures from file paths.
- Generate QR Code, Create Outlines, and anchored objects: anchor at the text cursor, options and release.
- 270 commands. Still to come: alternate and liquid layouts, ink manager, lists and endnotes, OpenType feature commands, and dialogs for the commands that open a raw parameters box today.

## 0.17.0 · 2026-10-10

- formdes now runs on its own layout engine. The layout core answers the window, the engine service, the templates, the Make pass and agents from `native/formdes-engine` and its model, text, render and interchange crates; the engine crates it used before are no longer used or built. All 71 templates build and render; a layout made from Markdown threads onto its continuation pages.
- The layout inside a `.fdes` is now formdes's own `.fdoc` file, and a bare layout saves and opens as `.fdoc`. Documents saved by earlier versions do not open; open their IDML export instead.
- Source Serif 4, Source Sans 3, Inter and JetBrains Mono and a US English spelling dictionary are built into the layout core.
- New Line (`line.create`) for rules in templates; a document whose start page number is even begins with a left page.
- The PDF Reader is gone: viewing and editing existing PDFs moves to FormPDF, a separate app. formdes still makes layouts from PDFs, places them and exports PDF.
- Not carried over yet: Overprint Preview, note and change markers and the highlight views on the canvas, profile-accurate soft proofing (the proof is an approximation), and typing inside table cells.

## 0.16.0 · 2026-10-10

- formdes's own engine has its toolbox: Selection, Direct Selection, Page, Gap, Content Collector and Placer, Type, Type on a Path, Line, Pen with its anchor tools, Pencil, Smooth, Erase, frame and shape tools, Scissors, Free Transform, Rotate, Scale, Shear, Gradient Swatch and Feather, Note, Color Theme, Eyedropper and Measure, each with InDesign's shortcut. A drag previews live and lands as one undo step; Escape cancels it.
- Click the out port of a text frame to load its text and click or drag to thread it on; the place gun fills a frame, places at natural size or into a dragged frame.
- Typing, caret and word movement, line-aware Up and Down, selection with Shift, Backspace and Delete respecting tracked changes, and nudging with the arrow keys; command and tool shortcuts are routed as the window expects.
- The engine draws the canvas furniture the window shows: margins, columns, guides, bleed, baseline grid, frame edges, parent items, selection outlines, handles, ports and threads, path anchors, text highlights and the caret, and live tool feedback.
- Still not wired into the app; the window keeps running on the engine crates it used before.

## 0.15.0 · 2026-10-10

- formdes's own engine has its command layer: `native/formdes-engine` holds open documents, the selection, undo and redo, and batches that run as one undo step with `$N` references between steps. It answers the 210 commands the window, the backend, the template compiler and agents send, from frames, objects, transforms, text, styles, swatches and tables to pages, parents, layers, links, review, XML, books, packaging and export, in the shapes they expect today.
- `native/formdes-interchange` reads IDML packages (pages, parents, layers, swatches, styles, frames, threaded stories, tables, footnotes, images) and writes them; places tagged text, plain text, Markdown, HTML, RTF, Word and CSV into frames; and exports HTML and EPUB, reflowable or fixed layout.
- The page drawing now numbers folios on parent pages per page, draws table cell fills, alternating rows and border weights, hides tracked deletions and hidden conditional text, honours hyphenation exceptions and gradient stop opacity.
- Still not wired into the app; the window keeps running on the engine crates it used before.

## 0.14.0 · 2026-10-10

- formdes's own engine can draw and export pages. `native/formdes-render` turns a page into one list of drawing operations and draws it to PNG or JPEG, or writes it as a PDF with embedded subset fonts, CMYK kept as CMYK, selectable and searchable text, trim and bleed boxes, placed PDFs as vector pages and working links. Layers, parent pages, gradients, dashed strokes, drop shadows, images clipped to their frames, paragraph rules and shading, underlines, tab leaders, drop caps, footnotes and tables are drawn.
- The text engine gained paragraphs that span or split columns, contour and inverted text wrap, text inside ellipses and polygons, optical margin alignment, glyph scaling in justification, balanced ragged lines, carets and hit-testing, and type on a path.
- Still not wired into the app; the window keeps running on the engine crates it used before.

## 0.13.0 · 2026-10-10

- formdes's own layout engine has begun, to replace the engine crates the core builds on today. `native/formdes-model` is the document model and the `.fdes` file format: pages, spreads, parents, sections, layers, items, stories, styles, swatches. `native/formdes-text` is the text engine: font matching with fallback, OpenType shaping, hyphenation, a Knuth and Plass paragraph composer, justification, tabs, drop caps, small caps, bidirectional text, lists, nested and GREP styles, and flow through threaded frames and columns with wrap, keep options, widow control, breaks, footnotes, tables and vertical justification. Neither is wired into the app yet; nothing you see changes in this release.
- Parity re-measured in 170 traced rows against InDesign, Affinity and Acrobat: 93 (55 percent). The earlier 48.0 of 53 credited menu entries that fail or run without parameters in the window and left out work on existing PDFs.

## 0.12.0 · 2026-10-08

- Websites from any layout: File > Export > Website makes a folder (or a .zip) of web pages that reflow for phones, in the document's own type, colours and embedded fonts. A magazine or report with several articles becomes a home page with a card for each article and a page per article, figures placed among the paragraphs; a web template keeps its rows and columns (hero, feature columns, card grids, stats) and stacks them on a phone; one-page pieces become one page.
- The edition: a page-by-page view of the layout as designed, spreads side by side on wide screens, with its text kept for search and reading aloud and links on the pages clickable.
- An address for the site adds a sitemap, an RSS feed for articles, canonical and share links; every article page carries its headline, description and author for search engines.
- Publish Online (File > Publish Online): the open document's site gets a link served from this computer, updated each time you publish again; the list of published sites opens, copies, shows, downloads as .zip or unpublishes each one.
- Export reports what to fix before publishing: images without alt text, buttons without a link, placeholder art still in place, fonts that cannot be embedded.
- For agents: `export_document` takes `web` and the `publish_website` tool publishes, lists and unpublishes (29 tools); `./formdes export FILE --out site.zip` from a terminal.
- Parity with InDesign measured at 48.0 of 53, unchanged: HTML export and Publish Online (row 42) stays partial because a published site's link works on this computer only; a hosted publishing service is not built.

## 0.11.0 · 2026-10-08

- Workspaces: Window > Workspace and a switcher in the title bar with Essentials, Advanced, Book, Digital Publishing, Interactive for PDF, Printing and Proofing and Typography, plus your own (New, Delete, Reset).
- Panels: drag a group by its header within or between the columns, drop a tab between groups to make a new group, set group heights and the dock's width, right-click a header to close or minimise a group; all remembered.
- Keyboard shortcut sets (Edit > Keyboard Shortcuts, Alt+Shift+Ctrl+K): product areas, search, current shortcuts, a new shortcut with the command that has it now, Show Set; tools are listed and can be rebound. Menus show the keys as changed.
- Soft proofing: View > Proof Setup (Working CMYK, a custom press with any CMYK profile, intent and Simulate Paper Color, RGB screens, two colour-blindness simulations) and View > Proof Colors. Loaded ICC profiles are kept in the data folder.
- Spelling in other languages: text is checked in its own language; Edit > Language Dictionaries gets twenty dictionaries from the LibreOffice project or installs your own Hunspell files.
- Fixed: a request whose window closed mid-call no longer leaves its reply for the next call (every later answer was one behind); a core shortcut works straight after the selection changes, without opening a menu first; dialog fields and notes are styled.
- Parity with InDesign measured at 48.0 of 53.

## 0.10.0 · 2026-10-08

- Object States (Window > Object States): make a multi-state object from a selection, show, rename and release states.
- Buttons and Forms: buttons that go to a page or a URL; text fields, check boxes, combo boxes, list boxes and signature fields with names, defaults, choices, font size, multiline and required. Placed video and sound get Play on page load, Loop and Show controls.
- Page Transitions: a transition and speed for each spread, Apply to All Spreads, Clear All.
- The XML Tags and Structure panels (Alt+Ctrl+1): tag frames and text, map paragraph styles to tags, load a DTD and validate, import XML into tagged frames, export XML.
- Libraries (Window > Library): libraries of saved pieces with thumbnails; add the selection, drag an item onto the page to place it there, or place it where it was; rename, delete and search. Libraries live in the data folder or in `FORMDES_LIBRARIES_DIR`.
- The `library` MCP tool (28 tools): agents list, create and fill libraries and place their pieces.
- Parity with InDesign measured at 47.0 of 53.

## 0.9.0 · 2026-10-08

- Find/Change (Edit > Find/Change, Ctrl+F): a floating window with Text, GREP, Glyph and Object tabs; InDesign's special characters and GREP codes, lookahead, lookbehind and `$1`; Find Format and Change Format; scopes, case, whole word, locked, hidden and parent-page options; tables and footnotes included; Find All with page and context; Find Next (Alt+Ctrl+F), Change, Change All, Change/Find; twelve built-in queries and your own.
- The `find_change` MCP tool (27 tools): count, list and change all, and run or save queries.
- Check Spelling (Ctrl+I) with suggestions, Skip, Change, Ignore All, Change All and Add; Dynamic Spelling underlines misspelled words; a dictionary for each document with its hyphenation exceptions.
- Type on a Path Options: align, start, flip, delete type from the path.
- Preflight profiles: [Basic], Commercial print, PDF/X-1a no transparency, Saddle-stitched booklet, Digital and screen, and your own; new checks for bleed, text near the trim, small type, very high resolution, spot inks, transparency and page count. The MCP `preflight` tool takes a profile.
- Fixed: function keys work while typing; the Make layout button stays in view in a short panel.
- Parity with InDesign measured at 46.0 of 53.

## 0.8.0 · 2026-10-08

- Separations Preview (View > Separations Preview, Shift+F6): one plate at a time, spot inks, and the ink limit marked in red.
- Overprint Preview (View > Overprint Preview, Alt+Shift+Ctrl+Y).
- The Review group: Track Changes (accept or reject one change or all), Notes (add, edit, delete, convert to text) and Conditional Text (new, apply, clear, show or hide, delete).
- The Story Editor shows each paragraph's style name beside it and a line where the text is overset.
- Object Style Options: name, fill, stroke, stroke weight and paragraph style, applied to every object that uses the style; Redefine from the selection; Delete.
- Parity with InDesign measured at 44.5 of 53.

## 0.7.1 · 2026-10-08

- Documentation: the wiki in `.docs/wiki/` (sixteen pages), `.docs/BRANCHING.md`, the user manual `docs/MANUAL.md`, this changelog, and `.github/workflows/wiki.yml` to publish the wiki.
- The `book` MCP tool: agents create books, add chapters, number pages, synchronise styles and export one PDF (26 tools).
- File > Recover Unsaved Documents reopens the recovery copies written every minute.

## 0.7.0 · 2026-10-08

- Rulers along the canvas with drag-out guides (View > Rulers, Ctrl+R).
- The Glyphs panel, drawn in the document's own fonts (new core methods `fonts.styles`, `fonts.face`, `fonts.data` and `/api/core/font`).
- The Tabs panel: a ruler of tab stops with alignment, position, leader and align-on.
- One Styles tab for paragraph, character and object styles; object styles can be applied and made from a selection.
- The Table panel: rows and columns, header and footer rows, row height, column width, cell fill, inset and vertical justification, borders, alternating rows, table and cell styles.
- Books (File > Book): chapters of saved documents, page numbering across them, style synchronisation, one PDF.
- The Story Editor (Edit > Edit in Story Editor, Ctrl+Y).
- A tab click opens a minimised panel group; the status bar shows section page names.

## 0.6.1 · 2026-10-08

- The right column shares its height, so Swatches, Pages, Layers, Links, Preflight and Info are all reachable beside Make.
- Panel tab names read in full.
- Typing straight after drawing a text frame keeps its first letters.
- A Markdown heading under the title becomes the deck, a "By Name" line the byline.
- The first parity measurement against InDesign.

## 0.6.0 · 2026-10-07

- The desktop app for macOS, Windows and Linux (Tauri 2, the engine and the layout core as sidecars).
- CI: frontend build, engine checks, template lint, skills drift, a command-line make, script syntax, the em dash and emoji scan; desktop bundles on each tag.
- The PDF Reader: thumbnails, selectable text, search, outline, plain text, Extract into Make.
- The agent skills: `formdes-harness` with eight sub-skills, `formdes-template-creator`, `formdes-skillset`; `CLAUDE.md`, `AGENTS.md`, the `.agents/skills` mirror.
- Briefs are read for page counts, dates, times, issues, brands and event names.
- Titles, dates and prices shrink further before any word is trimmed.

## 0.5.0 · 2026-10-07

- Intake of Markdown, Word, RTF, text, HTML, CSV, spreadsheets, PDFs, images and briefs.
- 71 templates with the compiler, registry, checker and brand kits.
- The Make pass: rank, fill, thread, continue, fit, report.
- The MCP server (25 tools), the stdio bridge, the agent installers and the CLI.
- The editor: tiled canvas, overlays, input, menus from the core's catalogue, tool rail, tabs, and first versions of the panels and dialogs.

## 0.1.0 · 2026-10-07

- The scaffold: dispatcher and setup scripts, formdes-core over formdes's own layout crates, the FastAPI engine with the window socket, the Studio Dark shell and home screen.
