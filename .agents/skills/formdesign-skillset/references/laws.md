# Build laws

- **Surgical edits only.** Change existing files with single-region hand edits made against a fresh read. No sed, awk, perl, codemods, patch files, `git apply` or bundles; an inbound diff is re-made by hand hunk by hunk. New files are fine when the change needs them.
- **Zero DOM flash.** Mount once, patch in place; keyed lists never clear and refill; reserve space for async content; no spinner before 300 ms; the canvas keeps stale tiles until their replacements arrive. Verify under 4x CPU throttling.
- **Tokens only.** Colours from `var(--...)`, stacking from `var(--z-*)`; app-owned media colours are tokens in `styles/formdesign.css`. No visible scrollbars, no icon libraries (inline currentColor SVG), no emoji anywhere, no em dash in markdown.
- **Vite** binds localhost only (`host: false`) on pinned ports 5196 and 5197; the engine is 8196.
- **Tests only as the family plan names them.** `backend/tests` (pytest), `native/*/tests` (cargo test) and `frontend/tests/window` (the window smoke, pytest with Playwright) hold the suite FAMILY_TEST_PLAN names, and CI runs it; add no other test code. `TEST_PLAN.md` in the planning sidecar says what is covered and what is manual.
- **Comments** stay near zero; context goes to the folder's `HASHOUT_LOG.md`.
- **Licences:** permissive dependencies only in anything shipped; record non-trivial ones in `NOTICE.md`.
- **Versions** are `vX.Y.Z` with dots; commits are Conventional Commits made as nodemode-ai; the bump is the last commit of a release.
- **No product names.** Never name another layout, drawing or photo application, or its maker, in code, strings, comments, docs or commit messages; describe the behaviour and name formats by extension. Only interop literals a format defines (MIME types, XML namespaces, byte signatures) and licence credits keep such names.
- **No sparkle icons.** The sparkle (a cluster of three or four pointed stars, or a wand trailing them) is banned app-wide, for AI, Claude, Make or anything else; use a glyph that shows what the control does. `./formdesign counts --check` fails if one returns.
- **Declined features stay declined.** Content-aware scale and skin protection are refused for patent reasons in every form (seam carving, retargeting, protected-region resizing); `docs/DECLINED.md` is the record.
- **Hand back** a versioned repo zip with `.git` and the planning sidecar zip at every milestone.
