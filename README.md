<p align="center"><img src="assets/formdes_folder_icon.png" width="112" alt="formdes"></p>
<h1 align="center">formdes</h1>
<p align="center"><b>Page layout and publishing for the web and the desktop, made for people and their agents, in the NodeMode design system.</b></p>
<p align="center">
<img src="https://img.shields.io/badge/version-0.1.0-f97316" alt="version 0.1.0">
<img src="https://img.shields.io/badge/node-22-339933" alt="node 22">
<img src="https://img.shields.io/badge/python-3.12-3776AB" alt="python 3.12">
<img src="https://img.shields.io/badge/rust-1.90-B7410E" alt="rust 1.90">
<img src="https://img.shields.io/badge/tauri-2-24C8DB" alt="tauri 2">
<img src="https://img.shields.io/badge/license-Apache--2.0-lightgrey" alt="Apache 2.0 license">
</p>

## What it is

formdes is the form family's page-layout application: an alternative to InDesign for magazines, newsletters, books, reports, print pieces, ads, websites and blogs. Its layout core is formdes's own engine (InDesign-class commands: spreads and parent pages, threaded stories, a Knuth-Plass paragraph composer, styles, tables, PDF, IDML and EPUB), wrapped by formdes's own interface in formvis's Studio Dark language and by an engine service built for agents: drop in Markdown, PDFs, Word files, images and a brief, pick a template, and formdes lays it out.

**Stack.** Vanilla TypeScript and Vite · Python and FastAPI · Rust (formdes-core, a JSON-lines sidecar over formdes's own layout crates) · Tauri 2.

## Status at 0.1.0

The foundation: the dispatcher and setup scripts, formdes-core answering the engine's 432 commands with viewport tiles, overlays and live change events, the FastAPI engine managing it with a window socket, and the shell booting in Studio Dark. The intake, templates, the Make pass, the agent harness and the editor follow in the next releases; `DEV_PLAN.md` in the planning sidecar lists them.

## Quick Start

Requirements: Node 20.19 or newer (22 recommended), Python 3.12 or newer, Rust stable (for the layout core; without it the installer fetches a prebuilt core when a release has one), git.

### macOS and Linux

```bash
git clone https://github.com/nodemode-ai/formdes.git
cd formdes
./formdes install
./formdes boot
```

### Windows

```bat
git clone https://github.com/nodemode-ai/formdes.git
cd formdes
formdes.bat install
formdes.bat boot
```

The first `install` compiles the layout core (a few minutes on the first build). `./formdes help` lists every command; `./formdes doctor` checks the toolchain, the installs and the ports.

| Service | URL | Notes |
| --- | --- | --- |
| Frontend (dev) | http://localhost:5196 | Vite, bound to localhost only, strict port |
| Engine | http://127.0.0.1:8196 | FastAPI; API under `/api`, the window socket at `/api/ws`; serves the built app in `./formdes serve` |
| Preview | http://localhost:5197 | `vite preview` of the production build |
| Layout core | standard input and output | `formdes-core`, started and supervised by the engine |

## Configuration

The engine reads `backend/.env`, which is never committed; `backend/.env.example` is the template.

| Variable | Purpose |
| --- | --- |
| `FORMDES_HOST` | Engine bind address |
| `FORMDES_PORT` | Engine port |
| `FORMDES_DATA_DIR` | Projects, templates, brand kits, intake and exports (default `~/Formdes`) |
| `FORMDES_CORE` | Path to a formdes-core binary to use instead of the built one |
| `FORMDES_FONT_DIRS` | Extra font folders for the layout core |
| `FORMDES_SYSTEM_FONTS` | `0` keeps the core from cataloguing the system's fonts |
| `FORMDES_CORS_ORIGINS` | Allowed browser origins for the dev frontend |
| `FORMDES_DIST` | Built frontend folder the engine serves |

## Architecture

```text
formdes/
├── formdes, formdes.bat           root dispatcher: install, boot, build, serve, desktop, make, mcp, agents, doctor
├── formdes_oc_install(.bat)       one-command install
├── setup/                         one script per task, each with a .bat twin; lib.sh holds shared helpers
├── assets/                        repo art (README logo, desktop icon source)
├── native/                        Rust workspace: formdes-core over formdes's own layout crates
├── backend/                       FastAPI engine: the core process, intake, templates, the Make pass, MCP
└── frontend/                      Vite app, vanilla TypeScript: the shell, the canvas, panels and dialogs
```

## Development

Branch model: `main` plus short-lived work branches. Contributions follow the dev-rules laws: single-region hand edits, zero DOM flash or re-render, no emoji, no em dash, no icon-library dependencies, `v*.*.*` versions, Conventional Commits. Read `frontend/HASHOUT_LOG.md`, `backend/HASHOUT_LOG.md` and `native/HASHOUT_LOG.md` before editing.

## Contributors

- **nodemode-ai** · owner and build

## Licensing

Apache License 2.0. Copyright 2026 nodemode-ai. See `LICENSE` and `NOTICE.md`.
