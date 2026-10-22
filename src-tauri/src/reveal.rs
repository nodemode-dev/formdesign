use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tauri::{Manager, Runtime, WebviewWindow};

pub const SPLASH_MIN: Duration = Duration::from_millis(2600);
pub const BACKGROUND: tauri::window::Color = tauri::window::Color(10, 10, 10, 255);
const BAR_FINISH: Duration = Duration::from_millis(220);
const FALLBACK: Duration = Duration::from_secs(20);

static LAUNCH: OnceLock<Instant> = OnceLock::new();
static REVEALED: AtomicBool = AtomicBool::new(false);

pub fn mark_launch() {
    let _ = LAUNCH.set(Instant::now());
}

pub const BUILD_VISIBLE: bool = !cfg!(target_os = "macos");

pub fn conceal<R: Runtime>(window: &WebviewWindow<R>) {
    if !BUILD_VISIBLE {
        set_seen(window, false);
        let _ = window.show();
    }
    let handle = window.app_handle().clone();
    std::thread::spawn(move || {
        std::thread::sleep(FALLBACK);
        show_main(&handle);
    });
}

#[tauri::command(async)]
pub fn app_ready<R: Runtime>(app: tauri::AppHandle<R>) {
    show_main(&app);
}

fn show_main<R: Runtime>(app: &tauri::AppHandle<R>) {
    if REVEALED.swap(true, Ordering::SeqCst) {
        return;
    }
    let started = *LAUNCH.get_or_init(Instant::now);
    if let Some(wait) = SPLASH_MIN.checked_sub(started.elapsed()) {
        std::thread::sleep(wait);
    }
    if let Some(splash) = app.get_webview_window("splash") {
        let _ = splash.eval("window.formdesignSplashDone && window.formdesignSplashDone()");
        std::thread::sleep(BAR_FINISH);
    }
    if let Some(main) = app.get_webview_window("main") {
        set_seen(&main, true);
        let _ = main.show();
        let _ = main.set_focus();
        crate::traffic_lights::place(&main.as_ref().window());
    }
    if let Some(splash) = app.get_webview_window("splash") {
        let _ = splash.close();
    }
}

#[cfg(target_os = "macos")]
fn set_seen<R: Runtime>(window: &WebviewWindow<R>, seen: bool) {
    let Ok(ns_window) = window.ns_window() else { return };
    let address = ns_window as usize;
    let _ = window.run_on_main_thread(move || unsafe {
        if let Some(window) = (address as *const objc2_app_kit::NSWindow).as_ref() {
            window.setAlphaValue(if seen { 1.0 } else { 0.0 });
            window.setIgnoresMouseEvents(!seen);
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn set_seen<R: Runtime>(_window: &WebviewWindow<R>, _seen: bool) {}

#[cfg(target_os = "macos")]
pub fn round_splash<R: Runtime>(app: &tauri::AppHandle<R>) {
    const RADIUS: f64 = 16.0;
    let Some(splash) = app.get_webview_window("splash") else { return };
    let Ok(ns_window) = splash.ns_window() else { return };
    let address = ns_window as usize;
    let _ = splash.run_on_main_thread(move || unsafe {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        let Some(window) = (address as *const objc2_app_kit::NSWindow).as_ref() else { return };
        window.setOpaque(false);
        window.setBackgroundColor(Some(&objc2_app_kit::NSColor::clearColor()));
        let Some(content) = window.contentView() else { return };
        content.setWantsLayer(true);
        let layer: *mut AnyObject = msg_send![&*content, layer];
        if let Some(layer) = layer.as_ref() {
            let _: () = msg_send![layer, setCornerRadius: RADIUS];
            let _: () = msg_send![layer, setMasksToBounds: true];
        }
        window.invalidateShadow();
    });
    let _ = splash.show();
}

#[cfg(not(target_os = "macos"))]
pub fn round_splash<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(splash) = app.get_webview_window("splash") {
        let _ = splash.show();
    }
}
