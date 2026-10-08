# Development Workflow

How changes are made to formdes. The branch model, commit format and version sites are in `.docs/BRANCHING.md`; this page covers the working rules.

## Before you edit

1. Read the `HASHOUT_LOG.md` of the folder you are changing: `backend/HASHOUT_LOG.md`, `frontend/HASHOUT_LOG.md` or `native/HASHOUT_LOG.md`. Source files carry almost no comments; what they would have said is in these logs, indexed by file and symbol.
2. Agents load `.claude/skills/formdes-skillset/SKILL.md`, which maps the three layers, lists the change recipes (a new panel, a new core method, a new MCP tool, a new template) and states the build laws.
3. Find the subsystem's page in this wiki and read its contract section.

## The build laws

| Law | What it means in practice |
| --- | --- |
| Surgical edits | Change existing files with small, single-region edits. No rewriting a working file, no `sed` sweeps or codemods, no applying patch files; an inbound patch is re-made by hand against the file as it is now. |
| Zero flash | No theme flash, layout jump, blank frame or rebuilt container in the window. Patch in place, reserve space, keep stale content until its replacement is ready. |
| Tokens only | Colours, z-index levels, spacing and radii come from `frontend/src/styles/tokens.css`. |
| No icon libraries | Inline `currentColor` SVG only. |
| No emoji, no em dash | Anywhere in the tree, including Markdown and commit messages. CI scans for both. |
| Local only | Vite binds to localhost on a pinned port; the engine binds to 127.0.0.1. |
| Licences | Vet a dependency's licence before adding it: permissive is fine everywhere, GPL never in what ships, AGPL never. Record non-trivial choices in `NOTICE.md`. |

## Making a change

| Kind of change | Where it goes | Also do |
| --- | --- | --- |
| A panel, dialog or view | a module in `frontend/src/features/`, registered in `features/index.ts` | its stylesheet in `frontend/src/styles/`, imported by the module |
| A core method | `native/formdes-core/src/core.rs` | add it to `JSON_METHODS` or `BINARY_METHODS` in `backend/app/routers/core_api.py` |
| An engine route | a router in `backend/app/routers/`, mounted in `backend/app/main.py` | |
| An MCP tool | `TOOLS` in `backend/app/routers/mcp.py` | `./formdes agents sync` |
| A built-in template | `backend/app/templates/library/[category]/` | the checker, both sweeps, `./formdes agents sync` |
| A dependency | `frontend/package.json`, `backend/requirements.txt` (and the macOS twin), `native/Cargo.toml` | licence check, `NOTICE.md` |

Then update the folder's `HASHOUT_LOG.md` for anything a reader would need explained, and this wiki's page when a contract changed.

## Verifying

Run what [[Verification]] lists for the layers you touched. For a quick local pass:

```bash
cd frontend && npx tsc --noEmit
cd backend && venv/bin/python -m compileall -q app && venv/bin/python ../setup/agents.py sync --check
./formdes make --brief "a flyer for a bake sale on Saturday at 10am" --out /tmp/flyer.png
```

## Planning documents

The plan (`DEV_PLAN.md`), the running build record (`BUILD_NOTES.md`), the test specification (`TEST_PLAN.md`), the parity scoreboard (`PARITY.md`), the root `HASHOUT_LOG.md` and `dev-wise.md` live in the planning sidecar that travels with each versioned build, not in this tree.
