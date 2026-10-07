use formdes_model::geom::Point;
use serde_json::{Value, json};

use crate::session::Session;
use crate::tools::{self, Ev, Key, Kind, Mods, View};
use crate::{EngineError, Result};

const KINDS: [(&str, Kind); 6] = [("down", Kind::Down), ("drag", Kind::Drag), ("up", Kind::Up), ("move", Kind::Move), ("doubleclick", Kind::Double), ("dblclick", Kind::Double)];

fn num(v: &Value, k: &str) -> Option<f64> {
    v.get(k)?.as_f64()
}

pub fn view_of(p: &Value) -> View {
    let zoom = num(p, "zoom").filter(|z| z.is_finite() && *z > 0.0).unwrap_or(1.0);
    let snap = match p.get("snap") {
        Some(Value::Bool(b)) => *b,
        Some(Value::Object(m)) => ["enabled", "on", "guides"].iter().find_map(|k| m.get(*k).and_then(Value::as_bool)).unwrap_or(true),
        _ => true,
    };
    View { zoom, snap }
}

pub fn mods_of(p: &Value) -> Mods {
    let flag = |k: &str| p.get(k).and_then(Value::as_bool) == Some(true);
    p.get("mods")
        .cloned()
        .and_then(|m| serde_json::from_value::<Mods>(m).ok())
        .unwrap_or(Mods { shift: flag("shift"), alt: flag("alt"), cmd: flag("cmd"), ctrl: flag("ctrl"), space: flag("space") })
}

struct Combo<'a> {
    key: &'a str,
    cmd: bool,
    shift: bool,
    alt: bool,
    ctrl: bool,
}

impl<'a> Combo<'a> {
    fn parse(text: &'a str) -> Combo<'a> {
        let (held, key) = match text.rsplit_once('+') {
            Some((h, "")) => (h.trim_end_matches('+'), "+"),
            Some((h, k)) => (h, k),
            None => ("", text),
        };
        let mut c = Combo { key, cmd: false, shift: false, alt: false, ctrl: false };
        for m in held.split('+').map(str::to_ascii_lowercase) {
            match m.as_str() {
                "mod" | "cmd" | "meta" => c.cmd = true,
                "shift" => c.shift = true,
                "alt" | "option" => c.alt = true,
                "ctrl" | "control" => c.ctrl = true,
                _ => {}
            }
        }
        c
    }

    fn pressed(&self, key: &str, m: Mods) -> bool {
        self.key.eq_ignore_ascii_case(key) && (self.cmd, self.shift, self.alt, self.ctrl) == (m.cmd, m.shift, m.alt, m.ctrl)
    }
}

fn bound(shortcut: Option<&str>, key: &str, m: Mods) -> bool {
    shortcut.is_some_and(|s| Combo::parse(s).pressed(key, m))
}

fn status(ses: &Session, cursor: Option<&str>) -> Value {
    json!({"selection": ses.selection_json(), "tool": ses.tool_id(), "busy": ses.tool_busy(), "wantsText": ses.wants_text(), "cursor": cursor})
}

pub fn pointer(ses: &mut Session, p: &Value) -> Result<Value> {
    let list = p.get("events").and_then(Value::as_array).ok_or_else(|| EngineError::new("missing `events`"))?;
    if ses.active().is_none() {
        return Err(EngineError::new("no document open"));
    }
    let held = mods_of(p);
    let mut events = Vec::with_capacity(list.len());
    for e in list {
        let name = e.get("kind").and_then(Value::as_str).unwrap_or("");
        let kind = KINDS
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, k)| *k)
            .ok_or_else(|| EngineError::new(format!("unknown pointer kind `{name}`: use down, drag, up, move or doubleclick")))?;
        let (Some(x), Some(y)) = (num(e, "x"), num(e, "y")) else { return Err(EngineError::new("every pointer event needs numbers `x` and `y`")) };
        let mods = e.get("mods").cloned().and_then(|m| serde_json::from_value(m).ok()).unwrap_or(held);
        events.push(Ev { kind, pos: Point::new(x, y), mods });
    }
    if let Some(t) = p.get("tool").and_then(Value::as_str) {
        ses.set_tool(t);
    }
    let view = view_of(p);
    for ev in &events {
        ses.pointer(ev, view)?;
    }
    let cursor = events.last().map(|ev| ses.cursor(ev.pos, ev.mods, view));
    Ok(status(ses, cursor))
}

