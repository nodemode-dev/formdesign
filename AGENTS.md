# FormDesign for agents (Codex, Cursor and others)

FormDesign (repo, command and MCP server: formdesign) is a page-layout application with an MCP server. Use it to make and edit layouts; do not lay pages out by writing HTML or by editing FormDesign's source.

## Setup

- In this repository: `.codex/config.toml` and `.cursor/mcp.json` already point at `./formdesign mcp`; Cursor also reads `.cursor/rules/formdesign.mdc`.
- The person controls agents in FormDesign's Window > Agent Connect: Allow all, Ask for risky steps (publishing, overwriting a file, packaging, printing, deleting several pages) or Deny. A call that was not approved returns an error that says so: ask the person, do not retry in a loop.
- For your user account, from this folder: `./formdesign agents install` (Claude Code, Codex and Cursor; `--dry-run` shows what it would change).
- The skills live in `.agents/skills/formdesign-harness` (making layouts) and `.agents/skills/formdesign-template-creator` (new templates), mirrored from `.claude/skills` by `./formdesign agents sync`.

## The fast path

1. `intake` with absolute paths of the person's files and their brief.
2. `make_layout` with the returned pack id (and a template id if the ranking misread the kind).
3. `preview` page 1; read the report's `kept`, `placeholders` and `fit`.
4. `set_slots` by name to finish; `run_commands` for structural changes (`find_commands` by word or domain to look them up; errors name the nearest id or the parameters). `show` moves the person's view to what you changed.
5. `save_document` and `export_document` (pdf, idml, html, epub, png, jpg).

Without MCP, the same from a shell: `./formdesign make FILES --brief "..." --out layout.pdf --json`.

## Working on the code

Follow `.claude/skills/formdesign-skillset/SKILL.md` (architecture map, change recipes, build laws).

- Never name another layout, drawing or photo application, or its maker, anywhere in the tree; describe the behaviour and name formats by extension. Interop literals a format defines (MIME types, XML namespaces, byte signatures) and licence credits are the only exceptions.
- Content-aware scale and skin protection are declined for patent reasons and never built in any form; see `docs/DECLINED.md`.
- Shape Crafter, Liquid Layout and Trace Image (declined for patent risk, 2026-10-23): never infer the Shape Crafter operation from where a stroke lands (it comes from Alt or `mode`); never read guide contact in a Liquid Layout rule or export rules for a viewer to reflow (the `guide` rule is shown as "Stretch with page"); never add tracing between two clicked points.
