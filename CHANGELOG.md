# Changelog

Every release of formdes, newest first. Versions follow semver; tags read `v*.*.*`.

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
