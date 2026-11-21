use formdesign_gpu::composite::{Frame, PageRect};
use formdesign_gpu::display::{Display, Presented};
use formdesign_gpu::{Gpu, TileAtlas, TileKey, TILE};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub struct Native {
    gpu: Gpu,
    display: Display,
    atlas: TileAtlas,
    rect: [f64; 4],
    dpr: f32,
    size: (u32, u32),
    evicted: Vec<TileKey>,
}

#[derive(Default)]
pub struct NativeState(pub Mutex<Option<Native>>);

const NOT_STARTED: &str = "The native canvas is not started.";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageIn {
    index: u32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameIn {
    view: [f32; 3],
    pages: Vec<PageIn>,
    pasteboard: [f32; 4],
}

#[derive(Serialize, Clone, Copy)]
pub struct KeyOut {
    page: u32,
    level: i8,
    tx: u32,
    ty: u32,
}

impl From<TileKey> for KeyOut {
    fn from(k: TileKey) -> KeyOut {
        KeyOut { page: k.page, level: k.level, tx: k.tx, ty: k.ty }
    }
}

#[derive(Serialize)]
pub struct FrameOut {
    presented: bool,
    missing: Vec<KeyOut>,
    evicted: Vec<KeyOut>,
}

fn size_of(window: &tauri::WebviewWindow) -> Result<(u32, u32), String> {
    let s = window.inner_size().map_err(|e| e.to_string())?;
    Ok((s.width.max(1), s.height.max(1)))
}

fn rect_of(rect: Vec<f64>) -> Result<[f64; 4], String> {
    <[f64; 4]>::try_from(rect).map_err(|_| "a rectangle is x, y, width, height".to_string())
}

#[tauri::command]
pub fn native_canvas_start(rect: Vec<f64>, dpr: f64, window: tauri::WebviewWindow, state: tauri::State<NativeState>) -> Result<bool, String> {
    let rect = rect_of(rect)?;
    eprintln!("formdesign: native canvas start requested at {rect:?}, dpr {dpr}");
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Ok(true);
    }
    let size = size_of(&window)?;
    let Some((gpu, surface)) = Gpu::for_window(window.clone()) else {
        eprintln!("formdesign: native canvas: no graphics adapter can draw into this window");
        return Ok(false);
    };
    let display = Display::new(&gpu, surface, size.0, size.1);
    eprintln!("formdesign: native canvas on {} ({:?})", gpu.info.name, gpu.info.backend);
    *guard = Some(Native { gpu, display, atlas: TileAtlas::with_system_budget(), rect, dpr: dpr.max(1.0) as f32, size, evicted: Vec::new() });
    Ok(true)
}

#[tauri::command]
pub fn native_canvas_resize(rect: Vec<f64>, dpr: f64, window: tauri::WebviewWindow, state: tauri::State<NativeState>) -> Result<(), String> {
    let rect = rect_of(rect)?;
    let size = size_of(&window)?;
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    let n = guard.as_mut().ok_or(NOT_STARTED)?;
    n.rect = rect;
    n.dpr = dpr.max(1.0) as f32;
    if n.size != size {
        n.display.resize(&n.gpu, size.0, size.1);
        n.size = size;
    }
    Ok(())
}

#[tauri::command]
pub fn native_canvas_tile(request: tauri::ipc::Request, state: tauri::State<NativeState>) -> Result<(), String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else { return Err("native_canvas_tile takes raw bytes".into()) };
    let header = |name: &str| request.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string).ok_or(format!("missing header {name}"));
    let number = |name: &str| header(name)?.parse::<u32>().map_err(|_| format!("bad header {name}"));
    let (w, h) = (number("x-w")?, number("x-h")?);
    let key = header("x-key")?;
    let mut parts = key.split('|');
    let mut next = || parts.next().ok_or_else(|| "x-key is page|level|tx|ty".to_string());
    let key = TileKey {
        page: next()?.parse().map_err(|_| "bad page in x-key")?,
        level: next()?.parse().map_err(|_| "bad level in x-key")?,
        tx: next()?.parse().map_err(|_| "bad tx in x-key")?,
        ty: next()?.parse().map_err(|_| "bad ty in x-key")?,
    };
    if w == 0 || h == 0 || w > TILE || h > TILE || bytes.len() != (w * h * 4) as usize {
        return Err("the pixels do not match a tile of at most 256 by 256".into());
    }
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    let n = guard.as_mut().ok_or(NOT_STARTED)?;
    let evicted = n.atlas.upload(&n.gpu, key, w, h, bytes);
    n.evicted.extend(evicted);
    Ok(())
}

#[tauri::command]
pub async fn native_canvas_frame(frame: FrameIn, window: tauri::WebviewWindow, state: tauri::State<'_, NativeState>) -> Result<FrameOut, String> {
    let size = size_of(&window)?;
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    let n = guard.as_mut().ok_or(NOT_STARTED)?;
    if n.size != size {
        n.display.resize(&n.gpu, size.0, size.1);
        n.size = size;
    }
    let [scale, tx, ty] = frame.view;
    let f = Frame {
        width: size.0,
        height: size.1,
        dpr: n.dpr,
        pasteboard: frame.pasteboard,
        view: [scale, tx + n.rect[0] as f32, ty + n.rect[1] as f32],
        pages: frame.pages.iter().map(|p| PageRect { index: p.index, x: p.x, y: p.y, w: p.w, h: p.h }).collect(),
    };
    let (shown, missing) = n.display.present(&n.gpu, &mut n.atlas, &f);
    Ok(FrameOut {
        presented: shown == Presented::Shown,
        missing: missing.into_iter().map(KeyOut::from).collect(),
        evicted: std::mem::take(&mut n.evicted).into_iter().map(KeyOut::from).collect(),
    })
}

#[tauri::command]
pub fn native_canvas_stop(state: tauri::State<NativeState>) {
    if let Ok(mut s) = state.0.lock() {
        *s = None;
    }
}
