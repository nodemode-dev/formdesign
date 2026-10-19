# Agents and MCP

formdes is built to be driven by agents as much as by hand. Claude Code, Codex and Cursor reach it through an MCP server; scripts and pipelines reach it through the CLI. Both act on the same engine documents as the window, so an agent's layout appears live in an open window and a person can finish it there.

## The MCP server

| Transport | How | When |
| --- | --- | --- |
| Streamable HTTP | `POST http://127.0.0.1:8196/mcp`, stateless JSON, Origin checked | the engine is already running |
| Standard input and output | `./formdes mcp` (`setup/mcp_stdio.py`), which starts the engine if it is not running | Claude Desktop, Claude Code, Codex, Cursor |

The catalogue lives in `backend/app/routers/mcp.py` (`TOOLS`), with a `layout_strategy` prompt. The 29 tools, by group:

| Group | Tools |
| --- | --- |
| Intake and make | `intake`, `list_templates`, `get_template`, `make_layout` |
| Look and fix | `preview`, `get_slots`, `set_slots`, `get_document`, `preflight` (with a `profile`), `find_change` |
| Documents | `list_documents`, `switch_document`, `new_document`, `open_document`, `save_document`, `export_document`, `publish_website`, `book` |
| Any command | `run_commands`, `find_commands`, `undo`, `redo` |
| PDF sources | `read_pdf`, `pdf_page_image` |
| Brands and templates | `list_brands`, `save_brand`, `validate_template`, `save_template`, `library` |

The fast path for a finished piece is five calls: `intake` (files and a brief into a pack), `make_layout` (the Make pass and its report), `preview` (page images), `set_slots` (replace what is still sample text), `export_document` (PDF, IDML, PNG, HTML, EPUB, a website or `.fdes`). `run_commands` reaches all 270 core commands for anything else; `find_commands` searches them by words.

For the web, `export_document {format: "web"}` writes a responsive site to a folder or a .zip (options `split`, `layout`, `edition`, `baseUrl`, `title`, `description`, `language`) and returns its pages and `warnings`; `publish_website` publishes the active document's site at `http://127.0.0.1:PORT/sites/SLUG/`, lists published sites and unpublishes them.

## The Claude dock

The window has a dock under the canvas for terminals and Claude. `ui.terminal.toggle` (Window > Show Terminal, Ctrl or Cmd+J) shows or hides it; `ui.terminal.new` adds a shell tab; `ui.terminal.claude` (the title-bar Claude Code button, Ctrl or Cmd+Shift+J) opens or reuses the Claude Code tab; `ui.claude` (Chat with Claude, Ctrl or Cmd+Alt+J) opens the dock on its Chat tab, where the Ask line sends a message about the open document. The window fires `fd-dock` with `{open}` whenever the dock opens or closes.

| Piece | Where |
| --- | --- |
| Terminal sessions | `POST /api/term` with `{preset, cols, rows, prompt?, resume?, reuse?}`; with `preset: "claude"` and `reuse: true` a live Claude Code session receives the prompt instead of a second one starting |
| Restart | `POST /api/term/{sid}/restart` runs the tab's program again in the same tab, preset and folder; a program that ends prints "Restart runs it again" in its tab |
| Chat tab | `GET` and `POST /api/claude/chat`, `/claude/chat/stop`, `/claude/chat/reset`; From Claude Code adopts a Claude Code conversation (`/claude/sessions`, `/claude/adopt`), Open in Claude Code continues the chat in the Claude Code tab |
| Status and preferences | `GET /api/claude/status` reports `installed`, `path`, `version`, `folder`, `install` and `binNote`; `POST /api/claude/prefs` saves `claudeFolder`, `claudeBin` and `claudeFirstMessage` |

Claude Code starts in the agent folder: the `claudeFolder` preference (Preferences > Agents > Claude Code in FormDesign, Starts in folder), else `FORMDES_AGENT_DIR`, else the `Agent` folder inside the engine's data folder, created when missing. The built-in Working with Claude workspace shows Pages and Properties and runs `ui.claude`; the Essentials workspace keeps Pages and Make on the left and Properties with Swatches on the right.

## The skills

| Skill | Path | For |
| --- | --- | --- |
| `formdes-harness` | `.claude/skills/formdes-harness/` | making layouts: a director and eight sub-skills (intake-curator, template-picker, layout-director, layout-finisher, brand-stylist, publisher, pdf-reader, cli-fallback), with the generated `references/tools.md` and `references/templates.md` |
| `formdes-template-creator` | `.claude/skills/formdes-template-creator/` | writing and checking new templates |
| `formdes-skillset` | `.claude/skills/formdes-skillset/` | working on formdes's own code |

`./formdes agents sync` regenerates the harness skill's tool and template references from the engine and mirrors the two user-facing skills to `.agents/skills/` for Codex and Cursor. CI runs `agents sync --check` and fails on drift.

## Installing for each agent

```bash
./formdes agents install --dry-run     # what it would change
./formdes agents install               # Claude Code, Codex and Cursor, for this user account
./formdes agents install claude        # one of them
```

It registers `./formdes mcp` at this folder's absolute path and installs the skills, backing up any file before it changes; running it twice changes nothing. Inside this repository, `.mcp.json`, `.codex/config.toml` and `.cursor/mcp.json` already point at the server, and `CLAUDE.md` and `AGENTS.md` route agents to the harness skill.

## The CLI

```bash
./formdes make FILES --brief "..." [--template ID] [--brand NAME] [--fills JSON] [--variations N] --out layout.pdf [--out layout.fdes]
./formdes templates [ID] [--category C] [--kind K] [--q WORDS] [--json]
```

`make` prints the Make pass report as JSON and writes each `--out` (`.fdes`, `.pdf`, `.idml`, `.png`, `.jpg`, `.html`, `.epub`). Relative paths are taken from the folder you ran it in.

## Contract

- A new tool goes into `TOOLS` with a description an agent can act on, annotations (`readOnlyHint`, `destructiveHint`) that are true, and a small result; preview images are the only large payloads.
- After adding or changing a tool or a built-in template, run `./formdes agents sync` in the same commit.
- Tools act on the active engine document unless they take a document argument; they never touch the window directly.
- Book operations share one tool, `book`, with an `action`; it calls the same service (`backend/app/services/books.py`) as the Book window.
