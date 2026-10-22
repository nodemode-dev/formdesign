# Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| The status bar stays on "Connecting to the engine" | the engine is not running, or not on 8196 | `./formdesign backend` in a terminal shows why; `./formdesign doctor` checks the ports |
| "the layout core is not built: run ./formdesign install-core" | formdesign-core was never built | `./formdesign install-core` (cargo build, a few minutes the first time) |
| `install-core` fails to download without Rust | there is no published release for this version, or no prebuilt core for this machine (Linux arm64 has none) | install Rust from https://rustup.rs and rerun; or publish the draft `formdesign-v[version]` release on formcompiler |
| Templates render in fallback fonts | the font library was not converted | `./formdesign install-fonts` |
| Port 5196 or 8196 is busy | another FormDesign, or another program | stop it, or set `FORMDESIGN_UI_PORT` and `FORMDESIGN_ENGINE_PORT` (dev) or `FORMDESIGN_PORT` (engine) |
| `install-desktop` stops on Linux | the webview libraries are missing | `sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev patchelf build-essential libssl-dev` |
| The bash scripts fail on Windows | they expect `venv/bin`; Windows virtual environments use `venv\Scripts` | use `formdesign.bat`, which every command has a twin for |
| The macOS app will not open | it is unsigned | right-click the app, Open, then Open again |
| The desktop app shows "The FormDesign engine failed to start" | the frozen engine crashed before it reported a port | run `src-tauri/binaries/formdesign-engine-[triple] --selfcheck`; a `false` means a missing `--add-data` in the freeze |
| The first letters typed into a new frame go missing | fixed in 0.6.1 | update |
| A chapter will not number or sync in a book | it is open with unsaved changes | save it (the message names it), then run again |
| Before 0.11.0: panels or tools went blank after a window closed in the middle of a call, and answers looked like the previous request's | the engine read the closed request's reply as the next one's | fixed in 0.11.0 (replies are matched by id); restart the engine on older versions |
| A language is not checked for spelling | no dictionary for it in `Dictionaries/` | Edit > Language Dictionaries > Get, or add a Hunspell `.aff` and `.dic` pair |
| Proofing against a printer's profile shows no change after a restart | the profile was loaded before 0.11.0 kept profiles | load it again in View > Proof Setup > Custom; it is kept in `Profiles/` from now on |
| An agent sees "unknown core method" | the method is not listed in `JSON_METHODS` | add it in `backend/app/routers/core_api.py` |
| CI fails on `agents sync --check` | an MCP tool or a built-in template changed without a sync | run `./formdesign agents sync` and commit the result |
| CI fails on the em dash or emoji scan | one slipped into tracked text | replace it (an interpunct, a colon, a comma, or a rewrite); in code, use a `\N{EM DASH}` name |

## Logs

| Process | Where |
| --- | --- |
| Engine | its terminal, prefixed `[formdesign]`; the core's warnings arrive there prefixed `[formdesign-core]` |
| Window | the browser's developer console |
| Desktop app | run it from a terminal to see the engine's output |

## Recovery

If FormDesign stops with unsaved work, File > Recover Unsaved Documents reopens the recovery copies written every minute to `~/FormDesign/.cache/recovery/`. They open unsaved; save each one to keep it.
