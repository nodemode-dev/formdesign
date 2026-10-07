use formdes_model::geom::Rect;
use formdes_model::ids::{ItemId, StoryId};
use serde_json::Value;

use crate::{EngineError, Result};

pub fn opt_str<'a>(p: &'a Value, k: &str) -> Option<&'a str> {
    p.get(k).and_then(Value::as_str)
}

pub fn str_<'a>(p: &'a Value, k: &str, what: &str) -> Result<&'a str> {
    opt_str(p, k).ok_or_else(|| EngineError::new(format!("missing `{k}` ({what})")))
}

pub fn opt_f64(p: &Value, k: &str) -> Option<f64> {
    p.get(k).and_then(Value::as_f64)
}

pub fn f64_(p: &Value, k: &str, what: &str) -> Result<f64> {
    opt_f64(p, k).ok_or_else(|| EngineError::new(format!("missing `{k}` ({what})")))
}

pub fn opt_u64(p: &Value, k: &str) -> Option<u64> {
    p.get(k).and_then(|v| v.as_u64().or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64)))
}

pub fn u64_(p: &Value, k: &str, what: &str) -> Result<u64> {
    opt_u64(p, k).ok_or_else(|| EngineError::new(format!("missing `{k}` ({what})")))
}

pub fn opt_bool(p: &Value, k: &str) -> Option<bool> {
    p.get(k).and_then(Value::as_bool)
}

pub fn rect(v: &Value) -> Option<Rect> {
    let a = v.as_array()?;
    if a.len() != 4 {
        return None;
    }
    let n: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
    (n.len() == 4).then(|| Rect::new(n[0], n[1], n[2], n[3]))
}

pub fn rect_(p: &Value, k: &str, what: &str) -> Result<Rect> {
    p.get(k).and_then(rect).ok_or_else(|| EngineError::new(format!("missing `{k}` ({what}: [x0, y0, x1, y1] in points)")))
}

pub fn item(p: &Value, k: &str) -> Option<ItemId> {
    opt_u64(p, k).map(ItemId)
}

pub fn items(p: &Value, k: &str) -> Option<Vec<ItemId>> {
    match p.get(k)? {
        Value::Array(a) => Some(a.iter().filter_map(|v| v.as_u64().map(ItemId)).collect()),
        v => v.as_u64().map(|n| vec![ItemId(n)]),
    }
}

pub fn story(p: &Value, k: &str) -> Option<StoryId> {
    opt_u64(p, k).map(StoryId)
}
