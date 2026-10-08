# formdes

formdes is the form family's page-layout application, an alternative to InDesign for magazines, newsletters, books, reports, print pieces, ads, websites and blogs. You (or an agent) drop in Markdown, PDFs, Word files, images and a brief in plain words; formdes picks one of its 71 templates, fills it, threads the article across pages, fits the copy and reports what is still sample text. The result is a real layout document you finish with InDesign-class tools in the window, or that an agent finishes over MCP.

Version 0.8.0 · Rust layout core (formdes-core over formdes's own layout crates), Python and FastAPI engine, vanilla TypeScript and Vite window, Tauri 2 desktop shell · Apache-2.0.

## Status at a glance

| Area | State |
| --- | --- |
| Layout core: 432 commands, tiles, overlays, slots, threads, fonts | Stable |
| Intake (Markdown, Word, RTF, text, HTML, CSV, spreadsheets, PDF, images, briefs) | Stable |
| Templates (71), brand kits, the template checker | Stable |
| The Make pass (rank, fill, continue, fit, report) | Stable; swept across all 71 templates every release |
| MCP server (26 tools), stdio bridge, agent installers, CLI | Stable |
| The editor: canvas, tools, menus, Properties, Character, Paragraph, Make, Slots, Pages, Layers, Links, Swatches, Styles, Preflight, Info | Working; verified in the browser at 0.6.1 |
| Rulers, Glyphs, Tabs, Table, Book, Story Editor | Working; new at 0.7.0, verified once each |
| Separations and Overprint Preview, Track Changes, Notes, Conditional Text, Object Style Options | Working; new at 0.8.0, verified once each |
| PDF Reader | Working |
| Desktop app | Linux built and launched; macOS and Windows built only by CI so far |
| Web export beyond one HTML page, soft proofing, a Find/Change dialog with GREP and saved queries, preflight profiles, the XML Structure and Library panels, animation | Not built |

The measured parity with InDesign (53 rows, full 1, partial 0.5, none 0) is 44.5 at 0.8.0.

## Where to go

| Page | What it covers |
| --- | --- |
| [[Getting-Started]] | Install, boot, every dispatcher command, ports, the data folder |
| [[Architecture]] | The three layers and how a change travels between them |
| [[Layout-Core]] | `native/formdes-core`: the JSON-lines protocol and its methods |
| [[Engine-Service]] | `backend/`: documents, intake, templates, books, routes |
| [[Make-Pass]] | How files and a brief become a filled layout |
| [[Templates]] | The template library, brand kits, adding templates |
| [[Agents-and-MCP]] | MCP tools, the skills, the CLI, installing for Claude Code, Codex and Cursor |
| [[Editor]] | `frontend/`: the canvas, input, the feature modules, the zero-flash rules |
| [[Project-Format]] | `.fdes` documents and `.fdbook` books, entry by entry |
| [[Desktop]] | The Tauri shell, the frozen engine, building bundles |
| [[Keyboard-Shortcuts]] | Every shortcut |
| [[Verification]] | What every change is checked against before it lands |
| [[Development-Workflow]] | Conventions, the logs to read first, version sites |
| [[Releasing]] | Tags, the desktop workflow, signing and the updater |
| [[Troubleshooting]] | Known problems and their fixes |

The user manual, written for people laying out documents rather than changing the code, is `docs/MANUAL.md` in the repository.

## Editing these pages

These pages live in the repository under `.docs/wiki/` and are published to the GitHub wiki by `.github/workflows/wiki.yml` when they change on `main`. Edit them there, in the same pull request as the code they describe; edits made in the GitHub wiki editor are overwritten at the next publish.

Write in full sentences, name real paths, and say what is untested. No emoji and no em dash; use the interpunct where a dash would go.
