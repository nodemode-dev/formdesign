use std::path::PathBuf;
use std::sync::Arc;

use formdes_model::doc::Doc;
use formdes_model::ids::{ItemId, StoryId};
use formdes_render::Renderer;
use formdes_text::layout::{DocLayout, layout_doc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::registry::{Registry, Spec};
use crate::{EngineError, Result};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSel {
    pub story: StoryId,
    pub anchor: usize,
    pub focus: usize,
}

impl TextSel {
    pub fn range(&self) -> std::ops::Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }

    pub fn caret(story: StoryId, at: usize) -> TextSel {
        TextSel { story, anchor: at, focus: at }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellSel {
    pub story: StoryId,
    pub at: usize,
    pub r0: usize,
    pub c0: usize,
    pub r1: usize,
    pub c1: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    #[serde(default)]
    pub items: Vec<ItemId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextSel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cells: Option<CellSel>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub content: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<(ItemId, usize, usize)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_text: Option<crate::celltext::CellText>,
}

impl Selection {
    pub fn items(items: Vec<ItemId>) -> Selection {
        Selection { items, ..Selection::default() }
    }

    pub fn text(t: TextSel) -> Selection {
        Selection { text: Some(t), ..Selection::default() }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.text.is_none() && self.cells.is_none()
    }
}

#[derive(Clone)]
pub struct Snapshot {
    pub label: String,
    pub doc: Arc<Doc>,
    pub selection: Selection,
}

#[derive(Default, Clone)]
pub struct History {
    pub undo: Vec<Snapshot>,
    pub redo: Vec<Snapshot>,
}

pub const UNDO_LIMIT: usize = 500;

pub struct DocState {
    pub uid: u64,
    pub doc: Arc<Doc>,
    pub selection: Selection,
    pub history: History,
    pub path: Option<PathBuf>,
    pub saved: Arc<Doc>,
    pub revision: u64,
    pub editing_parents: bool,
    pub rotations: Vec<(crate::canvas::SpreadRef, u32)>,
    pub pending: serde_json::Map<String, Value>,
    pub track_changes: bool,
    pub active_layer: Option<formdes_model::ids::LayerId>,
    pub last_find: Option<Value>,
    pub coalesce: Option<u64>,
    layout: Option<(u64, Arc<DocLayout>)>,
    ops: std::collections::HashMap<formdes_render::scene::Target, (u64, Arc<Vec<formdes_render::Op>>)>,
}

impl DocState {
    pub fn new(uid: u64, doc: Doc, path: Option<PathBuf>) -> DocState {
        let doc = Arc::new(doc);
        DocState {
            uid,
            saved: doc.clone(),
            doc,
            selection: Selection::default(),
            history: History::default(),
            path,
            revision: 1,
            editing_parents: false,
            rotations: Vec::new(),
            pending: serde_json::Map::new(),
            track_changes: false,
            active_layer: None,
            last_find: None,
            coalesce: None,
            layout: None,
            ops: std::collections::HashMap::new(),
        }
    }

    pub fn canvas(&self) -> crate::canvas::Canvas {
        crate::canvas::canvas(&self.doc, self.editing_parents, &self.rotations)
    }

    pub fn ops(&mut self, r: &Renderer, target: formdes_render::scene::Target, printing: bool, overprint: bool) -> Arc<Vec<formdes_render::Op>> {
        let key_rev = self.revision * 4 + u64::from(printing) + 2 * u64::from(overprint);
        if let Some((rev, ops)) = self.ops.get(&target)
            && *rev == key_rev
        {
            return ops.clone();
        }
        let lay = self.layout(r);
        let ops = Arc::new(formdes_render::scene::spread_ops(&self.doc, &lay, target, &formdes_render::SceneOpts { printing, bleed: false, overprint }));
        self.ops.insert(target, (key_rev, ops.clone()));
        ops
    }

    pub fn title(&self) -> String {
        match &self.path {
            Some(p) => p.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string(),
            None => if self.doc.title.is_empty() { "Untitled".into() } else { self.doc.title.clone() },
        }
    }

    pub fn is_dirty(&self) -> bool {
        !Arc::ptr_eq(&self.doc, &self.saved)
    }

    pub fn layout(&mut self, r: &Renderer) -> Arc<DocLayout> {
        if let Some((rev, l)) = &self.layout
            && *rev == self.revision
        {
            return l.clone();
        }
        let l = Arc::new(layout_doc(&self.doc, &r.fonts));
        self.layout = Some((self.revision, l.clone()));
        l
    }
}

pub struct Session {
    pub docs: Vec<DocState>,
    pub active: Option<usize>,
    pub renderer: Arc<Renderer>,
    pub registry: Arc<Registry>,
    pub prefs: Value,
    pub tool: String,
    pub requests: Vec<Value>,
    pub sel_rev: u64,
    pub profiles: Vec<Value>,
    pub book: Option<crate::cmd::book::Book>,
    pub place_gun: Vec<Value>,
    pub tools: crate::tools::ToolState,
    pub clipboard: Option<crate::cmd::clip::Clip>,
    next_uid: u64,
}

impl Default for Session {
    fn default() -> Self {
        Session::new(Arc::new(Renderer::default()))
    }
}

pub struct Ctx<'a> {
    pub session: &'a mut Session,
    pub changed: bool,
    pub label: Option<String>,
}

impl Ctx<'_> {
    pub fn state(&self) -> Result<&DocState> {
        let i = self.session.active.ok_or_else(|| EngineError::new("no document open"))?;
        self.session.docs.get(i).ok_or_else(|| EngineError::new("no document open"))
    }

    pub fn state_mut(&mut self) -> Result<&mut DocState> {
        let i = self.session.active.ok_or_else(|| EngineError::new("no document open"))?;
        self.session.docs.get_mut(i).ok_or_else(|| EngineError::new("no document open"))
    }

    pub fn doc(&self) -> Result<&Doc> {
        Ok(&self.state()?.doc)
    }

    pub fn doc_mut(&mut self) -> Result<&mut Doc> {
        self.changed = true;
        let st = self.state_mut()?;
        Ok(Arc::make_mut(&mut st.doc))
    }

    pub fn selection(&self) -> Selection {
        self.state().map(|s| s.selection.clone()).unwrap_or_default()
    }

    pub fn set_selection(&mut self, sel: Selection) {
        if let Ok(st) = self.state_mut() {
            if st.selection.text.map(|t| t.story) != sel.text.map(|t| t.story) || sel.text.is_none() {
                st.pending.clear();
            }
            st.selection = sel;
            st.coalesce = None;
            self.session.sel_rev += 1;
        }
    }

    pub fn layout(&mut self) -> Result<Arc<DocLayout>> {
        let r = self.session.renderer.clone();
        let st = self.state_mut()?;
        Ok(st.layout(&r))
    }

    pub fn target_layer(&self) -> Result<formdes_model::ids::LayerId> {
        let st = self.state()?;
        Ok(match st.active_layer {
            Some(l) if st.doc.layer(l).is_some() => l,
            _ => crate::ops::active_layer(&st.doc),
        })
    }

    pub fn label(&mut self, l: &str) {
        self.label = Some(l.to_string());
    }

    pub fn request(&mut self, r: Value) {
        self.session.requests.push(r);
    }
}

