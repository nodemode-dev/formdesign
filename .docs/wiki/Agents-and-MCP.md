# Agents and MCP

FormDesign is built to be driven by agents as much as by hand. Claude Code, Codex and Cursor reach it through an MCP server; scripts and pipelines reach it through the CLI. Both act on the same engine documents as the window, so an agent's layout appears live in an open window and a person can finish it there.

## The MCP server

| Transport | How | When |
| --- | --- | --- |
| Streamable HTTP | `POST http://127.0.0.1:8196/mcp`, stateless JSON, Origin checked | the engine is already running |
| Standard input and output | `./formdesign mcp` (`setup/mcp_stdio.py`), which starts the engine if it is not running | Claude Desktop, Claude Code, Codex, Cursor |
| Desktop extension | `./formdesign mcpb build DIR` writes `formdesign-VERSION.mcpb` (`setup/mcpb.py`, `setup/mcpb/manifest.json`); it runs `mcp_stdio.py --attach`, which never starts the engine and offers only `ping` until FormDesign answers | Claude Desktop |

The catalogue lives in `backend/app/routers/mcp.py` (`TOOLS`), with a `layout_strategy` prompt. The 38 tools (`agents/manifest.json` lists them, with the 16 that only read and the 7 that can wait for the person's approval), by group:

| Group | Tools |
| --- | --- |
| Intake and make | `intake`, `list_templates`, `get_template`, `make_layout` |
| Look and fix | `preview`, `get_slots`, `set_slots`, `get_document`, `preflight` (with a `profile`), `find_change` |
| Documents | `list_documents`, `switch_document`, `new_document`, `open_document`, `save_document`, `export_document`, `publish_website`, `book` |
| Any command | `run_commands`, `find_commands`, `undo`, `redo` |
| PDF sources | `read_pdf`, `pdf_page_image` |
| Brands and templates | `list_brands`, `save_brand`, `validate_template`, `save_template`, `library` |
| Review and place | `review`, `place_word` |
| Look at the layout | `outline`, `inspect`, `look`, `show` |
| Tools and commands by id | `use_tool` (one of the 41 layout tools by click or drag), `list_commands`, `run_command` |

The fast path for a finished piece is five calls: `intake` (files and a brief into a pack), `make_layout` (the Make pass and its report), `preview` (page images), `set_slots` (replace what is still sample text), `export_document` (PDF, IDML, PNG, HTML, EPUB, a website or `.fdes`). `run_commands` reaches all 394 core commands for anything else; `find_commands` searches them by words.

For the web, `export_document {format: "web"}` writes a responsive site to a folder or a .zip (options `split`, `layout`, `edition`, `baseUrl`, `title`, `description`, `language`) and returns its pages and `warnings`; `publish_website` publishes the active document's site at `http://127.0.0.1:PORT/sites/SLUG/`, lists published sites and unpublishes them.

## Agent Connect: how outside agents are asked

Any program that reaches the MCP server is an outside agent: Claude Desktop, Claude Code, Codex, Cursor, and FormDesign's own command line (`tool`, `cmd`, `cli run`), which shows up as `formdesign-cli`. Window > Agent Connect lists each agent that has connected and holds one choice per agent (`POLICIES` in `backend/app/services/agents.py`):

| Choice | What happens |
| --- | --- |
| Allow all | the agent's calls run without asking |
| Ask for risky steps | the default for an agent FormDesign has not seen before: ordinary calls run, and a risky step waits under Waiting for you in Agent Connect until the person chooses Approve or Decline |
| Deny | every call from that agent is refused |

The risky steps (`gate_reason` in the same file) are publishing a website or taking it down, sharing a document for review, saving or exporting over a file that already exists, packaging, printing, running a command that Undo cannot reverse, and deleting more than one page. The agent gets an error that says the person did not approve, and should ask rather than retry in a loop; the same happens when 300 seconds pass with no answer, and a risky step is refused at once when no FormDesign window is open to ask. The switch at the top of Agent Connect, Allow agents to use FormDesign (also in Preferences > Agents), turns all agent access off: every call is refused.

Only the FormDesign window can change any of this. The choices, the switch and Approve and Decline are engine routes that answer only a request carrying the window's key for this launch (`backend/app/core/window_key.py`); a script that posts to them without it gets 403 "Only the formdesign window can do this". The MCP endpoint itself stays open, because asking is what it does. The saved choices are sealed with a key in the system keyring where there is one: if the file is changed outside FormDesign, every Allow all falls back to Ask and Agent Connect says so.

Someone scripting FormDesign therefore uses the command line or the MCP endpoint and lets the person answer in the window, or asks the person to set that agent to Allow all in Agent Connect first.

## The Claude dock

The window has a dock under the canvas for terminals and Claude. `ui.terminal.toggle` (Window > Show Terminal, Ctrl or Cmd+J) shows or hides it; `ui.terminal.new` adds a shell tab; `ui.terminal.claude` (the title-bar Claude Code button, Ctrl or Cmd+Shift+J) opens or reuses the Claude Code tab; `ui.claude` (Chat with Claude, Ctrl or Cmd+Alt+J) opens the dock on its Chat tab, where the Ask line sends a message about the open document. The window fires `fd-dock` with `{open}` whenever the dock opens or closes.

| Piece | Where |
| --- | --- |
| Terminal sessions | `POST /api/term` with `{preset, cols, rows, prompt?, resume?, reuse?}`; with `preset: "claude"` and `reuse: true` a live Claude Code session receives the prompt instead of a second one starting |
| Restart | `POST /api/term/{sid}/restart` runs the tab's program again in the same tab, preset and folder; a program that ends prints "Restart runs it again" in its tab |
| Chat tab | `GET` and `POST /api/claude/chat`, `/claude/chat/stop`, `/claude/chat/reset`; From Claude Code adopts a Claude Code conversation (`/claude/sessions`, `/claude/adopt`), Open in Claude Code continues the chat in the Claude Code tab |
| Status and preferences | `GET /api/claude/status` reports `installed`, `path`, `version`, `folder`, `install` and `binNote`; `POST /api/claude/prefs` saves `claudeFolder`, `claudeBin` and `claudeFirstMessage` |

Claude Code starts in the agent folder: the `claudeFolder` preference (Preferences > Agents > Claude Code in FormDesign, Starts in folder), else `FORMDESIGN_AGENT_DIR`, else the `Agent` folder inside the engine's data folder, created when missing. The built-in Working with Claude workspace shows Pages and Properties and runs `ui.claude`; the Essentials workspace keeps Pages and Make on the left and Properties with Swatches on the right.

## The skills

| Skill | Path | For |
| --- | --- | --- |
| `formdesign-harness` | `.claude/skills/formdesign-harness/` | making layouts: a director and nine sub-skills (intake-curator, template-picker, layout-director, layout-finisher, brand-stylist, path-artist, publisher, pdf-reader, cli-fallback), with the generated `references/tools.md` and `references/templates.md` |
| `formdesign-template-creator` | `.claude/skills/formdesign-template-creator/` | writing and checking new templates |
| `formdesign-skillset` | `.claude/skills/formdesign-skillset/` | working on FormDesign's own code |

`./formdesign agents sync` regenerates the harness skill's tool and template references from the engine, writes `agents/manifest.json` (what FormDesign offers agents: its MCP tools with the read-only and approval-gated ones, its skills and its dock, all read from the code and the skills folder) and mirrors all three skills to `.agents/skills/` for Codex and Cursor (`agents install` copies only the two user-facing ones to your account). CI runs `agents sync --check` and fails on drift.

## Installing for each agent

```bash
./formdesign agents install --dry-run     # what it would change
./formdesign agents install               # Claude Code, Codex and Cursor, for this user account
./formdesign agents install claude        # one of them
```

It registers `./formdesign mcp` at this folder's absolute path and installs the skills, backing up any file before it changes; running it twice changes nothing. Inside this repository, `.mcp.json`, `.codex/config.toml` and `.cursor/mcp.json` already point at the server, and `CLAUDE.md` and `AGENTS.md` route agents to the harness skill.

## The CLI

```bash
./formdesign make FILES --brief "..." [--template ID] [--brand NAME] [--fills JSON] [--variations N] --out layout.pdf [--out layout.fdes]
./formdesign templates [ID] [--category C] [--kind K] [--q WORDS] [--json]
```

`make` prints the Make pass report as JSON and writes each `--out` (`.fdes`, `.pdf`, `.idml`, `.png`, `.jpg`, `.html`, `.epub`). Relative paths are taken from the folder you ran it in.

## Contract

- A new tool goes into `TOOLS` with a description an agent can act on, annotations (`readOnlyHint`, `destructiveHint`) that are true, and a small result; preview images are the only large payloads.
- After adding or changing a tool or a built-in template, run `./formdesign agents sync` in the same commit.
- Tools act on the active engine document unless they take a document argument; they never touch the window directly.
- Book operations share one tool, `book`, with an `action`; it calls the same service (`backend/app/services/books.py`) as the Book window.
