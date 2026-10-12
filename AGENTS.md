# FormDesign for agents (Codex, Cursor and others)

FormDesign (repo, command and MCP server: formdes) is a page-layout application with an MCP server. Use it to make and edit layouts; do not lay pages out by writing HTML or by editing formdes's source.

## Setup

- In this repository: `.codex/config.toml` and `.cursor/mcp.json` already point at `./formdes mcp`; Cursor also reads `.cursor/rules/formdesign.mdc`.
- The person controls agents in FormDesign's Window > Agent Connect: Allow all, Ask for risky steps (publishing, overwriting a file, packaging, printing, deleting several pages) or Deny. A call that was not approved returns an error that says so: ask the person, do not retry in a loop.
- For your user account, from this folder: `./formdes agents install` (Claude Code, Codex and Cursor; `--dry-run` shows what it would change).
- The skills live in `.agents/skills/formdes-harness` (making layouts) and `.agents/skills/formdes-template-creator` (new templates), mirrored from `.claude/skills` by `./formdes agents sync`.

## The fast path

1. `intake` with absolute paths of the person's files and their brief.
2. `make_layout` with the returned pack id (and a template id if the ranking misread the kind).
3. `preview` page 1; read the report's `kept`, `placeholders` and `fit`.
4. `set_slots` by name to finish; `run_commands` for structural changes (`find_commands` by word or domain to look them up; errors name the nearest id or the parameters). `show` moves the person's view to what you changed.
5. `save_document` and `export_document` (pdf, idml, html, epub, png, jpg).

Without MCP, the same from a shell: `./formdes make FILES --brief "..." --out layout.pdf --json`.

## Working on the code

Follow `.claude/skills/formdes-skillset/SKILL.md` (architecture map, change recipes, build laws).