impl Session {
    pub fn new(renderer: Arc<Renderer>) -> Session {
        Session {
            docs: Vec::new(),
            active: None,
            renderer,
            registry: Arc::new(crate::cmd::registry()),
            prefs: json!({"smartTextReflow": true}),
            tool: "selection".into(),
            requests: Vec::new(),
            sel_rev: 1,
            profiles: Vec::new(),
            book: None,
            place_gun: Vec::new(),
            tools: crate::tools::ToolState::default(),
            clipboard: None,
            next_uid: 1,
        }
    }

    pub fn open_doc(&mut self, doc: Doc, path: Option<PathBuf>) -> usize {
        let uid = self.next_uid;
        self.next_uid += 1;
        self.docs.push(DocState::new(uid, doc, path));
        let i = self.docs.len() - 1;
        self.active = Some(i);
        i
    }

    pub fn close_doc(&mut self, i: usize) {
        if i < self.docs.len() {
            self.docs.remove(i);
        }
        self.active = if self.docs.is_empty() { None } else { Some(i.min(self.docs.len() - 1)) };
    }

    pub fn active(&self) -> Option<&DocState> {
        self.docs.get(self.active?)
    }

    pub fn active_mut(&mut self) -> Option<&mut DocState> {
        let i = self.active?;
        self.docs.get_mut(i)
    }

