pub mod draw;
pub mod geo;
pub mod misc;
pub mod select;
pub mod text;

use std::sync::Arc;

use formdes_model::doc::Doc;
use formdes_model::geom::{Affine, BezPath, Point, Rect};
use formdes_model::ids::ItemId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canvas::SpreadRef;
use crate::session::{Selection, Session};
use crate::{EngineError, Result};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub cmd: bool,
    pub ctrl: bool,
    pub space: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Down,
    Drag,
    Up,
    Move,
    Double,
}

#[derive(Clone, Copy, Debug)]
pub struct Ev {
    pub kind: Kind,
    pub pos: Point,
    pub mods: Mods,
}

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub zoom: f64,
    pub snap: bool,
}

impl View {
    pub fn tol(&self, px: f64) -> f64 {
        px / self.zoom.max(0.01)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Enter,
    Escape,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
}

pub fn key_named(name: &str) -> Option<Key> {
    Some(match name.to_ascii_lowercase().as_str() {
        "enter" | "return" => Key::Enter,
        "escape" | "esc" => Key::Escape,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "arrowup" | "up" => Key::Up,
        "arrowdown" | "down" => Key::Down,
        "arrowleft" | "left" => Key::Left,
        "arrowright" | "right" => Key::Right,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "tab" => Key::Tab,
        _ => return None,
    })
}

#[derive(Clone, Debug)]
pub enum Overlay {
    Marquee(Rect),
    Path { path: BezPath, color: String, dashed: bool },
    Line { a: Point, b: Point, color: String, dashed: bool },
    Measure { p: Point, text: String },
    Guide { a: Point, b: Point },
    Gap { a: Point, b: Point, label: String },
}

pub struct ToolDef {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub shortcut: Option<&'static str>,
}

const fn t(id: &'static str, label: &'static str, icon: &'static str, shortcut: Option<&'static str>) -> ToolDef {
    ToolDef { id, label, icon, shortcut }
}

pub static GROUPS: &[&[ToolDef]] = &[
    &[t("selection", "Selection Tool", "tool-selection", Some("V")), t("directSelection", "Direct Selection Tool", "tool-direct", Some("A"))],
    &[t("page", "Page Tool", "tool-page", Some("Shift+P"))],
    &[t("gap", "Gap Tool", "tool-gap", Some("U"))],
    &[t("contentCollector", "Content Collector Tool", "tool-collector", Some("B")), t("contentPlacer", "Content Placer Tool", "tool-placer", None)],
    &[],
    &[t("type", "Type Tool", "tool-type", Some("T")), t("typeOnPath", "Type on a Path Tool", "tool-type-path", Some("Shift+T")), t("typeVertical", "Vertical Type Tool", "tool-type-vertical", None)],
    &[t("line", "Line Tool", "tool-line", Some("\\"))],
    &[t("pen", "Pen Tool", "tool-pen", Some("P")), t("curvature", "Curvature Tool", "tool-pen", Some("Shift+`")),t("addAnchor", "Add Anchor Point Tool", "tool-pen-add", Some("=")), t("deleteAnchor", "Delete Anchor Point Tool", "tool-pen-delete", Some("-")), t("convertDirection", "Convert Direction Point Tool", "tool-anchor", Some("Shift+C"))],
    &[t("pencil", "Pencil Tool", "tool-pencil", Some("N")), t("smooth", "Smooth Tool", "tool-smooth", None), t("erase", "Erase Tool", "tool-erase", None)],
    &[t("rectangleFrame", "Rectangle Frame Tool", "tool-rect-frame", Some("F")), t("ellipseFrame", "Ellipse Frame Tool", "tool-ellipse-frame", None), t("polygonFrame", "Polygon Frame Tool", "tool-polygon-frame", None)],
    &[t("rectangle", "Rectangle Tool", "tool-rect", Some("M")), t("ellipse", "Ellipse Tool", "tool-ellipse", Some("L")), t("polygon", "Polygon Tool", "tool-polygon", None)],
    &[],
    &[t("scissors", "Scissors Tool", "tool-scissors", Some("C"))],
    &[t("freeTransform", "Free Transform Tool", "tool-free-transform", Some("E")), t("rotate", "Rotate Tool", "tool-rotate", Some("R")), t("scale", "Scale Tool", "tool-scale", Some("S")), t("shear", "Shear Tool", "tool-shear", Some("O"))],
    &[t("gradientSwatch", "Gradient Swatch Tool", "tool-gradient", Some("G")), t("gradientFeather", "Gradient Feather Tool", "tool-gradient-feather", Some("Shift+G"))],
    &[],
    &[t("note", "Note Tool", "tool-note", None)],
    &[t("colorTheme", "Color Theme Tool", "tool-color-theme", Some("Shift+I")), t("eyedropper", "Eyedropper Tool", "tool-eyedropper", Some("I")), t("measure", "Measure Tool", "tool-measure", Some("K"))],
    &[],
    &[t("hand", "Hand Tool", "tool-hand", Some("H"))],
    &[t("zoom", "Zoom Tool", "tool-zoom", Some("Z"))],
];

pub fn tool_def(id: &str) -> Option<&'static ToolDef> {
    GROUPS.iter().flat_map(|g| g.iter()).find(|t| t.id == id)
}

