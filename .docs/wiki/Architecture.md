# Architecture

formdes is three processes with one owner of the truth. The layout core owns every document; the engine wraps it with everything a layout program needs that is not layout; the window and every agent are clients of the engine.

```
 window (frontend/, Vite)        agents (MCP over /mcp or ./formdes mcp)        CLI (./formdes make ...)
        │  WebSocket /api/ws + REST /api           │  JSON over HTTP                     │  in-process
        └────────────────────────────┬─────────────┴──────────────────────────────────────┘
                                     ▼
                      engine (backend/app, FastAPI on 8196)
          documents · intake · templates · Make pass · books · MCP
                                     │  JSON lines on standard input and output
                                     ▼
          layout core (native/formdes-core over formdes-engine, -text, -render, ...)
          212 commands · composer · renderer · tiles · overlays · slots · fonts
```

## The three layers

| Layer | Source | Owns |
| --- | --- | --- |
| Layout core | `native/formdes-core/src/` over `native/formdes-*` | Documents, selection, tools, text composition (Knuth-Plass), rendering, PDF, IDML and EPUB export, undo |
| Engine | `backend/app/` | The core process, `.fdes` packages, intake (including layouts made from PDFs), templates and brand kits, the Make pass, books, MCP, the CLI |
| Window | `frontend/src/` | Drawing tiles and overlays, sending pointer and key events, panels and dialogs; never layout logic |

## How a change travels

1. A person drags a frame. The window sends the pointer events over `/api/ws` (`pointer`), coalesced to one in flight at a time.
2. The engine passes them to the core, which runs the active tool and answers with the cursor, the tool and `wantsText`.
3. The core's change event carries a revision and damage rectangles. The engine pushes it to every window as `{type: "changed"}`.
4. The window invalidates only the 512 px tiles that intersect the damage, keeps the stale tiles drawn, and asks for replacements (`tiles`). Overlays (frame edges, handles, threads) are redrawn from the core's geometry (`overlays`).

An agent's `run_commands` call takes the same route from step 2, so its edits appear live in an open window.

## Why it is split this way

- **One engine, many clients.** The window, the CLI and MCP act on the same documents, so an agent can lay out while a person watches, and a person can fix what the agent left.
- **The layout engine is formdes's own.** formdes-core is a thin JSON-lines shell over the crates in `native/`: formdes-model (the document model and the `.fdoc` file), formdes-text (shaping, the Knuth and Plass composer, hyphenation, frames, tables, footnotes), formdes-render (raster and PDF), formdes-interchange (IDML, text import, HTML and EPUB) and formdes-engine (the session, undo, 212 commands, 36 tools, overlays, tiles). Engine changes go into `native/formdes-*` directly; everything that is not layout lives in the Python engine.
- **The window is thin.** It renders pixels the core made, so what you see is what exports.
- **Templates are data.** JSON specs compiled to core commands can be written by agents (`formdes-template-creator`) and re-skinned by brand kits without code.

## Where to read next

[[Layout-Core]] for the protocol, [[Engine-Service]] for the engine's modules, [[Editor]] for the window, [[Make-Pass]] for the automation.
