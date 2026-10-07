---
name: formdes-skillset
description: "Develops formdes itself across its three layers. Use whenever the task is to change, fix, extend or explain formdes's code: the Rust layout core (formdes-core over formdes's own layout crates), the FastAPI engine (intake, template compiler, Make pass, slots, documents, reader, MCP, CLI), the TypeScript editor (tiled canvas, overlays, input, shell, panels) or the setup scripts; a bug in the canvas or a panel, a new MCP tool, a compiler or Make pass change, a new core method, setup or build scripts, versions and release zips. Not for making layouts with formdes (formdes-harness) or authoring templates (formdes-template-creator)."
license: Apache-2.0
metadata:
  author: nodemode-ai
  compiled: "2026-10-07"
  version: "0.6.0"
---

# formdes skillset

formdes is three processes with one owner of the truth. The Rust layout core owns every document and its 432 commands; the Python engine supervises it and adds what a layout core does not have (intake, templates, Make, slots, documents, reader, MCP, CLI); the window is a thin client that renders tiles and overlays from the core and sends input to it. Most bugs come from putting logic in the wrong layer, so the first question for any change is which layer owns it.

## Map

| Layer | Folder | Owns |
| --- | --- | --- |
| Layout core | `native/formdes-core/src` | `proto.rs` JSON lines with binary payloads; `core.rs` dispatch, revisions, damage, `frame.thread`, `slots`, `docs.*`, fonts; `view.rs` layout and tiles and page images; `overlays.rs` overlay geometry; `input.rs` pointer, keys, text, cursor; `slots.rs` |
| Engine | `backend/app` | `engine/core.py` the core process and change events; `engine/documents.py` .fdes, open, save, export, autosave; `routers/*` REST, WebSocket and `/mcp`; `intake/*` content packs; `services/pdfread.py`; `templates/*` spec, compile, registry, brands, samples, check; `autolayout/*` content, rank, make, slots, fill, tagged; `cli.py` |
| Window | `frontend/src` | `engine/client.ts`; `editor/editor.ts` state sync, `viewport.ts` tiles and overlays, `input.ts`; `ui/shell.ts` menus, panels, tabs, rail, open and save; `ui/workspace.ts` the feature contract; `features/*` Make, Properties, Pages, Document, Reader |
| Setup | `formdes`, `formdes.bat`, `setup/*` | one script per task with a .bat twin; `setup/agents.py` and `setup/mcp_stdio.py` for agents |

`references/workflows.md` has the change recipes (a new MCP tool, a new core method, a new panel, a new template feature, a release) and `references/laws.md` the rules every change keeps.

## Procedure

1. Read the folder's `HASHOUT_LOG.md` (`backend/`, `frontend/`, `native/`) before editing.
2. Decide the owning layer with the map; follow the matching recipe in `references/workflows.md`.
3. Edit by hand, one region at a time; no scripted edits.
4. Verify for real: `npx tsc --noEmit -p frontend`, import the backend (`venv/bin/python -c "import app.main"`), `cargo build --release -p formdes-core` for core changes, `python -m app.templates.check` for compiler changes, and a browser run with screenshots for UI (look at them; throttle CPU 4x for zero-flash checks).
5. Commit one coherent change per commit with the nodemode-ai identity; bump versions last.

## Gates

- Type check, import and build pass; the changed behaviour was exercised (screenshot, MCP call or CLI run) and its result looked at.
- `./formdes agents sync --check` passes after any MCP tool or template library change.
- No emoji anywhere, no em dash in markdown, no raw hex or raw z-index in component CSS, no icon libraries, no comments that should be in a HASHOUT log.

## Reference files

- `references/workflows.md`: recipes for the common kinds of change.
- `references/laws.md`: the build rules (surgical edits, zero DOM flash, tokens, versions, commits, hand-back).
