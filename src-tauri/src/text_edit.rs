//! macOS: the menu bar's Edit items are FormDesign commands with Cmd+Z, Cmd+X, Cmd+C, Cmd+V and Cmd+A, so the menu takes
//! those keys before a focused text field can. While a field has focus the page calls this instead of the command, and
//! the standard edit action goes to the first responder, the web view, which edits the field exactly as the context
//! menu does, with no clipboard prompt.

#[tauri::command]
pub fn text_edit<R: tauri::Runtime>(window: tauri::Window<R>, action: String) -> Result<(), String> {
    let selector = match action.as_str() {
        "undo" => "undo:",
        "redo" => "redo:",
        "cut" => "cut:",
        "copy" => "copy:",
        "paste" => "paste:",
        "select-all" => "selectAll:",
        _ => return Err(format!("unknown edit action {action}")),
    };
    send(&window, selector);
    Ok(())
}

#[cfg(target_os = "macos")]
fn send<R: tauri::Runtime>(window: &tauri::Window<R>, selector: &'static str) {
    let _ = window.run_on_main_thread(move || unsafe {
        use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
        use objc2::msg_send;
        let Some(class) = AnyClass::get(c"NSApplication") else { return };
        let app: *mut AnyObject = msg_send![class, sharedApplication];
        let Some(app) = app.as_ref() else { return };
        let none: *mut AnyObject = std::ptr::null_mut();
        let _: Bool = msg_send![app, sendAction: Sel::register(&std::ffi::CString::new(selector).unwrap()), to: none, from: none];
    });
}

#[cfg(not(target_os = "macos"))]
fn send<R: tauri::Runtime>(_window: &tauri::Window<R>, _selector: &'static str) {}
