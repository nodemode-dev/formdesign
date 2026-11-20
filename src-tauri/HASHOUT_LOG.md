# HASHOUT_LOG · src-tauri · FormDesign

Living log for the desktop shell. Source files stay comment-free where this log can say it; what they would have said lives here, indexed. Read it before editing `src-tauri/`. The shell cannot be compiled on the machines that built this entry (the system WebKit libraries are missing), so every change here is read against the pinned Tauri 2.12.1 source and not run.

---

## v0.88.0 · the shell waits at most ten seconds for the engine's key and opens no window without it (2026-11-19)

- refers-to: src/main.rs :: setup (the spawned task that reads the engine's first lines)
- kind: fix
- logged: v0.88.0 · 2026-11-19

The engine prints `formdesign-window-key: <key>` on stdout before its address, but its output and its log reach the shell's reading loop through two pipes with no order between them, so the key line can arrive just after the address. Once the address is known the shell therefore keeps reading for the key line for at most ten seconds (it was three), sleeping 20 milliseconds between empty reads. If the ten seconds run out with no key, the shell takes the same path as an engine that did not start: the splash closes, the dialog says the engine failed to start, the engine is stopped and the shell exits. A window that cannot write is never opened.

The three comment lines that explained the wait in `main.rs` (added in the 0.88.0 shell change) moved here; the source keeps none of them.

The loop that waits for the address itself is unchanged and still ends only on the engine's address or on the engine ending. That wait belongs to the shell's earlier behaviour and is left alone.
