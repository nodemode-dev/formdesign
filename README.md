<p align="center"><img src="assets/formdes_folder_icon.png" width="112" alt="formdes"></p>
<h1 align="center">formdes</h1>
<p align="center"><b>Page layout and publishing for the web and the desktop, made for people and their agents, in the NodeMode design system.</b></p>
<p align="center">
<img src="https://img.shields.io/badge/version-0.17.0-f97316" alt="version 0.17.0">
<img src="https://img.shields.io/badge/node-22-339933" alt="node 22">
<img src="https://img.shields.io/badge/python-3.12-3776AB" alt="python 3.12">
<img src="https://img.shields.io/badge/rust-1.90-B7410E" alt="rust 1.90">
<img src="https://img.shields.io/badge/tauri-2-24C8DB" alt="tauri 2">
<img src="https://img.shields.io/badge/license-Apache--2.0-lightgrey" alt="Apache 2.0 license">
</p>

## What it is

formdes is the form family's page-layout application: an alternative to InDesign for magazines, newsletters, books, reports, print pieces, ads, websites and blogs. Its layout core is formdes's own engine (212 InDesign-class commands and 36 tools: spreads and parent pages, threaded stories, a Knuth-Plass paragraph composer, styles, tables, footnotes, PDF, IDML and EPUB), wrapped by formdes's own interface in formvis's Studio Dark language and by an engine service built for agents: drop in Markdown, PDFs, Word files, images and a brief, pick a template, and formdes lays it out.

