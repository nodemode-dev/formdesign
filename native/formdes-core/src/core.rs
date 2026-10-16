use std::sync::Arc;

use formdes_engine::session::substitute;
use formdes_engine::{Session, input, view, wire};
use formdes_model::doc::Doc;
use formdes_model::ids::ItemId;
use formdes_model::style::StyleKind;
use serde_json::{Map, Value, json};

use crate::proto::Reply;

const BUNDLED_FONTS: [&[u8]; 13] = [
    include_bytes!("../fonts/SourceSerif4-Regular.ttf"),
    include_bytes!("../fonts/SourceSerif4-It.ttf"),
    include_bytes!("../fonts/SourceSerif4-Bold.ttf"),
    include_bytes!("../fonts/SourceSerif4-BoldIt.ttf"),
    include_bytes!("../fonts/SourceSerif4-Semibold.ttf"),
    include_bytes!("../fonts/SourceSans3-Regular.ttf"),
    include_bytes!("../fonts/SourceSans3-It.ttf"),
    include_bytes!("../fonts/SourceSans3-Bold.ttf"),
    include_bytes!("../fonts/SourceSans3-Semibold.ttf"),
    include_bytes!("../fonts/Inter-Regular.ttf"),
    include_bytes!("../fonts/Inter-Medium.ttf"),
    include_bytes!("../fonts/Inter-SemiBold.ttf"),
    include_bytes!("../fonts/JetBrainsMono-Regular.ttf"),
];

pub struct Core {
    pub ses: Session,
    rev: u64,
    sel_rev: u64,
    seen: (u64, usize, u64),
    seen_sel: String,
    prev_doc: Option<Arc<Doc>>,
    changed: bool,
    damage: Option<Vec<[f64; 4]>>,
    requests: Vec<Value>,
}

fn s<'a>(p: &'a Value, k: &str) -> Option<&'a str> {
    p.get(k).and_then(Value::as_str)
}

fn err(e: formdes_engine::EngineError) -> String {
    e.0
}

impl Core {
    pub fn new() -> Self {
        Core { ses: Session::default(), rev: 1, sel_rev: 1, seen: (0, 0, 0), seen_sel: "0|selection".into(), prev_doc: None, changed: false, damage: Some(vec![]), requests: vec![] }
    }

    pub fn load_fonts_from_env(&mut self) {
        let fonts = &self.ses.renderer.fonts;
        for data in BUNDLED_FONTS {
            fonts.load_data(data.to_vec());
        }
        if let Some(dirs) = std::env::var_os("FORMDES_FONT_DIRS") {
            for dir in std::env::split_paths(&dirs) {
                fonts.load_dir(&dir);
            }
        }
        if std::env::var("FORMDES_SYSTEM_FONTS").map(|v| v != "0").unwrap_or(true) {
            fonts.load_system();
        }
    }

    pub fn selfcheck(&mut self) -> bool {
        let ok = self.ses.execute("file.new", &json!({"preset": "Letter"})).is_ok()
            && self.ses.execute("frame.create", &json!({"rect": [72, 72, 540, 200], "content": "text", "text": "formdes"})).is_ok();
        ok && view::page_image(&mut self.ses, &json!({"page": 0, "scale": 0.25})).is_ok()
    }

    fn key_now(&self) -> (u64, usize, u64) {
        match self.ses.active() {
            Some(st) => (st.uid, Arc::as_ptr(&st.doc) as usize, st.revision),
            None => (0, 0, 0),
        }
    }

    fn sel_now(&self) -> String {
        let tool = self.ses.tool_id();
        match self.ses.active() {
            Some(st) => format!("{}|{}|{}|{}", st.uid, tool, self.ses.sel_rev, self.ses.selection_json()),
            None => format!("0|{tool}"),
        }
    }

