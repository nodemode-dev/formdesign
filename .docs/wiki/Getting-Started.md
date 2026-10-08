# Getting Started

## Requirements

| Tool | Version | Why |
| --- | --- | --- |
| Node | 20.19 or newer (22 recommended) | the Vite window |
| Python | 3.12 or newer | the engine |
| Rust | stable (1.90 or newer) | the layout core; without it `install-core` downloads a prebuilt core from a published release |

Linux desktop builds also need the webview packages: `sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev patchelf`.

## Install and boot

```bash
git clone git@github.com:nodemode-ai/formdes.git
cd formdes
./formdes install      # engine venv, font library, layout core, frontend
./formdes boot         # engine on 8196 and the window on 5196 together
```

On Windows use `formdes.bat install` and `formdes.bat boot`; every command below has the same name there. The first install compiles the layout engine, which takes a few minutes. `./formdes doctor` checks the toolchain, the installs and the ports.

## Every command

| Command | What it does |
| --- | --- |
| `./formdes` or `boot` | engine (8196) and window (5196) together |
| `install [--lite]` | engine, fonts, core and frontend; `--lite` keeps a core that is already built |
| `install-core` | build formdes-core with cargo, or download the prebuilt core for this version |
| `install-fonts` | convert the bundled OFL font library into `backend/app/fonts` |
| `install-desktop` | PyInstaller, and a check of Rust and the webview libraries |
| `backend`, `frontend` | one side only |
| `build`, `serve` | build the window into `frontend/dist`; serve it from the engine on 8196 |
| `freeze` | freeze the engine and copy it and the core into `src-tauri/binaries` |
| `desktop dev`, `desktop build` | run or bundle the desktop app |
| `make`, `templates` | the command-line Make pass and the template list (see [[Agents-and-MCP]]) |
| `mcp` | MCP over standard input and output, for Claude Desktop and other MCP clients |
| `agents install`, `agents sync [--check]` | set up Claude Code, Codex and Cursor; regenerate the harness skill's references |
| `doctor`, `counts [--check]`, `clean [--all]`, `help` | checks, README code sizes, clean-up, usage |

## Ports

| Service | Address | Notes |
| --- | --- | --- |
| Window (dev) | http://localhost:5196 | Vite, `host: false`, strict port; proxies `/api` and `/mcp` to the engine |
| Engine | http://127.0.0.1:8196 | REST under `/api`, the window socket at `/api/ws`, MCP at `/mcp` |
| Preview | http://localhost:5197 | `vite preview` of the production build |
| Layout core | standard input and output | started and supervised by the engine |

`FORMDES_UI_PORT` and `FORMDES_ENGINE_PORT` move the dev ports; `FORMDES_PORT` moves the engine.

## The data folder

Everything a person makes lives in `~/Formdes` (or `FORMDES_DATA_DIR`):

| Folder | Holds |
| --- | --- |
| `Projects/` | saved `.fdes` documents; `Projects/Books/` holds `.fdbook` books |
| `Templates/` | your own templates, any subfolder |
| `Brands/` | brand kits as JSON |
| `Queries/` | your saved Find/Change queries |
| `Preflight/` | your preflight profiles |
| `Intake/` | content packs, uploads, PDFs opened in the Reader |
| `Exports/` | exported files, book PDFs |
| `Fonts/` | fonts you add |
| `.cache/` | thumbnails, placeholder art, recovery copies |

The engine reads `backend/.env` (never committed; `backend/.env.example` lists every variable).

## First layout in a minute

```bash
./formdes make article.md photo.jpg --brief "a two page newsletter for the Elm Street garden club" --out newsletter.pdf
```

Or open http://localhost:5196, press Make a layout, drop the same files, write the brief, press Analyse and then Make layout.
