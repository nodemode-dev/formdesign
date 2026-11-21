# HASHOUT_LOG · src-tauri · FormDesign

Living log for the desktop shell. Source files stay comment-free where this log can say it; what they would have said lives here, indexed. Read it before editing `src-tauri/`. The shell cannot be compiled on the machines that built this entry (the system WebKit libraries are missing), so every change here is read against the pinned Tauri 2.12.1 source and not run.

---

## v0.93.1 · the shell's sources carry no comments; what they said is here (2026-11-20)

- refers-to: src/native.rs (module, `Native`, `FrameIn`, `FrameOut`, the five `native_canvas_*` commands); src/text_edit.rs (module); src/main.rs :: setup (the main window's builder); build.rs
- kind: docs
- logged: v0.93.1 · 2026-11-20

Thirty-seven comment lines and two blank lines were taken out of four files; no code token changed (the diff is deletions only, each a `//`, `///` or `//!` line or the blank line after a module comment). What they said, by place:

**`src/native.rs`, the module.** The opt-in native canvas of the desktop app: the shell hosts formdesign-gpu (wgpu: Metal, Vulkan, Direct3D 12) beside the web view, the way FormVis hosts formvis-render. Nothing is created until the page calls `native_canvas_start` (View > Renderer > Native), so the app is unchanged otherwise. The wgpu surface is the main window's own; the web view sits above it and is transparent (the window and web view are built transparent in `main.rs`, and the page makes its canvas rectangle transparent once the first frame has been shown). The surface covers the whole window and the frame is drawn at the canvas rectangle's offset, so the pasteboard, pages and tiles land exactly under the page's hole; whatever the surface shows elsewhere is hidden by the opaque interface above it. Not yet verified on macOS or Windows: on macOS the Metal layer must sit beneath the web view, and the transparent window needs the private API (`tauri.macos.conf.json`); on Windows the swap chain of a window that also hosts WebView2 may need a composition visual. Only the owner's machines can confirm them.

**`src/native.rs`, the fields of `Native`.** `rect` is the canvas rectangle in CSS pixels (x, y, w, h) within the web view, and `dpr` the device pixel ratio. `size` is the window size the surface was last configured for. `evicted` holds the tiles the atlas evicted since the last frame reply, so the page forgets it uploaded them.

**`src/native.rs`, `FrameIn`.** The frame as the page sends it: the view as [scale, tx, ty] (CSS pixels, relative to the canvas rectangle), the pages that may be visible, and the pasteboard colour as straight RGBA from 0 to 1.

**`src/native.rs`, `FrameOut`.** `presented` is true when the frame was drawn and shown (false: skipped, send it again). `missing` lists the visible tiles that are not on the GPU (drawn as white paper), for the page to send. `evicted` lists the tiles the atlas dropped to stay inside its budget since the last reply.

**`src/native.rs`, the commands.** `native_canvas_start` starts the native canvas in this window and answers false when no graphics adapter can draw into it. `native_canvas_resize` is called when the canvas rectangle or the pixel ratio changed; the surface follows the window's size at the next frame. `native_canvas_tile` takes one tile's pixels as the raw request body (straight RGBA8, w by h); the header `x-key` is "page|level|tx|ty" and `x-w` and `x-h` give the size, so tiles are not expanded into JSON. `native_canvas_frame` draws the frame and reports what is missing; it is async so the wait for the display's vertical blank is not on the thread that runs the web view. Inside it, the view adds the canvas rectangle's offset inside the window, so the canvas is drawn under its hole. `native_canvas_stop` drops the native canvas: the surface and every tile on the GPU.

**`src/text_edit.rs`, the module.** macOS: the menu bar's Edit items are FormDesign commands with Cmd+Z, Cmd+X, Cmd+C, Cmd+V and Cmd+A, so the menu takes those keys before a focused text field can. While a field has focus the page calls `text_edit` instead of the command, and the standard edit action goes to the first responder, the web view, which edits the field exactly as the context menu does, with no clipboard prompt.

**`src/main.rs`, the main window's builder.** `FORMDESIGN_RENDERER=native` starts the page on the native canvas (a test and support aid; the normal way is View > Renderer, which the page remembers). The window and its web view are transparent so the native canvas shows through the page's hole; the page paints every pixel itself unless that mode is on.

**`build.rs`.** The page is served by the local engine (a remote origin to Tauri), so its calls to the app's own commands need permissions: the build generates `allow-native-canvas-*` (and the others named in the list) for each command, granted in `capabilities/default.json`.

Checked here: the diff has no added line; `rustfmt --emit stdout` parses `build.rs` and `src/main.rs` with its modules. Not compiled (the system WebKit libraries are missing).

---

## v0.91.0 · the engine is held in a job object on Windows, the wait for its address has a limit, and a failed start names the engine's last lines and its log (2026-11-20)

- refers-to: src/win_job.rs; src/engine_start.rs; src/main.rs :: kill_backend, start_limit, engine_failed, setup; Cargo.toml and Cargo.lock (`windows-sys`, Windows only)
- kind: feat
- logged: v0.91.0 · 2026-11-20

The part held in 0.90.0, done the way FormVis does it, because this shell still starts its engine through tauri-plugin-shell: the start is not moved, and the job takes the engine by its process id. Not compiled as a whole here (the system WebKit libraries are missing); both new files were checked in a scratch crate for `x86_64-pc-windows-msvc` and `x86_64-pc-windows-gnu`, and the plain functions of `engine_start.rs` ran there as five tests. `main.rs` was read line by line against FormVis at `origin/main`.

**`win_job.rs`.** One job object for the app, made the first time it is needed, with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. `hold_pid` opens the engine by its id and puts it in the job; when the shell ends in any way Windows closes the job's last handle and ends the engine with it. `end` ends the job at once and is called by `kill_backend` before the plugin's own kill. A job that cannot be made or a process that cannot be opened leaves the start as it was; the engine's own watch of `FORMDESIGN_SHELL_PID` still covers it. `windows-sys` 0.61.2 was already in `Cargo.lock` through other crates, so the lock gains only the line that names it for the shell; `cargo metadata --locked --offline` resolves.

**`engine_start.rs`.** The sentences of a failed start, the last twelve lines the engine wrote (never the window key line, control characters taken out, 300 characters a line at most), its exit code, and the full path of its log, worked out the way the engine works it out (`FORMDESIGN_DATA_DIR`, `~` expanded, else `FormDesign` in the home folder, which is `USERPROFILE` on Windows and `HOME` elsewhere, then `Logs/formdesign-engine.log`). The sentences of 0.90.0 are kept: code 2 is another copy holding the data folder, any other code is named.

**The waits.** The reading loop now takes its events from a channel that the engine's events and three timers feed. The address is waited for 90 seconds (the owner's decision for every app); after the address the key is waited for ten seconds, as in 0.88.0; after the engine ends, one more second lets its last lines arrive before the dialog. A wait that runs out says how long it waited and for what.