    fn settle(&mut self) {
        if let Some(st) = self.ses.active_mut() {
            formdes_engine::cmd::draw::refresh_blends(&mut st.doc);
        }
        let k = self.key_now();
        if k != self.seen {
            self.rev += 1;
            self.changed = true;
            let cur = self.ses.active().map(|st| st.doc.clone());
            self.damage = match (&self.prev_doc, &cur, self.ses.active()) {
                (Some(old), Some(new), Some(st)) if k.0 == self.seen.0 && k.0 != 0 => crate::damage::damage(old, new, &st.canvas()),
                _ => None,
            };
            self.seen = k;
            self.prev_doc = cur;
        } else {
            self.changed = false;
            self.damage = Some(vec![]);
        }
        let sel = self.sel_now();
        if sel != self.seen_sel {
            self.sel_rev += 1;
            self.seen_sel = sel;
        }
        self.requests.append(&mut self.ses.requests);
    }

    pub fn meta(&mut self) -> Value {
        let mut m = json!({"rev": self.rev, "selRev": self.sel_rev, "changed": self.changed});
        if self.changed {
            m["damage"] = match &self.damage {
                Some(d) => json!(d),
                None => Value::Null,
            };
        }
        if !self.requests.is_empty() {
            m["requests"] = Value::Array(std::mem::take(&mut self.requests));
        }
        m
    }

    pub fn call(&mut self, method: &str, p: Value) -> Result<Reply, String> {
        let r = self.dispatch(method, &p);
        self.settle();
        r
    }

    fn need_doc(&self) -> Result<(), String> {
        self.ses.active().map(|_| ()).ok_or_else(|| "no document open".to_string())
    }

