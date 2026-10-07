# Change recipes

## Contents

- A new MCP tool
- A new core method
- A new panel or dialog
- A template format feature
- A Make pass change
- A release

## A new MCP tool

1. Add the tool to `TOOLS` in `backend/app/routers/mcp.py` with `_tool(name, description, props, required, read=..., destructive=..., idempotent=...)`. Descriptions say what the tool does and returns in one or two sentences; parameters carry their own descriptions.
2. Handle it in `_run(name, a)`; return `_text(data)` (compact JSON) or `_image(png)`; raise or return `_fail(...)` with what to do next.
3. Keep replies small: summaries, not dumps. An agent pays for every byte.
4. Add the tool to a group in `setup/agents.py` `GROUPS`, run `./formdes agents sync`, and update the harness skill's sub-skill that uses it.
5. Exercise it over HTTP: POST `{"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": ..., "arguments": {...}}}` to `http://127.0.0.1:8196/mcp`.

## A new core method

1. Add the match arm in `Core::call` (`native/formdes-core/src/core.rs`) or a module with a pure function over the `Document` (as `slots.rs`). Return JSON with `Reply::from`, or `Reply { result, bin }` for binary.
2. Edits that change the document go through `edit_undoable(label, |doc, sel| ...)` so they are one undo step and produce damage.
3. Add the method to `JSON_METHODS` or `BINARY_METHODS` in `backend/app/routers/core_api.py` if the window calls it.
4. `cargo build --release -p formdes-core` (about four minutes with thin LTO), restart the engine.

## A new panel or dialog

1. Create `frontend/src/features/[name].ts` exporting `feature: Feature`, helpers under `features/[name]/`, styles in `styles/[name].css` imported by the feature; list it in `features/index.ts`.
2. In `install(ws)`: `ws.registerPanel({id, label, group, column, order, create})`, `ws.addCommands(...)`, `ws.addMenuItems(...)`, `ws.override(coreId, run)` for core commands that need a dialog.
3. Build the DOM once; update with `patchList`, `setText`, `setClass`, `setValue`; read the document through `editor.inspect()` (cached per revision) and change it through `editor.exec` or `editor.batch` (one undo step).
4. Screenshot it with Playwright against `./formdes boot`, look, fix, repeat.

## A template format feature

1. Parse in `backend/app/templates/spec.py` (`resolve`, `_style_params`, `problems`), compile in `compile.py` (`Builder.item`, `style_object`, `structure`, `continuation`).
2. Document it in `docs/TEMPLATES.md` and the template creator's `references/spec.md`.
3. `python -m app.templates.check` over all templates; look at a contact sheet of the renders before and after.

## A Make pass change

1. Content selection lives in `autolayout/content.py` (`Content.value`, `Content.flow`), ranking in `rank.py`, the orchestration and fitting in `make.py`.
2. Run Make over every template with a full pack and a brief-only pack (a loop over `registry.templates()` calling `make.make`) and check the reports for warnings, overset and kept slots; render the first page of each and look at a contact sheet.

## A release

1. Hand edits to the versions: `frontend/package.json`, the two fields in `frontend/package-lock.json`, `frontend/src/version.ts`, `backend/app/__init__.py`, `native/Cargo.toml` workspace version and the formdes-core entry in `native/Cargo.lock`, the README badge.
2. Commit the bump last; tag `vX.Y.Z`.
3. Zip a clean clone with `.git` (origin `git@github.com:nodemode-ai/formdes.git`) as `formdes_vX.Y.Z.zip` and the planning sidecar as `formdes_planning_docs_sidecar_vX.Y.Z.zip`.
