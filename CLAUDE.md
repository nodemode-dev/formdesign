# FormDesign (repo and tools: formdesign)

Page layout and publishing (a professional page-layout application) for the web and the desktop in the form family's design language: a Rust layout core (formdesign-core over FormDesign's own layout crates in `native/`), a Python and FastAPI engine, a vanilla TypeScript and Vite window, a Tauri 2 shell.

## Making layouts

When the person wants something laid out (a magazine spread, newsletter, report, flyer, poster, menu, card, ad, banner, landing page, blog, email or deck) from files, a PDF, images or an idea, use the `formdesign` MCP server with the `formdesign-harness` skill (`.claude/skills/formdesign-harness`), not FormDesign's source code. `.mcp.json` starts it (`./formdesign mcp`, which starts the engine if it is not running). The fast path is intake, make_layout, preview, set_slots, export_document.

To read a document cheaply use `outline` (one line per item) and `inspect` (fields by id); to see a page or a region with its problems listed, `look`; to draw what only a tool draws, `use_tool`. Beyond the fast path: `find_commands` (by word or domain) then `run_commands`; their errors name the fix. `show` moves the person's view so they can follow. Risky steps (publishing, overwriting a file, packaging, printing, deleting several pages) can wait for the person's approval in Window > Agent Connect; if a call was not approved, ask them rather than retrying. In a shell, `./formdesign cmd ID key=value`, `./formdesign actions` and `./formdesign tool NAME '{json}'` reach the same engine.

For a new template, use the `formdesign-template-creator` skill.

## Working on FormDesign itself

Route through `.claude/skills/formdesign-skillset/SKILL.md`: the map of the three layers, the change recipes and the build laws. Read the folder's `HASHOUT_LOG.md` (`backend/`, `frontend/`, `native/`) before editing.

- `frontend/src/editor/` never owns layout logic; it renders tiles and overlays from the core and sends input.
- New UI goes in a feature module under `frontend/src/features/` registered through `ui/workspace.ts`.
- After changing MCP tools or the template library, run `./formdesign agents sync` (it regenerates the harness skill's tool and template references and mirrors the skills to `.agents/skills`).
- Never name another layout, drawing or photo application, or its maker, anywhere in the tree (code, strings, comments, docs, commit messages); describe the behaviour and name formats by extension. Interop literals a format defines (MIME types, XML namespaces, byte signatures) and licence credits are the only exceptions.
- The same holds for other applications' feature names: a command, tool, menu or dialog is named for what it does (Fill Regions, Shape Crafter, Trace Image, AI Expand; command ids such as `path.livePaint` keep their names). A renamed English string gets new translations in all 23 tables and the old rows go with `./formdesign i18n prune OLD.json`; translations follow FormDesign's English, never another application's localized menus.
- Never copy another application's help text, presets, swatch libraries, templates or icons. Every new font, profile, dictionary, model or library gets a row in `NOTICE.md` with its source and licence before it ships.
- Content-aware scale and skin protection are declined for patent reasons and never built in any form; see `docs/DECLINED.md`.
- Planning documents (DEV_PLAN, BUILD_NOTES, TEST_PLAN, the root HASHOUT_LOG, dev-wise) ship in the planning sidecar zip, not in this tree.