    fn dispatch(&mut self, method: &str, p: &Value) -> Result<Reply, String> {
        match method {
            "ping" => Ok(json!({"pong": true, "version": env!("CARGO_PKG_VERSION")}).into()),
            "hello" => Ok(json!({
                "core": env!("CARGO_PKG_VERSION"),
                "engine": format!("formdes-engine {}", env!("CARGO_PKG_VERSION")),
                "commands": self.ses.registry.len(),
                "tools": formdes_engine::tools::GROUPS.iter().map(|g| g.len()).sum::<usize>(),
            })
            .into()),
            "exec" => self.exec(p),
            "batch" => self.batch(p),
            "commands" => Ok(self.commands(p).into()),
            "inspect" => self.ses.execute("document.inspect", &json!({})).map(Reply::from).map_err(err),
            "state" => Ok(self.state().into()),
            "layout" => Ok(view::layout_json(&self.ses).into()),
            "tiles" => view::tiles(&mut self.ses, p).map(|(result, bin)| Reply { result, bin: Some(bin) }).map_err(err),
            "page.png" => view::page_image(&mut self.ses, p).map(|(result, bin)| Reply { result, bin: Some(bin) }).map_err(err),
            "overlays" => Ok(input::overlays(&mut self.ses, p).into()),
            "pointer" => input::pointer(&mut self.ses, p).map(Reply::from).map_err(err),
            "key" => input::key(&mut self.ses, p).map(Reply::from).map_err(err),
            "text" => input::text(&mut self.ses, p).map(Reply::from).map_err(err),
            "cursor" => Ok(input::cursor(&mut self.ses, p).into()),
            "tool" => {
                s(p, "id").or(s(p, "tool")).ok_or("missing `id`")?;
                Ok(input::tool(&mut self.ses, p).into())
            }
            "tools" => Ok(input::tools().into()),
            "serialize" => {
                let st = self.ses.active().ok_or("no document open")?;
                let bytes = formdes_engine::cmd::file::native_bytes(&st.doc).map_err(err)?;
                let title = format!("{}.{}", st.title(), formdes_engine::cmd::file::NATIVE_EXT);
                Ok(Reply { result: json!({"bytes": bytes.len(), "title": title}), bin: Some(bytes) })
            }
            "docs.activate" => {
                let i = p.get("index").and_then(Value::as_u64).ok_or("missing `index`")? as usize;
                if i >= self.ses.docs.len() {
                    return Err(format!("no document {i}"));
                }
                self.ses.active = Some(i);
                self.ses.sel_rev += 1;
                Ok(self.state().into())
            }
            "docs.close" => {
                let i = p.get("index").and_then(Value::as_u64).map(|v| v as usize).or(self.ses.active).ok_or("no document open")?;
                if i >= self.ses.docs.len() {
                    return Err(format!("no document {i}"));
                }
                self.ses.close_doc(i);
                self.ses.sel_rev += 1;
                Ok(self.state().into())
            }
            "docs.saved" => {
                let st = self.ses.active_mut().ok_or("no document open")?;
                st.saved = st.doc.clone();
                Ok(json!({"dirty": st.is_dirty()}).into())
            }
            "slots" => {
                let st = self.ses.active().ok_or("no document open")?;
                Ok(crate::slots::slots_json(&st.doc, s(p, "prefix").unwrap_or("slot:")).into())
            }
            "frame.thread" | "frame.unthread" => self.ses.execute(method, p).map(Reply::from).map_err(err),
            "fonts.add" => {
                let fonts = &self.ses.renderer.fonts;
                let before = fonts.len();
                if let Some(path) = s(p, "path") {
                    let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
                    fonts.load_data(bytes);
                }
                if let Some(dir) = s(p, "dir") {
                    fonts.load_dir(std::path::Path::new(dir));
                }
                Ok(json!({"faces": fonts.len().saturating_sub(before)}).into())
            }
            "fonts.families" => Ok(json!(self.ses.renderer.fonts.families()).into()),
            "fonts.styles" => Ok(json!(self.ses.renderer.fonts.styles_of(s(p, "family").unwrap_or_default())).into()),
            "fonts.face" | "fonts.data" => {
                let family = s(p, "family").ok_or("missing `family`")?;
                let style = s(p, "style").unwrap_or("Regular");
                let fonts = &self.ses.renderer.fonts;
                if !fonts.families().iter().any(|f| f.eq_ignore_ascii_case(family)) {
                    return Err(format!("no font family {family}"));
                }
                let face = fonts.resolve(family, style).ok_or_else(|| format!("no font family {family}"))?;
                if method == "fonts.data" {
                    let bytes = face.data.to_vec();
                    return Ok(Reply { result: json!({"bytes": bytes.len(), "index": face.index}), bin: Some(bytes) });
                }
                Ok(json!({"family": family, "style": style, "index": face.index, "variable": face.is_variable(), "chars": face.chars()}).into())
            }
            "objectStyles" => self.object_styles(p),
            "findChange" => crate::find::run(self, p).map(Reply::from),
            "spellMarks" => crate::spell::marks(self, p).map(Reply::from),
            "spellSuggest" => crate::spell::suggest(p).map(Reply::from),
            "spellCheck" => crate::spell::check(self, p).map(Reply::from),
            "spellLanguages" => Ok(Reply::from(crate::spell::languages())),
            "spellReload" => Ok(Reply::from(crate::spell::reload())),
            "preflight" => crate::preflight::run(self, p).map(Reply::from),
            "snippetThumb" => crate::thumb::snippet(self, p),
            "web" => crate::web::content(self, p).map(Reply::from),
            "web.asset" => crate::web::asset(self, p),
            "pathType" => {
                let st = self.ses.active().ok_or("no document open")?;
                let id = p
                    .get("id")
                    .and_then(Value::as_u64)
                    .map(ItemId)
                    .or_else(|| st.selection.items.first().copied())
                    .or_else(|| st.selection.text.and_then(|t| st.doc.story(t.story)?.frames.first().copied()));
                let pt = id.and_then(|i| st.doc.item(i)).and_then(|it| it.text()).and_then(|tf| tf.on_path.clone());
                Ok(match (id, pt) {
                    (Some(i), Some(pt)) => json!({"id": i.0, "start": pt.start, "flip": pt.flip, "align": pt.align}),
                    _ => Value::Null,
                }
                .into())
            }
            "story.paras" => {
                p.get("story").and_then(Value::as_u64).ok_or("missing `story`")?;
                self.need_doc()?;
                self.ses.execute("story.paras", p).map(Reply::from).map_err(err)
            }
            other => Err(format!("unknown method {other}")),
        }
    }

