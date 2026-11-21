#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine_start;
mod native;
mod reveal;
mod text_edit;
mod traffic_lights;
#[cfg(windows)]
mod win_job;

use std::sync::Mutex;

use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_updater::UpdaterExt;

struct Backend(Mutex<Option<CommandChild>>);

type EngineStep = Result<CommandEvent, Option<engine_start::Wait>>;

const FALLBACK_LOGICAL: (f64, f64) = (1440.0, 900.0);
const MIN_LOGICAL: (f64, f64) = (1100.0, 700.0);
const SCREEN_FRACTION: f64 = 0.9;

fn kill_backend(handle: &tauri::AppHandle) {
    if let Ok(mut guard) = handle.state::<Backend>().0.lock() {
        if let Some(child) = guard.take() {
            #[cfg(windows)]
            win_job::end();
            let _ = child.kill();
        }
    }
}

fn start_limit(late: &tauri::async_runtime::Sender<EngineStep>, limit: std::time::Duration, wait: Option<engine_start::Wait>) {
    let late = late.clone();
    std::thread::spawn(move || {
        std::thread::sleep(limit);
        let _ = late.blocking_send(Err(wait));
    });
}

fn engine_failed(handle: &tauri::AppHandle, failure: engine_start::Failure, tail: &engine_start::Tail) {
    let text = engine_start::message(&failure, &tail.lines(), engine_start::engine_log().as_deref());
    eprintln!("formdesign: {text}");
    let _ = handle
        .dialog()
        .message(text)
        .title(engine_start::APP)
        .kind(tauri_plugin_dialog::MessageDialogKind::Error)
        .blocking_show();
    close_splash(handle);
    kill_backend(handle);
    std::process::exit(1);
}

fn close_splash(handle: &tauri::AppHandle) {
    if let Some(splash) = handle.get_webview_window("splash") {
        let _ = splash.close();
    }
}

fn core_path() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = format!("formdesign-core{}", std::env::consts::EXE_SUFFIX);
    let p = dir.join(name);
    p.is_file().then(|| p.to_string_lossy().into_owned())
}

fn check_for_updates(handle: tauri::AppHandle) {
    let configured = handle
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|updater| updater.get("pubkey"))
        .and_then(|key| key.as_str())
        .map_or(false, |key| !key.is_empty());
    if !configured {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let Ok(updater) = handle.updater() else { return };
        let Ok(Some(update)) = updater.check().await else { return };
        let install = handle
            .dialog()
            .message(format!("FormDesign {} is available. Install it and restart?", update.version))
            .title("Update available")
            .buttons(MessageDialogButtons::OkCancel)
            .blocking_show();
        if install && update.download_and_install(|_, _| {}, || {}).await.is_ok() {
            kill_backend(&handle);
            handle.restart();
        }
    });
}

fn move_old_app_folders() {
    const OLD: &str = "ai.nodemode.formdes";
    const NEW: &str = "ai.nodemode.formdesign";
    let env = |key: &str| std::env::var_os(key).filter(|v| !v.is_empty()).map(std::path::PathBuf::from);
    let mut bases: Vec<std::path::PathBuf> = Vec::new();
    #[cfg(target_os = "macos")]
    if let Some(home) = env("HOME") {
        let library = home.join("Library");
        bases.extend([library.join("WebKit"), library.join("Application Support"), library.join("Caches")]);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(home) = env("HOME") {
        bases.push(env("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local").join("share")));
        bases.push(env("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")));
        bases.push(env("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache")));
    }
    #[cfg(windows)]
    bases.extend(["LOCALAPPDATA", "APPDATA"].into_iter().filter_map(env));
    for base in bases {
        let (old, new) = (base.join(OLD), base.join(NEW));
        if old.is_dir() && !new.exists() {
            let _ = std::fs::rename(&old, &new);
        }
    }
}