pub fn tools_json() -> Value {
    Value::Array(GROUPS.iter().map(|g| Value::Array(g.iter().map(|t| json!({"id": t.id, "label": t.label, "icon": t.icon, "shortcut": t.shortcut})).collect())).collect())
}

#[derive(Clone)]
pub struct Before {
    pub uid: u64,
    pub doc: Arc<Doc>,
    pub sel: Selection,
}

#[derive(Clone, Debug)]
pub enum G {
    Marquee { add: bool },
    Move { ids: Vec<ItemId>, dup: bool, copies: Vec<ItemId> },
    Resize { ids: Vec<ItemId>, handle: usize, bounds: Rect },
    Rotate { ids: Vec<ItemId>, center: Point },
    Scale { ids: Vec<ItemId>, center: Point },
    Shear { ids: Vec<ItemId>, center: Point },
    Create { tool: String },
    TextSelect { story: formdes_model::ids::StoryId, anchor: usize },
    Thread { from: ItemId, loaded: bool },
    Anchor { item: ItemId, sub: usize, idx: usize, part: u8 },
    Corner { item: ItemId, flat: usize, flats: Vec<usize> },
    Content { item: ItemId },
    Pen { idx: usize },
    Pencil { points: Vec<Point> },
    Measure,
    Gradient { ids: Vec<ItemId> },
    Gap { a: Vec<ItemId>, b: Vec<ItemId>, vertical: bool, edge: f64 },
    Place,
    None,
}

#[derive(Clone)]
pub struct Gesture {
    pub kind: G,
    pub start: Point,
    pub start_sp: Point,
    pub slot: SpreadRef,
    pub slot_xf: Affine,
    pub before: Option<Before>,
    pub moved: bool,
    pub mods: Mods,
}

#[derive(Clone, Default)]
pub struct Pen {
    pub slot: Option<SpreadRef>,
    pub anchors: Vec<(Point, Point, Point)>,
    pub corners: Vec<usize>,
}

#[derive(Default, Clone)]
pub struct ToolState {
    pub gesture: Option<Gesture>,
    pub pen: Pen,
    pub sample: Option<Value>,
    pub overlays: Vec<Overlay>,
    pub conveyor: Vec<ItemId>,
    pub page: Option<usize>,
    pub thread: Option<ItemId>,
}

impl Session {
    pub fn tool_id(&self) -> &str {
        &self.tool
    }

    pub fn tool_busy(&self) -> bool {
        self.tools.gesture.is_some() || !self.tools.pen.anchors.is_empty() || self.tools.thread.is_some()
    }

    pub fn wants_text(&self) -> bool {
        self.active().is_some_and(|s| s.selection.text.is_some() || s.selection.cell_text.is_some())
    }

    pub fn set_tool(&mut self, id: &str) {
        let id = if id == "place" && self.place_gun.is_empty() { "selection" } else { id };
        if id != "place" && tool_def(id).is_none() {
            return;
        }
        if self.tool != id {
            draw::finish_pen(self, false);
            self.tools.gesture = None;
            self.tools.thread = None;
            self.tools.overlays.clear();
            if !matches!(id, "type" | "typeOnPath" | "typeVertical" | "note")
                && let Some(st) = self.active_mut()
                && let Some(t) = st.selection.text
            {
                let frame = st.doc.story(t.story).and_then(|s| s.frames.first().copied());
                st.selection = Selection::items(frame.into_iter().collect());
            }
            self.tool = id.to_string();
            self.sel_rev += 1;
        }
    }

    pub fn begin_edit(&mut self) -> Option<Before> {
        let st = self.active()?;
        Some(Before { uid: st.uid, doc: st.doc.clone(), sel: st.selection.clone() })
    }

    pub fn preview(&mut self, before: &Before, f: impl FnOnce(&mut Doc)) {
        if let Some(st) = self.active_mut()
            && st.uid == before.uid
        {
            let mut doc = before.doc.clone();
            f(Arc::make_mut(&mut doc));
            st.doc = doc;
            st.revision += 1;
        }
    }

    pub fn edit_now(&mut self, label: &str, f: impl FnOnce(&mut Doc, &mut Selection)) {
        let Some(before) = self.begin_edit() else { return };
        if let Some(st) = self.active_mut() {
            let mut doc = st.doc.clone();
            let mut sel = st.selection.clone();
            f(Arc::make_mut(&mut doc), &mut sel);
            st.doc = doc;
            st.selection = sel;
            st.revision += 1;
        }
        self.commit(&before, label);
        self.sel_rev += 1;
    }