pub fn key(ses: &mut Session, p: &Value) -> Result<Value> {
    let name = p.get("key").and_then(Value::as_str).ok_or_else(|| EngineError::new("missing `key`"))?;
    let mods = mods_of(p);
    let view = view_of(p);
    let typing = ses.wants_text();
    let named: Option<Key> = tools::key_named(name);
    let unmodified = !(mods.cmd || mods.ctrl || mods.alt);
    let to_tool = |ses: &mut Session| -> Result<bool> {
        match named {
            Some(k) => ses.tool_key(k, mods, view),
            None => Ok(false),
        }
    };
    let tool_reply = |ses: &Session| json!({"handledBy": "tool", "tool": ses.tool_id(), "selection": ses.selection_json()});
    if (typing || ses.tool_busy()) && to_tool(ses)? {
        return Ok(tool_reply(ses));
    }
    if !(typing && unmodified)
        && let Some(id) = ses.specs().iter().find(|s| bound(s.shortcut, name, mods)).map(|s| s.id)
    {
        let result = ses.execute(id, &json!({}))?;
        return Ok(json!({"handledBy": "command", "command": id, "result": result}));
    }
    if !(typing || mods.cmd || mods.ctrl)
        && let Some(id) = tools::GROUPS.iter().flat_map(|g| g.iter()).find(|t| bound(t.shortcut, name, mods)).map(|t| t.id)
    {
        ses.set_tool(id);
        return Ok(json!({"handledBy": "tool.select", "tool": ses.tool_id()}));
    }
    if to_tool(ses)? {
        return Ok(tool_reply(ses));
    }
    if typing && unmodified {
        let t = match p.get("text").and_then(Value::as_str) {
            Some(t) => Some(t.to_string()),
            None if name.chars().count() == 1 => Some(name.to_string()),
            None if name.eq_ignore_ascii_case("space") => Some(" ".to_string()),
            None => None,
        };
        if let Some(t) = t {
            ses.execute("text.insert", &json!({"text": t}))?;
            return Ok(json!({"handledBy": "text", "text": t}));
        }
    }
    Ok(json!({"handledBy": null}))
}

pub fn text(ses: &mut Session, p: &Value) -> Result<Value> {
    let t = p.get("text").and_then(Value::as_str).ok_or_else(|| EngineError::new("missing `text`"))?;
    if !ses.wants_text() {
        return Err(EngineError::new("nothing is taking text: click into a text frame with the Type tool first"));
    }
    let raw = p.get("raw").and_then(Value::as_bool).unwrap_or(false);
    ses.execute("text.insert", &json!({"text": t, "raw": raw}))?;
    Ok(json!({"inserted": t.chars().count()}))
}

pub fn cursor(ses: &mut Session, p: &Value) -> Value {
    let at = Point::new(num(p, "x").unwrap_or(0.0), num(p, "y").unwrap_or(0.0));
    json!({"cursor": ses.cursor(at, mods_of(p), view_of(p))})
}

pub fn tool(ses: &mut Session, p: &Value) -> Value {
    if let Some(id) = p.get("id").or_else(|| p.get("tool")).and_then(Value::as_str) {
        ses.set_tool(id);
    }
    json!({"tool": ses.tool_id()})
}

pub fn tools() -> Value {
    tools::tools_json()
}

pub fn overlays(ses: &mut Session, p: &Value) -> Value {
    let view = view_of(p);
    crate::overlays::overlays(ses, p, view)
}
