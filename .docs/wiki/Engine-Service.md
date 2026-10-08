# Engine Service

The engine is a FastAPI application on 127.0.0.1:8196. It supervises the layout core, keeps formdes's own metadata beside each core document, and adds everything a layout program needs that is not layout: reading files, templates, the Make pass, books, the PDF Reader, MCP and the command line. The window, agents and the CLI all go through it.

## Source

| Path | What lives there |
| --- | --- |
| `backend/app/main.py` | the app, its routers, the lifespan that starts the core and autosave, serving `frontend/dist` |
| `backend/app/core/config.py` | ports, the data folder, the core binary and font folder discovery |
| `backend/app/engine/core.py` | the formdes-core child process: calls, binary replies, change events, restart |
| `backend/app/engine/documents.py` | `.fdes` packages: new, open, save, export, recent files, autosave, adopting a made layout |
| `backend/app/intake/` | files and a brief into one content pack: `markdown.py`, `office.py`, `images.py`, `blocks.py`, `brief.py`, `pack.py`, `store.py` |
| `backend/app/templates/` | the template spec (`spec.py`), its compiler to core commands (`compile.py`), the registry, brand kits, the checker, the 71-file `library/` |
| `backend/app/autolayout/` | the Make pass: `content.py`, `rank.py`, `make.py`, `fill.py`, `slots.py`, `tagged.py` |
| `backend/app/services/pdfread.py` | the Reader: pages, text with boxes, search, outline, structure, extraction (pypdfium2) |
| `backend/app/services/books.py` | `.fdbook` books over the core's book commands |
| `backend/app/routers/` | the HTTP surface, one module per area, and `mcp.py` |
| `backend/app/cli.py` | `./formdes make`, `intake`, `export`, `templates`, `check-template` |
| `backend/formdes_engine.py` | the PyInstaller entry for the desktop app |

## Routes

| Area | Routes |
| --- | --- |
| System | `GET /api/version`, `/api/health`, `/api/capabilities` |
| Core | `POST /api/core/{method}` (JSON methods), `GET /api/core/page.png`, `GET /api/core/font`, the window socket `/api/ws` |
| Documents | `GET /api/documents`, `/api/documents/recent`; `POST /api/documents/new`, `open`, `upload`, `save`, `export`, `activate`, `close`; `POST /api/files` |
| Intake | `POST /api/intake`, `/api/intake/paths`; `GET /api/intake`, `/api/intake/{pack}`, `/api/intake/{pack}/image/{ref}` |
| Templates and brands | `GET /api/templates`, `/api/templates/rank`, `/api/templates/{id}`, its `thumb.png`; `POST /api/templates/{id}/new`, `/api/templates/validate`, `/api/templates`; `GET` and `POST /api/brands` |
| Make and slots | `POST /api/make`, `/api/make/quick`, `/api/make/preview`; `GET` and `POST /api/slots` |
| Reader | `POST /api/reader/open`, `upload`; `GET /api/reader/{doc}` with `page/{n}.png`, `text/{n}`, `plain/{n}`, `search`, `structure`; `POST /api/reader/{doc}/extract` |
| Books | `GET /api/books`, `/api/books/info`, `/api/books/export.pdf`; `POST /api/books`, `/api/books/edit`, `/api/books/paginate`, `/api/books/sync` |
| Find/Change queries | `GET`, `POST` and `DELETE /api/find/queries` (twelve built in, the rest one file each in `Queries/`) |
| Preflight profiles | `GET`, `POST` and `DELETE /api/preflight/profiles` (five built in, the rest one file each in `Preflight/`) |
| MCP | `POST /mcp` (streamable HTTP, stateless JSON), `GET /api/mcp/status` |

## How documents are kept

The core holds the document; the engine holds a `DocMeta` per document (template, brand, brief, content pack, slots, path). Saving writes a `.fdes` package (see [[Project-Format]]); `file.new`, `file.open`, `file.save` and `file.saveAs` sent as core commands are routed through the documents service so the metadata stays in step. Every 60 seconds, if any document has unsaved changes, the core's `file.recovery.save` writes recovery copies to `.cache/recovery/` in the data folder.

## Contract

- Every route that changes a document goes through the core, never around it, so undo, change events and every open window stay correct.
- A new core method must be listed in `JSON_METHODS` or `BINARY_METHODS` in `backend/app/routers/core_api.py`.
- A new MCP tool is added to `TOOLS` in `backend/app/routers/mcp.py`, then `./formdes agents sync` regenerates the harness skill's tool reference; CI fails on drift.
- Long work (books, exports) runs in the engine and reports in one reply; the window shows progress, the engine does not stream partial results.
- Python 3.12, dependencies pinned in `backend/requirements.txt` (and the unpinned macOS twin); vet licences before adding one.
- Read `backend/HASHOUT_LOG.md` before editing.
