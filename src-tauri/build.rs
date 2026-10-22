fn main() {
    // The page is served by the local engine (a remote origin to Tauri), so its calls to the app's own commands need
    // permissions: this generates allow-native-canvas-* for each, granted in capabilities/default.json.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
        "native_canvas_start",
        "native_canvas_frame",
        "native_canvas_tile",
        "native_canvas_resize",
        "native_canvas_stop",
        "set_window_title",
        "app_ready",
        "text_edit",
    ])))
    .expect("the tauri build failed");
}