**Stack.** Vanilla TypeScript and Vite · Python and FastAPI · Rust (formdes-core, a JSON-lines sidecar over formdes's own layout crates) · Tauri 2.

## Status at 0.17.0

Working end to end:

- **Intake.** Markdown, Word, RTF, text, HTML, CSV and spreadsheets, PDFs (structure-aware extraction), images and GIFs, and a brief in plain words become one compact content pack. Key: value lines become fields, and the brief is read for kind, size, page count, tone, colours, brand, event name, date, time, issue and call to action.
- **71 templates** across magazine, newsletter, book, report, print, stationery, ad, web and presentation, each slotted for automatic filling and re-skinned by a brand kit. `docs/TEMPLATES.md` is the format; you can add your own.
- **The Make pass.** Ranks templates for your content, fills them (titles, decks, bylines, stats, features, captions, contact fields), threads the article through continuation pages, matches images to frames, fits copy, and reports what is still sample text.
- **Websites.** Any layout as a responsive multi-page site in its own type and colours: an article per page with a home page of cards, a web template's rows and columns that stack on a phone, a page-by-page edition, sitemap and feed (File > Export > Website), and File > Publish Online for a link served from this computer.
- **Agents.** An MCP server with 29 tools (`./formdes mcp`, `./formdes agents install` for Claude Code, Codex and Cursor), a CLI (`./formdes make FILES --brief "..." --out layout.pdf`, `./formdes export FILE --out site.zip`), and three skills in `.claude/skills` (mirrored to `.agents/skills`): `formdes-harness` (eight sub-skills from intake to publishing), `formdes-template-creator` and `formdes-skillset`.
- **The editor.** Tiled rendering of the layout core with overlays, the 36 InDesign tools, menus built from the core's 212 commands, document tabs, and first versions of the Make, Slots, Properties, Character, Paragraph, Swatches, Styles, Pages, Layers, Links and Preflight panels and the New Document and Export dialogs.
- **Desktop.** A Tauri 2 app for macOS, Windows and Linux with the engine and the layout core as sidecars; CI builds the bundles on each `v*.*.*` tag.

- **InDesign workspace pieces.** Rulers with drag-out guides (Ctrl+R), the Glyphs panel, the Tabs ruler, one Styles tab for paragraph, character and object styles, the Table panel (options, alternating rows, cell and table styles), Books (File > Book: chapters, numbering, style sync, one PDF) and the Story Editor (Ctrl+Y) with a style-name column and the overset line.
- **Review and output.** Track Changes, Notes and Conditional Text panels; Separations Preview with the ink limit (Shift+F6) and soft proofing (View > Proof Setup and Proof Colors: an approximate CMYK proof with paper simulation, and colour-blindness views; profile-accurate proofing and Overprint Preview return with a colour engine); Object Style Options, Redefine and Delete; preflight profiles (Commercial print, PDF/X-1a, booklets, digital, your own) with bleed, trim, type size, resolution, spot ink, transparency and page count checks.
- **Editing text at scale.** Find/Change (Ctrl+F) with Text, GREP, Glyph and Object tabs, formats, scopes and saved queries, also as the `find_change` MCP tool; Check Spelling (Ctrl+I) and Dynamic Spelling in the text's own language (US English built in, twenty more from Edit > Language Dictionaries) and a dictionary per document; Type on a Path Options.
- **Interactive, structure and reuse.** Object States, Buttons and Forms (with media options) and Page Transitions panels; the XML Tags and Structure panels (Alt+Ctrl+1) with style mapping, DTD validation, import and export; Libraries of saved pieces with thumbnails, dragged onto the page, also as the `library` MCP tool.
- **Your workspace.** Seven built-in workspaces and your own (Window > Workspace or the title-bar switcher); panel groups that move between columns, split, size and close; keyboard shortcut sets with every command and tool rebindable (Alt+Shift+Ctrl+K).

Measured against InDesign and Affinity in 133 traced rows (full 1, partial 0.5, none 0), formdes scored 88 (66 percent) when last traced at 0.12.0: 66 percent of what InDesign documents and 68 of Affinity; it has not been re-traced on its own engine yet, which drops Overprint Preview, canvas markers and typing in table cells for now. Work on existing PDFs is not scored: it belongs to FormPDF, a separate app. The earlier 53-row count of 48.0 credited menu entries that fail or run without parameters in the window (hyperlinks, Data Merge, table of contents, cross-references, QR codes, Print Booklet); a capability reachable only through a raw parameters box or MCP `run_commands` now counts as partial. Next: the engine commands still to come (clipboard, hyperlinks, cross-references, table of contents, data merge, indexes), then dialogs for every command that needs one.

**Documentation.** `docs/MANUAL.md` is the user manual; `.docs/wiki/` is the developer wiki (published to the GitHub wiki); `.docs/BRANCHING.md` the branch and release model; `CHANGELOG.md` every release.

## Try it

```bash
./formdes boot                     # then open http://localhost:5196
./formdes templates                # list the 71 templates
./formdes make article.md photo.jpg logo.png --brief "Square instagram ad for Northlight. CTA: Read the issue" --out ad.png
```

In the app, drop files on the Make panel (right column), write a brief, press Analyse, pick a template and press Make layout.

## Quick Start

Requirements: Node 20.19 or newer (22 recommended), Python 3.12 or newer, Rust stable (for the layout core; without it the installer fetches a prebuilt core when a release has one), git.

### macOS and Linux

```bash
git clone https://github.com/nodemode-ai/formdes.git
cd formdes
./formdes install
./formdes boot
```

### Windows

```bat
git clone https://github.com/nodemode-ai/formdes.git
cd formdes
formdes.bat install
formdes.bat boot
```

The first `install` compiles the layout core (a few minutes on the first build). `./formdes help` lists every command; `./formdes doctor` checks the toolchain, the installs and the ports.

| Service | URL | Notes |
| --- | --- | --- |
| Frontend (dev) | http://localhost:5196 | Vite, bound to localhost only, strict port |
| Engine | http://127.0.0.1:8196 | FastAPI; API under `/api`, the window socket at `/api/ws`; serves the built app in `./formdes serve` |
| Preview | http://localhost:5197 | `vite preview` of the production build |
| Layout core | standard input and output | `formdes-core`, started and supervised by the engine |

### Desktop app

```bash
./formdes install-desktop          # PyInstaller, and checks Rust and the webview libraries
./formdes desktop dev              # run the app against the frozen engine
./formdes desktop build            # bundles in src-tauri/target/release/bundle (.dmg, .msi and .exe, .deb, .rpm, .AppImage)
```

On Windows use `formdes.bat install-desktop` and `formdes.bat desktop build`. Linux needs the webview packages first: `sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev patchelf`. The desktop engine prefers port 8196 and takes a free port when 8196 is busy.

## Configuration

The engine reads `backend/.env`, which is never committed; `backend/.env.example` is the template.

| Variable | Purpose |
| --- | --- |
| `FORMDES_HOST` | Engine bind address |
| `FORMDES_PORT` | Engine port |
| `FORMDES_DATA_DIR` | Projects, templates, brand kits, intake and exports (default `~/Formdes`) |
| `FORMDES_CORE` | Path to a formdes-core binary to use instead of the built one |
| `FORMDES_FONT_DIRS` | Extra font folders for the layout core |
| `FORMDES_SYSTEM_FONTS` | `0` keeps the core from cataloguing the system's fonts |
| `FORMDES_CORS_ORIGINS` | Allowed browser origins for the dev frontend |
| `FORMDES_DIST` | Built frontend folder the engine serves |

## Architecture

```text
formdes/
├── formdes, formdes.bat           root dispatcher: install, boot, build, serve, desktop, make, mcp, agents, doctor
├── formdes_oc_install(.bat)       one-command install
├── setup/                         one script per task, each with a .bat twin; lib.sh holds shared helpers
├── assets/                        repo art (README logo, desktop icon source)
├── native/                        Rust workspace: formdes-core over formdes-model, -text, -render, -interchange and -engine
├── backend/                       FastAPI engine: the core process, intake, templates, the Make pass, MCP
├── frontend/                      Vite app, vanilla TypeScript: the shell, the canvas, panels and dialogs
├── src-tauri/                     Tauri 2 desktop shell; the engine and the core ride along as sidecars
├── .claude/skills/                formdes-harness, formdes-template-creator, formdes-skillset (mirrored to .agents/skills)
└── .github/workflows/             ci (checks and a command-line make) and desktop (bundles and the release)
```

## Development

Branch model: `main` plus short-lived work branches. Contributions follow the dev-rules laws: single-region hand edits, zero DOM flash or re-render, no emoji, no em dash, no icon-library dependencies, `v*.*.*` versions, Conventional Commits. Read `frontend/HASHOUT_LOG.md`, `backend/HASHOUT_LOG.md` and `native/HASHOUT_LOG.md` before editing.

## Contributors

- **nodemode-ai** · owner and build

## Licensing

Apache License 2.0. Copyright 2026 nodemode-ai. See `LICENSE` and `NOTICE.md` for the fonts, dictionaries and libraries formdes includes.