    pub fn specs(&self) -> Vec<&Spec> {
        self.registry.specs()
    }

    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value> {
        let registry = self.registry.clone();
        let Some((spec, handler)) = registry.get(id) else {
            return Err(EngineError::new(format!("no command `{id}`")));
        };
        let before = self.active().map(|s| (s.uid, s.doc.clone(), s.selection.clone(), s.revision));
        let mut ctx = Ctx { session: self, changed: false, label: None };
        let out = handler(&mut ctx, params);
        let changed = ctx.changed;
        let label = ctx.label.take().unwrap_or_else(|| spec.label.to_string());
        match out {
            Ok(v) => {
                let typing = label == "Typing";
                let reflow = changed && matches!(label.as_str(), "Typing" | "Paste" | "Paste without Formatting" | "Place" | "Fill with Placeholder Text" | "Replace Text");
                if changed
                    && spec.undoable
                    && let Some((uid, doc, sel, rev0)) = before
                    && let Some(st) = self.active_mut()
                    && st.uid == uid
                    && !Arc::ptr_eq(&st.doc, &doc)
                {
                    let merge = typing && st.coalesce == Some(rev0) && st.history.undo.last().is_some_and(|s| s.label == "Typing");
                    if !merge {
                        st.history.undo.push(Snapshot { label, doc, selection: sel });
                        if st.history.undo.len() > UNDO_LIMIT {
                            st.history.undo.remove(0);
                        }
                    }
                    st.history.redo.clear();
                }
                if changed && let Some(st) = self.active_mut() {
                    st.revision += 1;
                    st.coalesce = if typing { Some(st.revision) } else { None };
                }
                if reflow {
                    let r = self.renderer.clone();
                    if let Some(st) = self.active_mut()
                        && st.doc.settings.smart_reflow
                    {
                        let mut doc = (*st.doc).clone();
                        if crate::reflow::smart_reflow(&mut doc, &r) > 0 {
                            st.doc = Arc::new(doc);
                            st.revision += 1;
                            st.coalesce = None;
                        }
                    }
                }
                Ok(v)
            }
            Err(e) => {
                if changed
                    && let Some((uid, doc, sel, _)) = before
                    && let Some(st) = self.active_mut()
                    && st.uid == uid
                {
                    st.doc = doc;
                    st.selection = sel;
                    st.revision += 1;
                }
                Err(e)
            }
        }
    }

    pub fn undo(&mut self) -> Result<Value> {
        let st = self.active_mut().ok_or_else(|| EngineError::new("no document open"))?;
        let snap = st.history.undo.pop().ok_or_else(|| EngineError::new("nothing to undo"))?;
        let label = snap.label.clone();
        st.history.redo.push(Snapshot { label: snap.label, doc: std::mem::replace(&mut st.doc, snap.doc), selection: std::mem::replace(&mut st.selection, snap.selection) });
        st.revision += 1;
        Ok(json!({"undone": label}))
    }

    pub fn redo(&mut self) -> Result<Value> {
        let st = self.active_mut().ok_or_else(|| EngineError::new("no document open"))?;
        let snap = st.history.redo.pop().ok_or_else(|| EngineError::new("nothing to redo"))?;
        let label = snap.label.clone();
        st.history.undo.push(Snapshot { label: snap.label, doc: std::mem::replace(&mut st.doc, snap.doc), selection: std::mem::replace(&mut st.selection, snap.selection) });
        st.revision += 1;
        Ok(json!({"redone": label}))
    }

    pub fn group<F: FnOnce(&mut Session) -> Result<Value>>(&mut self, label: &str, f: F) -> Result<Value> {
        let before = self.active().map(|s| (s.uid, s.doc.clone(), s.selection.clone(), s.history.undo.len()));
        let out = f(self);
        if let Some((uid, doc, sel, depth)) = before
            && let Some(st) = self.active_mut()
            && st.uid == uid
        {
            match &out {
                Ok(_) if !Arc::ptr_eq(&st.doc, &doc) => {
                    st.history.undo.truncate(depth);
                    st.history.undo.push(Snapshot { label: label.to_string(), doc, selection: sel });
                    st.history.redo.clear();
                }
                Ok(_) => {}
                Err(_) => {
                    st.history.undo.truncate(depth);
                    st.doc = doc;
                    st.selection = sel;
                    st.revision += 1;
                }
            }
        }
        out
    }

    pub fn batch(&mut self, cmds: &[Value], label: Option<&str>) -> Result<Value> {
        let registry = self.registry.clone();
        let mut labels: Vec<&str> = Vec::new();
        for c in cmds {
            let id = c.get("command").or_else(|| c.get("id")).and_then(Value::as_str).unwrap_or("");
            if let Some((spec, _)) = registry.get(id)
                && spec.undoable
                && !labels.contains(&spec.label)
            {
                labels.push(spec.label);
            }
        }
        let label = match (label, labels.as_slice()) {
            (Some(l), _) => l.to_string(),
            (None, [one]) => one.to_string(),
            _ => "Script".to_string(),
        };
        self.group(&label, |s| {
            let mut results: Vec<Value> = Vec::with_capacity(cmds.len());
            for (i, c) in cmds.iter().enumerate() {
                let id = c.get("command").or_else(|| c.get("id")).and_then(Value::as_str).ok_or_else(|| EngineError::new(format!("step {}: missing `command` (a command id)", i + 1)))?;
                let params = substitute(c.get("params").cloned().filter(|v| !v.is_null()).unwrap_or_else(|| json!({})), &results).map_err(|e| EngineError::new(format!("step {} (`{id}`): {}", i + 1, e.0)))?;
                let r = s.execute(id, &params).map_err(|e| EngineError::new(format!("step {} (`{id}`): {}", i + 1, e.0)))?;
                results.push(r);
            }
            Ok(json!({"results": results}))
        })
    }