---

## v0.90.0 · the shell tells the engine its process id, and a failed start shows its message before the last window closes (2026-11-19)

- refers-to: src/main.rs :: setup (the engine's start, the spawned task's Terminated arm, the two failure paths)
- kind: fix
- logged: v0.90.0 · 2026-11-19

The small part of the family's Windows pattern, the part the engine of this version cannot do without. Not compiled here and not run on any system; each line was read against tauri-plugin-shell 2.4 (`Command::env`, `TerminatedPayload.code`) and tauri-plugin-dialog 2 (`message`, `blocking_show`), which this file already used.

**`FORMDESIGN_SHELL_PID`.** The engine is started with the shell's own process id in that variable. The engine records it in its lock, watches that process and leaves within about two seconds of its ending, so an engine is no longer left behind when the shell is ended or when the kill reaches only the first of the engine's two processes. It must stay: a frozen engine that finds no such variable takes itself for started alone, opens the app and stops (backend log, 0.90.0), so a shell that did not set it would start itself again and again.

**The message comes first.** Both failure paths closed the loading window and then showed the dialog. On Windows the app can end when its last window closes, so the person saw nothing. Now the dialog is shown, and only after it returns the loading window is closed, the engine stopped and the app ended. A main window that fails to build takes the same path with its own sentence; before, it closed the loading window and said nothing.

**What the message says.** The exit code of an engine that stopped before it gave its address is kept from the `Terminated` event. Code 2 is the engine's refusal of a second copy, and the sentence says another copy of its engine holds its data folder. Any other code is named. With no code the old sentence stays. The last two say where the engine's log is.

**Held, not in this version:** the job object that ends the engine with the shell however the shell ends (`win_job.rs`, which wants a `std::process::Child` or the child's handle, and `windows-sys` named in `Cargo.toml` and `Cargo.lock`); `engine_start.rs` with the engine's last lines and the log's full path in the message; a limit on the wait for the engine's address, which is still unlimited; single-instance handling. Until the job object is in, ending the shell is covered by the engine's own watch.

---

## v0.88.0 · the shell waits at most ten seconds for the engine's key and opens no window without it (2026-11-19)

- refers-to: src/main.rs :: setup (the spawned task that reads the engine's first lines)
- kind: fix
- logged: v0.88.0 · 2026-11-19

The engine prints `formdesign-window-key: <key>` on stdout before its address, but its output and its log reach the shell's reading loop through two pipes with no order between them, so the key line can arrive just after the address. Once the address is known the shell therefore keeps reading for the key line for at most ten seconds (it was three), sleeping 20 milliseconds between empty reads. If the ten seconds run out with no key, the shell takes the same path as an engine that did not start: the splash closes, the dialog says the engine failed to start, the engine is stopped and the shell exits. A window that cannot write is never opened.

The three comment lines that explained the wait in `main.rs` (added in the 0.88.0 shell change) moved here; the source keeps none of them.

The loop that waits for the address itself is unchanged and still ends only on the engine's address or on the engine ending. That wait belongs to the shell's earlier behaviour and is left alone.
