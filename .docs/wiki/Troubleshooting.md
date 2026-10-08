# Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| The status bar stays on "Connecting to the engine" | the engine is not running, or not on 8196 | `./formdes backend` in a terminal shows why; `./formdes doctor` checks the ports |
| "the layout core is not built: run ./formdes install-core" | formdes-core was never built | `./formdes install-core` (cargo build, a few minutes the first time) |
| `install-core` fails to download without Rust | there is no published release for this version, or no prebuilt core for this machine (Linux arm64 has none) | install Rust from https://rustup.rs and rerun; or publish the draft release |
| Templates render in fallback fonts | the font library was not converted | `./formdes install-fonts` |
| Port 5196 or 8196 is busy | another formdes, or another program | stop it, or set `FORMDES_UI_PORT` and `FORMDES_ENGINE_PORT` (dev) or `FORMDES_PORT` (engine) |
| `install-desktop` stops on Linux | the webview libraries are missing | `sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev patchelf build-essential libssl-dev` |
| The bash scripts fail on Windows | they expect `venv/bin`; Windows virtual environments use `venv\Scripts` | use `formdes.bat`, which every command has a twin for |
| The macOS app will not open | it is unsigned | right-click the app, Open, then Open again |
| The desktop app shows "The formdes engine failed to start" | the frozen engine crashed before it reported a port | run `src-tauri/binaries/formdes-engine-[triple] --selfcheck`; a `false` means a missing `--add-data` in the freeze |
| The first letters typed into a new frame go missing | fixed in 0.6.1 | update |
| Drop caps or small caps do not show | the composer does not render them yet | use a larger first character style, or real small-cap fonts |
| Text beside a wrapped object sets its last line wrongly | a known text wrap issue | nudge the object or the frame |
| A chapter will not number or sync in a book | it is open with unsaved changes | save it (the message names it), then run again |
| An agent sees "unknown core method" | the method is not listed in `JSON_METHODS` | add it in `backend/app/routers/core_api.py` |
| CI fails on `agents sync --check` | an MCP tool or a built-in template changed without a sync | run `./formdes agents sync` and commit the result |
| CI fails on the em dash or emoji scan | one slipped into tracked text | replace it (an interpunct, a colon, a comma, or a rewrite); in code, use a `\N{EM DASH}` name |

## Logs

| Process | Where |
| --- | --- |
| Engine | its terminal, prefixed `[formdes]`; the core's warnings arrive there prefixed `[formdes-core]` |
| Window | the browser's developer console |
| Desktop app | run it from a terminal to see the engine's output |

## Recovery

If formdes stops with unsaved work, File > Recover Unsaved Documents reopens the recovery copies written every minute to `~/Formdes/.cache/recovery/`. They open unsaved; save each one to keep it.