    pub fn selection_json(&self) -> Value {
        let Some(st) = self.active() else { return Value::Null };
        let sel = &st.selection;
        let mut out = json!({"items": sel.items});
        if let Some(t) = sel.text
            && let Some(story) = st.doc.story(t.story)
        {
            let text = story.text();
            let frame = story.frames.first().copied();
            let cell = sel.cells.map(|c| json!({"table": c.at, "row": c.r0, "col": c.c0}));
            out["text"] = json!({"story": t.story, "anchor": crate::target::char_to_byte(&text, t.anchor), "focus": crate::target::char_to_byte(&text, t.focus), "frame": frame, "cell": cell});
        }
        if let Some(c) = sel.cells {
            out["cells"] = json!({"story": c.story, "table": c.at, "r0": c.r0, "c0": c.c0, "r1": c.r1, "c1": c.c1});
        }
        if let Some(t) = sel.cell_text {
            out["cellText"] = json!({"story": t.story, "table": t.at, "row": t.row, "col": t.col, "anchor": t.anchor, "focus": t.focus});
        }
        out
    }

    pub fn state_json(&self) -> Value {
        let docs: Vec<Value> = self
            .docs
            .iter()
            .enumerate()
            .map(|(i, d)| {
                json!({
                    "index": i, "uid": d.uid, "title": d.title(), "dirty": d.is_dirty(), "path": d.path.as_ref().map(|p| p.display().to_string()),
                    "pages": d.doc.page_count(), "canUndo": !d.history.undo.is_empty(), "canRedo": !d.history.redo.is_empty(),
                    "undoLabel": d.history.undo.last().map(|s| s.label.clone()), "redoLabel": d.history.redo.last().map(|s| s.label.clone()),
                    "trackChanges": d.track_changes, "revision": d.revision,
                })
            })
            .collect();
        let wants = self.active().is_some_and(|s| s.selection.text.is_some() || s.selection.cell_text.is_some());
        json!({
            "rev": self.active().map(|s| s.revision).unwrap_or(0), "selRev": self.sel_rev, "active": self.active, "docs": docs,
            "selection": self.selection_json(), "tool": self.tool, "wantsText": wants, "busy": false,
        })
    }

    pub fn commands_json(&self) -> Value {
        Value::Array(
            self.registry
                .specs()
                .into_iter()
                .map(|s| json!({"id": s.id, "label": s.label, "menu": s.menu, "params": s.sig, "shortcut": s.shortcut, "enabled": self.enabled(s), "undoable": s.undoable, "help": s.help}))
                .collect(),
        )
    }

    pub fn enabled(&self, spec: &Spec) -> bool {
        let st = self.active();
        match spec.needs {
            crate::registry::Needs::Nothing => true,
            crate::registry::Needs::Document => st.is_some(),
            crate::registry::Needs::Selection => st.is_some_and(|s| !s.selection.is_empty()),
            crate::registry::Needs::Items => st.is_some_and(|s| !s.selection.items.is_empty()),
            crate::registry::Needs::Text => st.is_some_and(|s| s.selection.text.is_some() || s.selection.items.iter().any(|i| s.doc.item(*i).is_some_and(|it| it.story().is_some()))),
        }
    }
}

fn reference(s: &str, results: &[Value]) -> Option<Result<Value>> {
    let rest = s.strip_prefix('$')?;
    let (n, field) = match rest.split_once('.') {
        Some((n, f)) => (n, Some(f)),
        None => (rest, None),
    };
    let n: usize = n.parse().ok()?;
    if field.is_some_and(|f| f.is_empty() || !f.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
        return None;
    }
    let Some(r) = results.get(n) else {
        return Some(Err(EngineError::new(format!("`{s}` refers to step {n}, which has not run yet (steps count from 0)"))));
    };
    Some(match field {
        None => Ok(r.clone()),
        Some(f) => r.get(f).cloned().ok_or_else(|| EngineError::new(format!("`{s}`: step {n} returned no `{f}`"))),
    })
}

pub fn substitute(v: Value, results: &[Value]) -> Result<Value> {
    Ok(match v {
        Value::String(s) => match reference(&s, results) {
            Some(r) => r?,
            None => Value::String(s),
        },
        Value::Array(a) => Value::Array(a.into_iter().map(|x| substitute(x, results)).collect::<Result<_>>()?),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, x)| Ok((k, substitute(x, results)?))).collect::<Result<_>>()?),
        other => other,
    })
}