fn main() {
    reveal::mark_launch();
    move_old_app_folders();
    let app = tauri::Builder::default()
        .on_window_event(|window, event| {
            if window.label() == "main"
                && matches!(
                    event,
                    WindowEvent::Resized(_)
                        | WindowEvent::Focused(_)
                        | WindowEvent::ThemeChanged(_)
                        | WindowEvent::ScaleFactorChanged { .. }
                )
            {
                traffic_lights::place(window);
            }
        })
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Backend(Mutex::new(None)))
        .manage(native::NativeState::default())
        .invoke_handler(tauri::generate_handler![
            native::native_canvas_start,
            native::native_canvas_frame,
            native::native_canvas_tile,
            native::native_canvas_resize,
            native::native_canvas_stop,
            traffic_lights::set_window_title,
            reveal::app_ready,
            text_edit::text_edit
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            reveal::round_splash(&handle);
            let started = app.shell().sidecar("formdesign-engine").map_err(|_| engine_start::Failure::Missing).and_then(|engine| {
                let mut cmd = engine
                    .env("FORMDESIGN_PORT", "8196")
                    .env("FORMDESIGN_WINDOW_KEY_STDOUT", "1")
                    .env(engine_start::SHELL_ENV, std::process::id().to_string());
                if let Some(core) = core_path() {
                    cmd = cmd.env("FORMDESIGN_CORE", core);
                }
                cmd.spawn().map_err(|err| engine_start::Failure::NotStarted(err.to_string()))
            });
            let (mut rx, child) = match started {
                Ok(pair) => pair,
                Err(failure) => {
                    std::thread::spawn(move || engine_failed(&handle, failure, &engine_start::Tail::default()));
                    return Ok(());
                }
            };
            #[cfg(windows)]
            win_job::hold_pid(child.pid());
            if let Ok(mut guard) = app.state::<Backend>().0.lock() {
                guard.replace(child);
            }
            tauri::async_runtime::spawn(async move {
                let pattern = regex::Regex::new(r"http://127\.0\.0\.1:(\d+)").ok();
                let key_pattern = regex::Regex::new(r"formdesign-window-key: ([A-Za-z0-9_-]{32,})").ok();
                let (tx, mut events) = tauri::async_runtime::channel::<EngineStep>(64);
                let late = tx.clone();
                tauri::async_runtime::spawn(async move {
                    while let Some(event) = rx.recv().await {
                        if tx.send(Ok(event)).await.is_err() {
                            break;
                        }
                    }
                    let _ = tx.send(Err(None)).await;
                });
                start_limit(&late, engine_start::Wait::Address.limit(), Some(engine_start::Wait::Address));
                let mut port: Option<u16> = None;
                let mut key: Option<String> = None;
                let mut waiting = false;
                let mut stopped = false;
                let mut tail = engine_start::Tail::default();
                let mut failure = engine_start::Failure::Stopped(None);
                while let Some(step) = events.recv().await {
                    let text = match step {
                        Ok(CommandEvent::Stdout(bytes)) => {
                            let text = String::from_utf8_lossy(&bytes).into_owned();
                            if key.is_none() {
                                key = key_pattern.as_ref().and_then(|p| p.captures(&text)).map(|found| found[1].to_string());
                            }
                            text
                        }
                        Ok(CommandEvent::Stderr(bytes)) => String::from_utf8_lossy(&bytes).into_owned(),
                        Ok(CommandEvent::Terminated(ended)) => {
                            failure = engine_start::Failure::Stopped(ended.code);
                            stopped = true;
                            start_limit(&late, engine_start::EXIT_WAIT, None);
                            continue;
                        }
                        Ok(_) => continue,
                        Err(None) => break,
                        Err(Some(_)) if stopped => break,
                        Err(Some(engine_start::Wait::Address)) if port.is_some() => continue,
                        Err(Some(wait)) => {
                            failure = engine_start::Failure::TimedOut(wait);
                            break;
                        }
                    };
                    for line in text.lines() {
                        tail.push(line);
                    }
                    if port.is_none() {
                        port = pattern.as_ref().and_then(|p| p.captures(&text)).and_then(|found| found[1].parse::<u16>().ok());
                    }
                    if port.is_some() && key.is_some() {
                        break;
                    }
                    if port.is_some() && !waiting {
                        waiting = true;
                        start_limit(&late, engine_start::Wait::Key.limit(), Some(engine_start::Wait::Key));
                    }
                }
                drop(late);
                let origin = port.map(|found| format!("http://127.0.0.1:{found}")).unwrap_or_default();
                let key_script = match &key {
                    Some(value) => format!("if (window.location.origin === {origin:?}) {{ window.__FORMDESIGN_WINDOW_KEY = {value:?}; }}"),
                    None => String::new(),
                };
                let url = port.filter(|_| key.is_some()).and_then(|found| format!("http://127.0.0.1:{found}").parse::<tauri::Url>().ok());
                match url {
                    Some(url) => {
                        let (width, height) = handle
                            .primary_monitor()
                            .ok()
                            .flatten()
                            .map(|monitor| {
                                let size = monitor.size();
                                let scale = monitor.scale_factor();
                                ((size.width as f64 / scale) * SCREEN_FRACTION, (size.height as f64 / scale) * SCREEN_FRACTION)
                            })
                            .unwrap_or(FALLBACK_LOGICAL);
                        let preset = match std::env::var("FORMDESIGN_RENDERER").as_deref() {
                            Ok("native") => "window.__FORMDESIGN_RENDERER__ = 'native';",
                            _ => "",
                        };
                        let builder = WebviewWindowBuilder::new(&handle, "main", WebviewUrl::External(url))
                            .transparent(true)
                            .visible(reveal::BUILD_VISIBLE)
                            .background_color(reveal::BACKGROUND)
                            .initialization_script(preset)
                            .initialization_script(key_script)
                            .title("FormDesign")
                            .inner_size(width, height)
                            .min_inner_size(MIN_LOGICAL.0, MIN_LOGICAL.1)
                            .center();
                        #[cfg(target_os = "macos")]
                        let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
                        let builder = builder.on_page_load(|window, payload| {
                            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                                traffic_lights::place(&window.as_ref().window());
                            }
                        });
                        if let Ok(window) = builder.build() {
                            reveal::conceal(&window);
                            traffic_lights::place(&window.as_ref().window());
                        } else {
                            engine_failed(&handle, engine_start::Failure::Window, &engine_start::Tail::default());
                            return;
                        }
                        check_for_updates(handle.clone());
                    }
                    None => engine_failed(&handle, failure, &tail),
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!());
    let Ok(app) = app else {
        eprintln!("formdesign: the desktop shell could not start");
        std::process::exit(1);
    };
    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            kill_backend(handle);
        }
    });
}
