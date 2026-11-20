# HASHOUT_LOG · src-tauri · FormDesign

Living log for the desktop shell. Source files stay comment-free where this log can say it; what they would have said lives here, indexed. Read it before editing `src-tauri/`. The shell cannot be compiled on the machines that built this entry (the system WebKit libraries are missing), so every change here is read against the pinned Tauri 2.12.1 source and not run.

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
