# FormDesign (repo and tools: formdes)

Page layout and publishing (an InDesign alternative) for the web and the desktop in the form family's design language: a Rust layout core (formdes-core over formdes's own layout crates in `native/`), a Python and FastAPI engine, a vanilla TypeScript and Vite window, a Tauri 2 shell.

## Making layouts

When the person wants something laid out (a magazine spread, newsletter, report, flyer, poster, menu, card, ad, banner, landing page, blog, email or deck) from files, a PDF, images or an idea, use the `formdes` MCP server with the `formdes-harness` skill (`.claude/skills/formdes-harness`), not formdes's source code. `.mcp.json` starts it (`./formdes mcp`, which starts the engine if it is not running). The fast path is intake, make_layout, preview, set_slots, export_document.

To read a document cheaply use `outline` (one line per item) and `inspect` (fields by id); to see a page or a region with its problems listed, `look`; to draw what only a tool draws, `use_tool`. Beyond the fast path: `find_commands` (by word or domain) then `run_commands`; their errors name the fix. `show` moves the person's view so they can follow. Risky steps (publishing, overwriting a file, packaging, printing, deleting several pages) can wait for the person's approval in Window > Agent Connect; if a call was not approved, ask them rather than retrying. In a shell, `./formdes cmd ID key=value`, `./formdes actions` and `./formdes tool NAME '{json}'` reach the same engine.

For a new template, use the `formdes-template-creator` skill.

## Working on formdes itself

Route through `.claude/skills/formdes-skillset/SKILL.md`: the map of the three layers, the change recipes and the build laws. Read the folder's `HASHOUT_LOG.md` (`backend/`, `frontend/`, `native/`) before editing.

- `frontend/src/editor/` never owns layout logic; it renders tiles and overlays from the core and sends input.
- New UI goes in a feature module under `frontend/src/features/` registered through `ui/workspace.ts`.
- After changing MCP tools or the template library, run `./formdes agents sync` (it regenerates the harness skill's tool and template references and mirrors the skills to `.agents/skills`).
- Planning documents (DEV_PLAN, BUILD_NOTES, TEST_PLAN, the root HASHOUT_LOG, dev-wise) ship in the planning sidecar zip, not in this tree.