    pub(crate) fn exec_value(&mut self, id: &str, params: &Value) -> Result<Value, String> {
        self.ses.execute(id, params).map_err(err)
    }

    fn object_styles(&mut self, p: &Value) -> Result<Reply, String> {
        let st = self.ses.active().ok_or("no document open")?;
        let doc = &st.doc;
        if let Some(name) = s(p, "delete") {
            let key = formdes_model::style::style_key(name);
            if !doc.styles.exists(StyleKind::Object, &key) {
                return Err(format!("no object style `{name}`"));
            }
            self.exec_value("style.object.delete", &json!({"name": name}))?;
            return Ok(Value::Null.into());
        }
        if let Some(name) = s(p, "edit") {
            let key = formdes_model::style::style_key(name);
            let style = doc.styles.find(StyleKind::Object, &key).ok_or_else(|| format!("no object style `{name}`"))?;
            let mut attrs = Map::new();
            if p.get("redefine").and_then(Value::as_bool).unwrap_or(false)
                && let Some(it) = st.selection.items.first().and_then(|i| doc.item(*i))
            {
                attrs.insert("fill".into(), serde_json::to_value(&it.fill).unwrap_or(Value::Null));
                attrs.insert("stroke".into(), serde_json::to_value(&it.stroke).unwrap_or(Value::Null));
                if let Some(t) = it.text() {
                    attrs.insert("textFrame".into(), json!({"columns": t.columns, "gutter": t.gutter, "insets": t.insets, "vjust": t.vjust, "autosize": t.autosize, "ignoreWrap": t.ignore_wrap}));
                }
            }
            match p.get("fill") {
                Some(Value::Null) => {
                    attrs.insert("fill".into(), Value::Null);
                }
                Some(Value::String(sw)) => {
                    attrs.insert("fill".into(), serde_json::to_value(wire::paint_in(sw, None)).unwrap_or(Value::Null));
                }
                _ => {}
            }
            match p.get("stroke") {
                Some(Value::Null) => {
                    attrs.insert("stroke".into(), Value::Null);
                }
                Some(Value::Object(patch)) => {
                    let cur: formdes_model::item::Stroke = attrs
                        .get("stroke")
                        .or_else(|| style.attrs.get("stroke"))
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_else(|| formdes_engine::ops::default_stroke(1.0));
                    let mut sk = cur;
                    if let Some(sw) = patch.get("swatch").and_then(Value::as_str) {
                        sk.paint = wire::paint_in(sw, None);
                    }
                    if let Some(w) = patch.get("weight").and_then(Value::as_f64) {
                        sk.weight = w.max(0.0);
                    }
                    attrs.insert("stroke".into(), serde_json::to_value(&sk).unwrap_or(Value::Null));
                }
                _ => {}
            }
            match p.get("paragraphStyle") {
                Some(Value::Null) => {
                    attrs.insert("paragraphStyle".into(), Value::Null);
                }
                Some(Value::String(ps)) => {
                    attrs.insert("paragraphStyle".into(), json!(ps));
                }
                _ => {}
            }
            let to = s(p, "to").map(str::trim).filter(|t| !t.is_empty() && *t != name).map(str::to_string);
            let mut params = json!({"name": name, "attrs": attrs});
            if let Some(t) = &to {
                params["rename"] = json!(t);
            }
            self.exec_value("style.object.edit", &params)?;
            return Ok(json!({"name": to.unwrap_or_else(|| name.to_string())}).into());
        }
        let styles: Vec<Value> = doc
            .styles
            .list(StyleKind::Object)
            .iter()
            .map(|os| {
                let fill = os.attrs.get("fill").and_then(|v| serde_json::from_value::<formdes_model::color::Paint>(v.clone()).ok()).map(|f| json!({"swatch": wire::paint_name(&f)}));
                let stroke = os
                    .attrs
                    .get("stroke")
                    .and_then(|v| serde_json::from_value::<formdes_model::item::Stroke>(v.clone()).ok())
                    .map(|k| json!({"swatch": wire::paint_name(&k.paint), "weight": k.weight}));
                let ps = os.attrs.get("paragraphStyle").and_then(Value::as_str).map(|n| wire::style_name(StyleKind::Paragraph, n));
                json!({"name": wire::style_name(StyleKind::Object, &os.name), "fill": fill, "stroke": stroke, "paragraphStyle": ps})
            })
            .collect();
        let selected: Vec<String> = st
            .selection
            .items
            .iter()
            .filter_map(|i| doc.item(*i))
            .map(|it| wire::style_name(StyleKind::Object, it.style.as_deref().unwrap_or("None")))
            .collect();
        Ok(json!({"styles": styles, "selected": selected}).into())
    }