    pub fn commit(&mut self, before: &Before, label: &str) {
        if let Some(st) = self.active_mut()
            && st.uid == before.uid
            && !Arc::ptr_eq(&st.doc, &before.doc)
        {
            st.history.undo.push(crate::session::Snapshot { label: label.to_string(), doc: before.doc.clone(), selection: before.sel.clone() });
            if st.history.undo.len() > crate::session::UNDO_LIMIT {
                st.history.undo.remove(0);
            }
            st.history.redo.clear();
            st.coalesce = None;
        }
    }

    pub fn restore(&mut self, before: &Before) {
        if let Some(st) = self.active_mut()
            && st.uid == before.uid
            && !Arc::ptr_eq(&st.doc, &before.doc)
        {
            st.doc = before.doc.clone();
            st.revision += 1;
        }
    }

    pub fn select(&mut self, sel: Selection) {
        if let Some(st) = self.active_mut() {
            st.selection = sel;
            st.coalesce = None;
            st.pending.clear();
        }
        self.sel_rev += 1;
    }

    pub fn pointer(&mut self, ev: &Ev, view: View) -> Result<()> {
        if self.active().is_none() {
            return Err(EngineError::new("no document open"));
        }
        let tool = self.tool.clone();
        match tool.as_str() {
            "selection" | "freeTransform" => select::selection(self, ev, view, tool == "freeTransform"),
            "directSelection" => select::direct(self, ev, view),
            "rotate" | "scale" | "shear" => select::transform_tool(self, ev, view, &tool),
            "type" | "typeVertical" => text::type_tool(self, ev, view),
            "typeOnPath" => text::type_on_path(self, ev, view),
            "rectangleFrame" | "ellipseFrame" | "polygonFrame" | "rectangle" | "ellipse" | "polygon" | "line" => draw::shape_tool(self, ev, view, &tool),
            "pen" => draw::pen(self, ev, view),
            "curvature" => draw::curvature(self, ev, view),
            "addAnchor" | "deleteAnchor" | "convertDirection" | "scissors" => draw::anchor_tool(self, ev, view, &tool),
            "pencil" | "smooth" | "erase" => draw::pencil(self, ev, view, &tool),
            "eyedropper" => misc::eyedropper(self, ev, view),
            "colorTheme" => misc::color_theme(self, ev, view),
            "measure" => misc::measure(self, ev, view),
            "gradientSwatch" | "gradientFeather" => misc::gradient(self, ev, view, &tool),
            "note" => misc::note(self, ev, view),
            "page" => misc::page(self, ev, view),
            "gap" => misc::gap(self, ev, view),
            "contentCollector" | "contentPlacer" => misc::conveyor(self, ev, view, &tool),
            "place" => misc::place(self, ev, view),
            _ => Ok(()),
        }
    }

    pub fn tool_key(&mut self, key: Key, mods: Mods, view: View) -> Result<bool> {
        if self.active().is_none() {
            return Ok(false);
        }
        if key == Key::Escape && self.tools.gesture.is_some() {
            if let Some(b) = self.tools.gesture.take().and_then(|g| g.before) {
                self.restore(&b);
            }
            self.tools.overlays.clear();
            return Ok(true);
        }
        if key == Key::Escape && self.tools.thread.take().is_some() {
            self.tools.overlays.clear();
            return Ok(true);
        }
        let tool = self.tool.clone();
        match tool.as_str() {
            "type" | "typeVertical" | "typeOnPath" | "note" if self.wants_text() => text::key(self, key, mods, view),
            "pen" | "curvature" => Ok(draw::pen_key(self, key)),
            _ => select::key(self, key, mods),
        }
    }

    pub fn cursor(&mut self, at: Point, mods: Mods, view: View) -> &'static str {
        if self.active().is_none() {
            return "arrow";
        }
        if self.tools.thread.is_some() {
            return "loadedText";
        }
        match self.tool.as_str() {
            "selection" | "freeTransform" => select::cursor(self, at, view),
            "directSelection" => "arrowHollow",
            "type" | "typeOnPath" | "typeVertical" => text::cursor(self, at, view),
            "rectangleFrame" | "ellipseFrame" | "polygonFrame" | "rectangle" | "ellipse" | "polygon" | "line" | "measure" | "gap" | "page" => "crosshair",
            "pen" | "curvature" | "addAnchor" | "deleteAnchor" | "convertDirection" | "pencil" | "smooth" | "erase" | "scissors" => "pen",
            "eyedropper" | "colorTheme" => "eyedropper",
            "rotate" => "rotate",
            "scale" | "shear" | "gradientSwatch" | "gradientFeather" => "crosshair",
            "hand" => if mods.alt { "handGrab" } else { "hand" },
            "zoom" => if mods.alt { "zoomOut" } else { "zoomIn" },
            "place" => {
                if self.place_gun.first().and_then(|e| e.get("name")).and_then(Value::as_str).is_some_and(formdes_interchange::text::is_text_name) {
                    "loadedText"
                } else {
                    "loadedGraphic"
                }
            }
            "note" => "text",
            _ => "arrow",
        }
    }
}