    fn exec(&mut self, p: &Value) -> Result<Reply, String> {
        let id = s(p, "command").or(s(p, "id")).ok_or("missing `command`")?.to_string();
        let params = match p.get("params") {
            None | Some(Value::Null) => json!({}),
            Some(v) => v.clone(),
        };
        self.exec_value(&id, &params).map(Reply::from)
    }

    fn batch(&mut self, p: &Value) -> Result<Reply, String> {
        let cmds: Vec<Value> = match (s(p, "script"), p.get("commands")) {
            (Some(t), _) => match serde_json::from_str::<Value>(t) {
                Ok(Value::Array(a)) => a,
                _ => return Err("`script` must be a JSON array of {command, params}".into()),
            },
            (None, Some(Value::Array(cmds))) => cmds.clone(),
            _ => return Err("missing `commands` (an array of {command, params}) or `script` (text)".into()),
        };
        let atomic = p.get("atomic").and_then(Value::as_bool).unwrap_or(true);
        if atomic {
            return match self.ses.batch(&cmds, None) {
                Ok(v) => Ok(v.into()),
                Err(e) => Err(format!("{} (nothing was changed: the batch was rolled back)", e.0)),
            };
        }
        let mut results: Vec<Value> = Vec::with_capacity(cmds.len());
        for (i, c) in cmds.iter().enumerate() {
            let id = c.get("command").or_else(|| c.get("id")).and_then(Value::as_str).ok_or_else(|| format!("step {}: missing `command` (a command id)", i + 1))?;
            let params = substitute(c.get("params").cloned().filter(|v| !v.is_null()).unwrap_or_else(|| json!({})), &results).map_err(|e| format!("step {} (`{id}`): {}", i + 1, e.0))?;
            let r = self.ses.execute(id, &params).map_err(|e| format!("step {} (`{id}`): {}", i + 1, e.0))?;
            results.push(r);
        }
        Ok(json!({"results": results}).into())
    }

    fn commands(&self, p: &Value) -> Value {
        let filter = s(p, "filter").map(str::to_ascii_lowercase);
        let enabled_only = p.get("enabledOnly").and_then(Value::as_bool).unwrap_or(false);
        let all = self.ses.commands_json();
        let list: Vec<Value> = all
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| !enabled_only || c.get("enabled").and_then(Value::as_bool).unwrap_or(false))
            .filter(|c| {
                filter.as_ref().is_none_or(|f| {
                    let has = |k: &str| c.get(k).and_then(Value::as_str).is_some_and(|v| v.to_ascii_lowercase().contains(f.as_str()));
                    has("id") || has("label") || c.get("menu").and_then(Value::as_array).is_some_and(|m| m.iter().filter_map(Value::as_str).any(|x| x.to_ascii_lowercase().contains(f.as_str())))
                })
            })
            .collect();
        Value::Array(list)
    }

    pub fn state(&mut self) -> Value {
        let mut v = self.ses.state_json();
        v["rev"] = json!(self.rev);
        v["selRev"] = json!(self.sel_rev);
        v["busy"] = json!(self.ses.tool_busy());
        v["wantsText"] = json!(self.ses.wants_text());
        v["tool"] = json!(self.ses.tool_id());
        v
    }
}
