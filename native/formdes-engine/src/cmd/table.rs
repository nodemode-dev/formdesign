use formdes_model::attrs::Attrs;
use formdes_model::ids::StoryId;
use formdes_model::story::{Inline, Para, Table, TableCell};
use formdes_model::style::{BASIC_PARA, StyleKind, style_key};
use serde_json::{Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{CellSel, Ctx, Selection};
use crate::{EngineError, Result, args, wire};

fn ok() -> Result<Value> {
    Ok(json!({}))
}

fn tables_in(st: &formdes_model::story::Story) -> Vec<usize> {
    st.inlines().into_iter().filter(|(_, i)| matches!(i, Inline::Table { .. })).map(|(p, _)| p).collect()
}

pub fn current(c: &Ctx) -> Result<CellSel> {
    let sel = c.selection();
    let doc = c.doc()?;
    if let Some(cs) = sel.cells
        && doc.story(cs.story).is_some_and(|st| tables_in(st).contains(&cs.at))
    {
        return Ok(cs);
    }
    let pick = |sid: StoryId, near: Option<usize>| -> Option<CellSel> {
        let st = doc.story(sid)?;
        let list = tables_in(st);
        let at = match near {
            Some(p) => list.iter().copied().find(|a| *a == p || *a + 1 == p).or_else(|| {
                let (pi, _) = st.locate(p);
                let r = st.para_range(pi);
                list.iter().copied().find(|a| r.contains(a))
            }),
            None => list.first().copied(),
        }?;
        Some(CellSel { story: sid, at, r0: 0, c0: 0, r1: 0, c1: 0 })
    };
    if let Some(t) = sel.text
        && let Some(cs) = pick(t.story, Some(t.focus))
    {
        return Ok(cs);
    }
    for id in &sel.items {
        if let Some(sid) = doc.item(*id).and_then(|i| i.story())
            && let Some(cs) = pick(sid, None)
        {
            return Ok(cs);
        }
    }
    Err(EngineError::new("needs a table: put the text cursor in a table"))
}

fn table_of<'a>(c: &'a Ctx, cs: &CellSel) -> Result<&'a Table> {
    let st = c.doc()?.story(cs.story).ok_or_else(|| EngineError::new("no such story"))?;
    st.inlines().into_iter().find_map(|(p, i)| match i {
        Inline::Table { table } if p == cs.at => Some(table.as_ref()),
        _ => None,
    }).ok_or_else(|| EngineError::new("the table is gone"))
}

fn edit<T>(c: &mut Ctx, cs: &CellSel, f: impl FnOnce(&mut Table) -> Result<T>) -> Result<T> {
    let doc = c.doc_mut()?;
    let st = doc.story_mut(cs.story).ok_or_else(|| EngineError::new("no such story"))?;
    match st.inline_at_mut(cs.at) {
        Some(Inline::Table { table }) => f(table),
        _ => Err(EngineError::new("the table is gone")),
    }
}

fn norm(cs: &CellSel, t: &Table) -> (usize, usize, usize, usize) {
    let r1 = cs.r1.max(cs.r0).min(t.rows().saturating_sub(1));
    let c1 = cs.c1.max(cs.c0).min(t.cols().saturating_sub(1));
    (cs.r0.min(r1), cs.c0.min(c1), r1, c1)
}

fn ranges(c: &Ctx, cs: &CellSel) -> Vec<CellSel> {
    let mut out = vec![*cs];
    for m in c.selection().more_cells {
        if m.story == cs.story && m.at == cs.at && !out.contains(&m) {
            out.push(m);
        }
    }
    out
}

fn each(c: &mut Ctx, p: &Value, f: crate::registry::Handler, key: fn(&CellSel) -> usize, keep: bool) -> Result<Value> {
    let cs = current(c)?;
    let mut list = ranges(c, &cs);
    if list.len() == 1 {
        return f(c, p);
    }
    list.sort_by_key(|r| std::cmp::Reverse(key(r)));
    let saved = c.selection();
    let mut done = Vec::new();
    for r in list {
        let mut sel = saved.clone();
        sel.cells = Some(r);
        sel.more_cells.clear();
        sel.cell_text = None;
        c.set_selection(sel);
        match f(c, p) {
            Ok(v) => done.push(v),
            Err(e) => {
                c.set_selection(saved);
                return Err(e);
            }
        }
    }
    if keep {
        c.set_selection(saved);
    } else {
        let mut sel = c.selection();
        sel.more_cells.clear();
        c.set_selection(sel);
    }
    Ok(json!({"ranges": done.len(), "results": done}))
}

fn cell_value(c: &TableCell) -> Value {
    let ins = formdes_text::flow::cell_insets(c);
    json!({
        "row": c.row, "col": c.col, "rows": c.rows, "cols": c.cols,
        "fill": c.attrs.get("fill").and_then(formdes_model::color::Paint::from_value).map(|p| wire::paint_name(&p)).unwrap_or_else(|| "[None]".into()),
        "insets": ins, "vj": c.attrs.get("vj").cloned().unwrap_or(json!("top")),
        "style": wire::style_name(StyleKind::Cell, c.style.as_deref().unwrap_or("None")),
        "graphic": c.attrs.get("graphic").cloned().unwrap_or(Value::Null),
        "rotate": formdes_text::flow::cell_rotation(&c.attrs), "diagonal": c.attrs.get("diagonal").cloned().unwrap_or(Value::Null),
        "paraStyle": c.attrs.get("paraStyle").cloned().unwrap_or(Value::Null),
        "text": c.paras.iter().map(|p| p.plain()).collect::<Vec<_>>().join("\n"),
    })
}

fn get(c: &mut Ctx, _: &Value) -> Result<Value> {
    let cs = current(c)?;
    let t = table_of(c, &cs)?;
    let (r0, c0, r1, c1) = norm(&cs, t);
    let rows: Vec<Value> = (0..t.rows())
        .map(|r| {
            let kind = if r < t.header_rows { "header" } else if r >= t.rows().saturating_sub(t.footer_rows) { "footer" } else { "body" };
            json!({"height": t.heights[r], "kind": kind, "mode": if t.exact.get(r).copied().unwrap_or(false) { "exactly" } else { "atLeast" }})
        })
        .collect();
    let mut opts = serde_json::Map::new();
    opts.insert("border".into(), t.attrs.get("border").cloned().unwrap_or(json!({"weight": 1})));
    opts.insert("altRows".into(), t.attrs.get("altRows").cloned().unwrap_or(Value::Null));
    for k in TABLE_STYLE_KEYS.iter().filter(|k| !matches!(**k, "border" | "altRows")) {
        opts.insert((*k).into(), t.attrs.get(*k).cloned().unwrap_or(Value::Null));
    }
    Ok(json!({
        "table": cs.at, "story": cs.story, "rows": rows, "columns": t.widths.iter().map(|w| json!({"width": w})).collect::<Vec<_>>(),
        "cells": t.cells.iter().map(cell_value).collect::<Vec<_>>(), "headerRows": t.header_rows, "footerRows": t.footer_rows,
        "style": wire::style_name(StyleKind::Table, t.style.as_deref().unwrap_or("Basic Table")),
        "range": {"r0": r0, "r1": r1, "c0": c0, "c1": c1}, "options": opts,
        "ranges": ranges(c, &cs).iter().map(|r| { let (a, b, x, y) = norm(r, t); json!({"r0": a, "c0": b, "r1": x, "c1": y}) }).collect::<Vec<_>>(),
    }))
}

fn select(c: &mut Ctx, p: &Value) -> Result<Value> {
    let mut cs = current(c)?;
    let t = table_of(c, &cs)?;
    let (rows, cols) = (t.rows(), t.cols());
    cs.r0 = args::opt_u64(p, "row").or_else(|| args::opt_u64(p, "r0")).unwrap_or(0).min(rows as u64 - 1) as usize;
    cs.c0 = args::opt_u64(p, "col").or_else(|| args::opt_u64(p, "c0")).unwrap_or(0).min(cols as u64 - 1) as usize;
    cs.r1 = args::opt_u64(p, "r1").map(|v| v as usize).unwrap_or(cs.r0).min(rows - 1);
    cs.c1 = args::opt_u64(p, "c1").map(|v| v as usize).unwrap_or(cs.c0).min(cols - 1);
    let mut sel = c.selection();
    let mut more = Vec::new();
    if let Some(list) = p.get("rows").and_then(Value::as_array) {
        let mut picked: Vec<usize> = list.iter().filter_map(Value::as_u64).map(|v| v as usize).filter(|r| *r < rows).collect();
        picked.sort_unstable();
        picked.dedup();
        if picked.len() != list.len() || picked.is_empty() {
            return Err(EngineError::new(format!("rows must be distinct row numbers from 0 to {}", rows - 1)));
        }
        let base = cs;
        let whole = move |r: usize| CellSel { r0: r, r1: r, c0: 0, c1: cols - 1, ..base };
        cs = whole(picked[0]);
        more = picked[1..].iter().map(|r| whole(*r)).collect();
    } else if p.get("add").and_then(Value::as_bool).unwrap_or(false)
        && let Some(prev) = sel.cells.filter(|x| x.story == cs.story && x.at == cs.at)
    {
        let same = |a: &CellSel, b: &CellSel| norm(a, t) == norm(b, t);
        more = std::iter::once(prev).chain(sel.more_cells.iter().copied()).filter(|m| m.story == cs.story && m.at == cs.at).collect();
        if let Some(i) = more.iter().position(|m| same(m, &cs)) {
            more.remove(i);
            if more.is_empty() {
                return Err(EngineError::new("that is the only selected range; select another range to replace it"));
            }
            cs = more.remove(0);
        }
    }
    sel.cells = Some(cs);
    if !more.is_empty() {
        sel.cell_text = None;
    }
    sel.more_cells = more.clone();
    c.set_selection(sel);
    let all: Vec<Value> = std::iter::once(cs).chain(more).map(|r| json!({"r0": r.r0, "c0": r.c0, "r1": r.r1, "c1": r.c1})).collect();
    Ok(json!({"range": all[0], "ranges": all}))
}

fn insert(c: &mut Ctx, p: &Value) -> Result<Value> {
    let sel = c.selection().text.ok_or_else(|| EngineError::new("needs a text insertion point: click into a text frame with the Type tool"))?;
    let rows = args::opt_u64(p, "rows").unwrap_or(4).clamp(1, 500) as usize;
    let cols = args::opt_u64(p, "cols").or_else(|| args::opt_u64(p, "columns")).unwrap_or(3).clamp(1, 200) as usize;
    let doc = c.doc()?;
    let st = doc.story(sel.story).ok_or_else(|| EngineError::new("no such story"))?;
    let width = st
        .frames
        .first()
        .and_then(|f| doc.item(*f))
        .map(|it| {
            let tf = it.text();
            let w = it.shape.bbox().width() - tf.map(|t| t.insets.inside + t.insets.outside).unwrap_or(0.0);
            let cols = tf.map(|t| t.columns.max(1) as f64).unwrap_or(1.0);
            let gutter = tf.map(|t| t.gutter).unwrap_or(0.0);
            ((w - gutter * (cols - 1.0)) / cols).max(36.0)
        })
        .unwrap_or(300.0);
    let mut t = Table::new(rows, cols, width);
    t.header_rows = (args::opt_u64(p, "headerRows").unwrap_or(1) as usize).min(rows);
    t.footer_rows = (args::opt_u64(p, "footerRows").unwrap_or(0) as usize).min(rows - t.header_rows);
    t.style = args::opt_str(p, "style").map(style_key).filter(|s| s != "Basic Table");
    let mut at = sel.range().start;
    let doc = c.doc_mut()?;
    let st = doc.story_mut(sel.story).ok_or_else(|| EngineError::new("no such story"))?;
    st.delete(sel.range());
    let (_, off) = st.locate(at);
    if off > 0 {
        st.insert_text(at, "\n");
        at += 1;
    }
    let (pi2, _) = st.locate(at);
    if st.paras.get(pi2).is_some_and(|p| !p.is_empty()) {
        st.insert_text(at, "\n");
    }
    st.insert_inline(at, Inline::Table { table: Box::new(t) });
    c.set_selection(Selection { text: Some(crate::session::TextSel::caret(sel.story, at + 1)), cells: Some(CellSel { story: sel.story, at, r0: 0, c0: 0, r1: 0, c1: 0 }), ..Selection::default() });
    Ok(json!({"table": at, "story": sel.story}))
}

fn options(c: &mut Ctx, p: &Value) -> Result<Value> {
    let cs = current(c)?;
    let given: serde_json::Map<String, Value> = TABLE_STYLE_KEYS.iter().filter_map(|k| p.get(*k).map(|v| ((*k).to_string(), v.clone()))).collect();
    validate_table_style(c.doc()?, &given)?;
    edit(c, &cs, |t| {
        let n = t.rows();
        if let Some(h) = args::opt_u64(p, "headerRows") {
            t.header_rows = (h as usize).min(n.saturating_sub(t.footer_rows));
        }
        if let Some(f) = args::opt_u64(p, "footerRows") {
            t.footer_rows = (f as usize).min(n.saturating_sub(t.header_rows));
        }
        if let Some(b) = p.get("border") {
            if b.is_null() {
                t.attrs.remove("border");
            } else {
                let mut cur = t.attrs.get("border").and_then(Value::as_object).cloned().unwrap_or_default();
                if let Some(o) = b.as_object() {
                    cur.extend(o.clone());
                }
                t.attrs.insert("border".into(), Value::Object(cur));
            }
        }
        if let Some(v) = p.get("cornerRadius") {
            match v.as_f64().filter(|r| *r > 0.0) {
                Some(r) => { t.attrs.insert("cornerRadius".into(), json!(r.min(500.0))); }
                None => { t.attrs.remove("cornerRadius"); }
            }
        }
        if let Some(v) = p.get("continued") {
            match v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                Some(s) => { t.attrs.insert("continued".into(), json!(s)); }
                None => { t.attrs.remove("continued"); }
            }
        }
        if let Some(v) = p.get("splitRows") {
            if v.as_bool() == Some(true) { t.attrs.insert("splitRows".into(), json!(true)); } else { t.attrs.remove("splitRows"); }
        }
        for k in ["alt", "summary"] {
            if let Some(v) = p.get(k) {
                match v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    Some(s) => { t.attrs.insert(k.into(), json!(s)); }
                    None => { t.attrs.remove(k); }
                }
            }
        }
        for k in TABLE_STYLE_KEYS.iter().filter(|k| **k != "border") {
            if let Some(a) = p.get(*k) {
                if a.is_null() || a.as_str() == Some("") {
                    t.attrs.remove(*k);
                } else {
                    t.attrs.insert((*k).into(), a.clone());
                }
            }
        }
        Ok(())
    })?;
    c.label("Table Options");
    ok()
}

/// The keys of a table style: the border, the alternating fill and stroke patterns, and the cell style each
/// region of the table uses.
pub const TABLE_STYLE_KEYS: [&str; 10] = [
    "border",
    "altRows",
    "altCols",
    "altRowStrokes",
    "altColStrokes",
    "headerCellStyle",
    "footerCellStyle",
    "leftColumnCellStyle",
    "rightColumnCellStyle",
    "bodyCellStyle",
];

const REGION_KEYS: [&str; 5] = ["headerCellStyle", "footerCellStyle", "leftColumnCellStyle", "rightColumnCellStyle", "bodyCellStyle"];

fn want_number(k: &str, o: &Value, name: &str) -> Result<()> {
    match o.get(name) {
        None | Some(Value::Null) => Ok(()),
        Some(v) if v.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0) => Ok(()),
        _ => Err(EngineError::new(format!("`{k}.{name}` expects a number of zero or more"))),
    }
}

/// Checks the table style keys of `o` (a style's attributes or a table's options) and the cell styles they name.
pub fn validate_table_style(doc: &formdes_model::doc::Doc, o: &serde_json::Map<String, Value>) -> Result<()> {
    for (k, v) in o {
        if v.is_null() {
            continue;
        }
        if REGION_KEYS.contains(&k.as_str()) {
            let Some(name) = v.as_str() else { return Err(EngineError::new(format!("`{k}` expects the name of a cell style"))) };
            if !name.is_empty() && name != "None" && !doc.styles.exists(StyleKind::Cell, name) {
                return Err(EngineError::new(format!("`{k}`: no cell style named {name}")));
            }
        } else if k == "border" {
            if !v.is_object() {
                return Err(EngineError::new("`border` expects {weight, color}"));
            }
        } else if matches!(k.as_str(), "altRows" | "altCols" | "altRowStrokes" | "altColStrokes") {
            if !v.is_object() {
                return Err(EngineError::new(format!("`{k}` expects an object: {{first, next, skipFirst, skipLast, firstColor, firstTint, nextColor, nextTint}} (strokes: firstWeight, firstColor, firstTint, firstType, next...)")));
            }
            for n in ["first", "next", "skipFirst", "skipLast", "firstTint", "nextTint", "firstWeight", "nextWeight"] {
                want_number(k, v, n)?;
            }
        }
    }
    Ok(())
}

/// Checks the cell style keys of `o` that need more than a type: the paragraph style, the text rotation and the
/// diagonal lines.
pub fn validate_cell_style(doc: &formdes_model::doc::Doc, o: &serde_json::Map<String, Value>) -> Result<()> {
    for (k, v) in o {
        if v.is_null() {
            continue;
        }
        match k.as_str() {
            "paraStyle" => {
                let Some(name) = v.as_str() else { return Err(EngineError::new("`paraStyle` expects the name of a paragraph style")) };
                if !name.is_empty() && !doc.styles.exists(StyleKind::Paragraph, name) {
                    return Err(EngineError::new(format!("`paraStyle`: no paragraph style named {name}")));
                }
            }
            "slant" => {
                if !v.as_f64().is_some_and(|d| d == 0.0 || (10.0..=80.0).contains(&d)) {
                    return Err(EngineError::new("`slant` expects 10 to 80 degrees (0 for straight text)"));
                }
            }
            "width" | "minHeight" | "height" => {
                if !v.as_f64().is_some_and(|d| d > 0.0 && d < 5000.0) {
                    return Err(EngineError::new(format!("`{k}` expects a size in points above 0")));
                }
            }
            "rotate" => {
                if !v.as_f64().is_some_and(|d| matches!(d.round() as i64, 0 | 90 | 180 | 270 | -90 | -180 | -270)) {
                    return Err(EngineError::new("`rotate` expects 0, 90, 180 or 270 degrees"));
                }
            }
            "diagonal" => {
                let Some(d) = v.as_object() else { return Err(EngineError::new("`diagonal` expects {down?: stroke, up?: stroke, front?}")) };
                for side in ["down", "up"] {
                    if d.get(side).is_some_and(|s| !s.is_object() && !s.is_null()) {
                        return Err(EngineError::new(format!("`diagonal.{side}` expects a stroke {{weight, color, tint, type}}")));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn set_cell(c: &mut Ctx, p: &Value) -> Result<Value> {
    let cs = current(c)?;
    let vj = args::opt_str(p, "vj").map(str::to_string);
    if let Some(v) = &vj
        && !["top", "center", "bottom", "justify"].contains(&v.as_str())
    {
        return Err(EngineError::new("`vj` expected top, center, bottom or justify"));
    }
    let insets = p.get("insets").cloned();
    let fill = args::opt_str(p, "fill").map(str::to_string);
    let tint = args::opt_f64(p, "tint");
    let mut extra = serde_json::Map::new();
    for k in ["rotate", "diagonal", "paraStyle", "fillOpacity", "slant"] {
        if let Some(v) = p.get(k) {
            extra.insert(k.into(), v.clone());
        }
    }
    validate_cell_style(c.doc()?, &extra)?;
    edit(c, &cs, |t| {
        let (r0, c0, r1, c1) = norm(&cs, t);
        for cell in t.cells.iter_mut().filter(|x| x.row <= r1 && x.row + x.rows > r0 && x.col <= c1 && x.col + x.cols > c0) {
            for (k, v) in &extra {
                if v.is_null() || v.as_str() == Some("") {
                    cell.attrs.remove(k);
                } else {
                    cell.attrs.insert(k.clone(), v.clone());
                }
            }
            if let Some(v) = &vj {
                cell.attrs.insert("vj".into(), json!(v));
            }
            if let Some(i) = &insets {
                cell.attrs.insert("insets".into(), i.clone());
            }
            if let Some(f) = &fill {
                if formdes_model::color::swatch_key(f).eq_ignore_ascii_case("none") {
                    cell.attrs.remove("fill");
                } else {
                    cell.attrs.insert("fill".into(), json!(formdes_model::color::swatch_key(f)));
                }
            }
            if let Some(tn) = tint {
                cell.attrs.insert("fillTint".into(), json!(tn));
            }
        }
        Ok(())
    })?;
    c.label("Cell Options");
    ok()
}

fn row_height(c: &mut Ctx, p: &Value) -> Result<Value> {
    let cs = current(c)?;
    let h = args::f64_(p, "height", "row height in points")?.max(1.0);
    let exact = args::opt_str(p, "mode") == Some("exactly");
    edit(c, &cs, |t| {
        let (r0, _, r1, _) = norm(&cs, t);
        for r in r0..=r1 {
            t.heights[r] = h;
            if t.exact.len() < t.rows() {
                t.exact.resize(t.rows(), false);
            }
            t.exact[r] = exact;
        }
        Ok(())
    })?;
    ok()
}

fn col_width(c: &mut Ctx, p: &Value) -> Result<Value> {
    let cs = current(c)?;
    let w = args::f64_(p, "width", "column width in points")?.max(1.0);
    edit(c, &cs, |t| {
        let (_, c0, _, c1) = norm(&cs, t);
        for col in c0..=c1 {
            t.widths[col] = w;
        }
        Ok(())
    })?;
    ok()
}

fn blank(row: usize, col: usize) -> TableCell {
    TableCell { row, col, rows: 1, cols: 1, paras: vec![Para::empty(BASIC_PARA)], attrs: Attrs::new(), style: None }
}

fn insert_rows(t: &mut Table, at: usize, count: usize) {
    let h = t.heights.get(at.saturating_sub(1)).copied().unwrap_or(18.0);
    for _ in 0..count {
        t.heights.insert(at, h);
        t.exact.insert(at.min(t.exact.len()), false);
    }
    for cell in t.cells.iter_mut() {
        if cell.row >= at {
            cell.row += count;
        } else if cell.row + cell.rows > at {
            cell.rows += count;
        }
    }
    for r in at..at + count {
        for col in 0..t.cols() {
            if t.cell(r, col).is_none() {
                t.cells.push(blank(r, col));
            }
        }
    }
    if at < t.header_rows {
        t.header_rows += count;
    }
}

fn insert_cols(t: &mut Table, at: usize, count: usize) {
    let w = t.widths.get(at.saturating_sub(1)).copied().unwrap_or(72.0);
    for _ in 0..count {
        t.widths.insert(at, w);
    }
    for cell in t.cells.iter_mut() {
        if cell.col >= at {
            cell.col += count;
        } else if cell.col + cell.cols > at {
            cell.cols += count;
        }
    }
    for col in at..at + count {
        for r in 0..t.rows() {
            if t.cell(r, col).is_none() {
                t.cells.push(blank(r, col));
            }
        }
    }
}

fn rows_cols(c: &mut Ctx, p: &Value, which: &str) -> Result<Value> {
    let cs = current(c)?;
    let count = args::opt_u64(p, "count").unwrap_or(1).clamp(1, 500) as usize;
    edit(c, &cs, |t| {
        let (r0, c0, r1, c1) = norm(&cs, t);
        match which {
            "above" => insert_rows(t, r0, count),
            "below" => insert_rows(t, r1 + 1, count),
            "left" => insert_cols(t, c0, count),
            _ => insert_cols(t, c1 + 1, count),
        }
        t.cells.sort_by_key(|x| (x.row, x.col));
        Ok(())
    })?;
    c.label(if which == "above" || which == "below" { "Insert Rows" } else { "Insert Columns" });
    ok()
}

fn row_above(c: &mut Ctx, p: &Value) -> Result<Value> {
    rows_cols(c, p, "above")
}

fn row_below(c: &mut Ctx, p: &Value) -> Result<Value> {
    rows_cols(c, p, "below")
}

fn col_left(c: &mut Ctx, p: &Value) -> Result<Value> {
    rows_cols(c, p, "left")
}

fn col_right(c: &mut Ctx, p: &Value) -> Result<Value> {
    rows_cols(c, p, "right")
}

fn delete_rows(c: &mut Ctx, _: &Value) -> Result<Value> {
    let cs = current(c)?;
    edit(c, &cs, |t| {
        let (r0, _, r1, _) = norm(&cs, t);
        let n = r1 - r0 + 1;
        if n >= t.rows() {
            return Err(EngineError::new("a table keeps at least one row"));
        }
        t.cells.retain(|x| !(x.row >= r0 && x.row + x.rows <= r1 + 1));
        for cell in t.cells.iter_mut() {
            let end = cell.row + cell.rows;
            let overlap = end.min(r1 + 1).saturating_sub(cell.row.max(r0));
            cell.rows -= overlap.min(cell.rows - 1);
            if cell.row > r1 {
                cell.row -= n;
            } else if cell.row >= r0 {
                cell.row = r0;
            }
        }
        t.heights.drain(r0..=r1);
        if t.exact.len() > r0 {
            let e = (r1 + 1).min(t.exact.len());
            t.exact.drain(r0..e);
        }
        let hdr = (0..t.header_rows).filter(|r| *r < r0 || *r > r1).count();
        let body_end = t.rows() + n - t.footer_rows;
        let ftr = (body_end..t.rows() + n).filter(|r| *r < r0 || *r > r1).count();
        t.header_rows = hdr;
        t.footer_rows = ftr;
        Ok(())
    })?;
    let rows = table_of(c, &cs).map(|t| t.rows()).unwrap_or(1);
    let r = cs.r0.min(rows - 1);
    let mut sel = c.selection();
    sel.cells = Some(CellSel { r0: r, r1: r, ..cs });
    c.set_selection(sel);
    c.label("Delete Rows");
    ok()
}

fn delete_cols(c: &mut Ctx, _: &Value) -> Result<Value> {
    let cs = current(c)?;
    edit(c, &cs, |t| {
        let (_, c0, _, c1) = norm(&cs, t);
        let n = c1 - c0 + 1;
        if n >= t.cols() {
            return Err(EngineError::new("a table keeps at least one column"));
        }
        t.cells.retain(|x| !(x.col >= c0 && x.col + x.cols <= c1 + 1));
        for cell in t.cells.iter_mut() {
            let end = cell.col + cell.cols;
            let overlap = end.min(c1 + 1).saturating_sub(cell.col.max(c0));
            cell.cols -= overlap.min(cell.cols - 1);
            if cell.col > c1 {
                cell.col -= n;
            } else if cell.col >= c0 {
                cell.col = c0;
            }
        }
        t.widths.drain(c0..=c1);
        Ok(())
    })?;
    let mut sel = c.selection();
    if let Some(x) = sel.cells.as_mut() {
        let cols = table_of(c, &cs).map(|t| t.cols()).unwrap_or(1);
        x.c0 = x.c0.min(cols - 1);
        x.c1 = x.c0;
    }
    c.set_selection(sel);
    c.label("Delete Columns");
    ok()
}

fn merge(c: &mut Ctx, _: &Value) -> Result<Value> {
    let cs = current(c)?;
    edit(c, &cs, |t| {
        let (r0, c0, r1, c1) = norm(&cs, t);
        if r0 == r1 && c0 == c1 {
            return Err(EngineError::new("needs a range of cells: select two or more cells to merge"));
        }
        if t.cells.iter().any(|x| {
            let inside_r = x.row >= r0 && x.row + x.rows <= r1 + 1;
            let inside_c = x.col >= c0 && x.col + x.cols <= c1 + 1;
            let touches = x.row <= r1 && x.row + x.rows > r0 && x.col <= c1 && x.col + x.cols > c0;
            touches && !(inside_r && inside_c)
        }) {
            return Err(EngineError::new("the range cuts through a merged cell"));
        }
        let mut paras = Vec::new();
        let mut keep_attrs = Attrs::new();
        let mut style = None;
        for x in t.cells.iter().filter(|x| x.row >= r0 && x.row <= r1 && x.col >= c0 && x.col <= c1) {
            if x.row == r0 && x.col == c0 {
                keep_attrs = x.attrs.clone();
                style = x.style.clone();
            }
            for p in &x.paras {
                if !p.plain().is_empty() || paras.is_empty() {
                    paras.push(p.clone());
                }
            }
        }
        t.cells.retain(|x| !(x.row >= r0 && x.row <= r1 && x.col >= c0 && x.col <= c1));
        t.cells.push(TableCell { row: r0, col: c0, rows: r1 - r0 + 1, cols: c1 - c0 + 1, paras, attrs: keep_attrs, style });
        t.cells.sort_by_key(|x| (x.row, x.col));
        Ok(())
    })?;
    c.label("Merge Cells");
    ok()
}

fn unmerge(c: &mut Ctx, _: &Value) -> Result<Value> {
    let cs = current(c)?;
    edit(c, &cs, |t| {
        let (r0, c0, _, _) = norm(&cs, t);
        let cell = t.cell(r0, c0).cloned().ok_or_else(|| EngineError::new("no cell there"))?;
        if cell.rows == 1 && cell.cols == 1 {
            return Err(EngineError::new("that cell is not merged"));
        }
        if let Some(x) = t.cell_mut(r0, c0) {
            x.rows = 1;
            x.cols = 1;
        }
        for r in cell.row..cell.row + cell.rows {
            for col in cell.col..cell.col + cell.cols {
                if (r, col) != (cell.row, cell.col) {
                    t.cells.push(blank(r, col));
                }
            }
        }
        t.cells.sort_by_key(|x| (x.row, x.col));
        Ok(())
    })?;
    c.label("Unmerge Cells");
    ok()
}

fn strip_stroke(cell: &mut TableCell, side: &str) {
    let empty = match cell.attrs.get_mut("strokes").and_then(Value::as_object_mut) {
        Some(s) => {
            s.remove(side);
            s.is_empty()
        }
        None => false,
    };
    if empty {
        cell.attrs.remove("strokes");
    }
}

fn split_cell(c: &mut Ctx, p: &Value) -> Result<Value> {
    let cs = current(c)?;
    let horizontal = match args::opt_str(p, "direction").unwrap_or("horizontal") {
        "horizontal" => true,
        "vertical" => false,
        _ => return Err(EngineError::new("`direction` expected horizontal or vertical")),
    };
    edit(c, &cs, |t| {
        let (r0, c0, _, _) = norm(&cs, t);
        let cell = t.cell(r0, c0).cloned().ok_or_else(|| EngineError::new("no cell there"))?;
        let (r, col) = (cell.row, cell.col);
        let mut fresh = blank(r, col);
        fresh.style = cell.style.clone();
        fresh.attrs = cell.attrs.clone();
        let own = |x: &TableCell| x.row == r && x.col == col;
        if horizontal {
            if cell.rows > 1 {
                let top = cell.rows.div_ceil(2);
                fresh.row = r + top;
                fresh.rows = cell.rows - top;
                fresh.cols = cell.cols;
                strip_stroke(&mut fresh, "top");
                if let Some(x) = t.cell_mut(r, col) {
                    x.rows = top;
                    strip_stroke(x, "bottom");
                }
            } else {
                let h = t.heights[r];
                t.heights[r] = h / 2.0;
                t.heights.insert(r + 1, h / 2.0);
                let ex = t.exact.get(r).copied().unwrap_or(false);
                if t.exact.len() >= r + 1 {
                    t.exact.insert(r + 1, ex);
                }
                let body_end = t.rows() - 1 - t.footer_rows;
                if r < t.header_rows {
                    t.header_rows += 1;
                } else if r >= body_end {
                    t.footer_rows += 1;
                }
                for x in t.cells.iter_mut() {
                    if x.row > r {
                        x.row += 1;
                    } else if !own(x) && x.row + x.rows > r {
                        x.rows += 1;
                    }
                }
                fresh.row = r + 1;
                fresh.cols = cell.cols;
                strip_stroke(&mut fresh, "top");
                if let Some(x) = t.cell_mut(r, col) {
                    strip_stroke(x, "bottom");
                }
            }
            fresh.col = col;
        } else {
            if cell.cols > 1 {
                let left = cell.cols.div_ceil(2);
                fresh.col = col + left;
                fresh.cols = cell.cols - left;
                fresh.rows = cell.rows;
                strip_stroke(&mut fresh, "left");
                if let Some(x) = t.cell_mut(r, col) {
                    x.cols = left;
                    strip_stroke(x, "right");
                }
            } else {
                let w = t.widths[col];
                t.widths[col] = w / 2.0;
                t.widths.insert(col + 1, w / 2.0);
                for x in t.cells.iter_mut() {
                    if x.col > col {
                        x.col += 1;
                    } else if !own(x) && x.col + x.cols > col {
                        x.cols += 1;
                    }
                }
                fresh.col = col + 1;
                fresh.rows = cell.rows;
                strip_stroke(&mut fresh, "left");
                if let Some(x) = t.cell_mut(r, col) {
                    strip_stroke(x, "right");
                }
            }
            fresh.row = r;
        }
        fresh.paras = vec![Para::empty(BASIC_PARA)];
        t.cells.push(fresh);
        t.cells.sort_by_key(|x| (x.row, x.col));
        Ok(())
    })?;
    c.label(if horizontal { "Split Cell Horizontally" } else { "Split Cell Vertically" });
    ok()
}

fn fitting_of(p: &Value) -> Result<Option<String>> {
    match args::opt_str(p, "fitting") {
        None => Ok(None),
        Some(f @ ("fill" | "fit" | "center")) => Ok(Some(f.to_string())),
        Some(_) => Err(EngineError::new("`fitting` expected fill, fit or center")),
    }
}

fn convert_cell(c: &mut Ctx, p: &Value) -> Result<Value> {
    let cs = current(c)?;
    let to = args::opt_str(p, "to").unwrap_or("graphic").to_string();
    if to != "graphic" && to != "text" {
        return Err(EngineError::new("`to` expected graphic or text"));
    }
    let fitting = fitting_of(p)?;
    edit(c, &cs, |t| {
        let (r0, c0, r1, c1) = norm(&cs, t);
        for cell in t.cells.iter_mut().filter(|x| x.row <= r1 && x.row + x.rows > r0 && x.col <= c1 && x.col + x.cols > c0) {
            if to == "text" {
                cell.attrs.remove("graphic");
            } else if !cell.attrs.contains_key("graphic") {
                cell.attrs.insert("graphic".into(), json!({"fitting": fitting.clone().unwrap_or_else(|| "fit".into())}));
            } else if let Some(f) = &fitting
                && let Some(g) = cell.attrs.get_mut("graphic").and_then(Value::as_object_mut)
            {
                g.insert("fitting".into(), json!(f));
            }
        }
        Ok(())
    })?;
    c.label(if to == "text" { "Convert Cell to Text Cell" } else { "Convert Cell to Graphic Cell" });
    ok()
}

fn place_in_cell(c: &mut Ctx, p: &Value) -> Result<Value> {
    let cs = current(c)?;
    let path = args::str_(p, "path", "the image file to place")?.to_string();
    let fitting = fitting_of(p)?.unwrap_or_else(|| "fit".into());
    let alt = args::opt_str(p, "alt").unwrap_or("").to_string();
    let asset = crate::assets::load(&path, args::opt_u64(p, "page").unwrap_or(0) as u32)?;
    let aid = c.doc_mut()?.add_asset(asset);
    let mut count = 0;
    edit(c, &cs, |t| {
        let (r0, c0, _, _) = norm(&cs, t);
        let cell = t.cell_mut(r0, c0).ok_or_else(|| EngineError::new("no cell there"))?;
        cell.attrs.insert("graphic".into(), json!({"asset": aid.0, "fitting": fitting, "alt": alt}));
        count += 1;
        Ok(())
    })?;
    c.label("Place in Cell");
    Ok(json!({"asset": aid.0, "cells": count}))
}

const STROKE_SIDES: [&str; 6] = ["top", "bottom", "left", "right", "innerHorizontal", "innerVertical"];

fn cell_strokes(c: &mut Ctx, p: &Value) -> Result<Value> {
    let cs = current(c)?;
    let mut sides: Vec<String> = match p.get("sides") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => vec!["outside".into()],
    };
    let mut expanded = Vec::new();
    for s in sides.drain(..) {
        match s.as_str() {
            "all" => expanded.extend(STROKE_SIDES.iter().map(|x| x.to_string())),
            "outside" => expanded.extend(STROKE_SIDES[..4].iter().map(|x| x.to_string())),
            "inside" => expanded.extend(STROKE_SIDES[4..].iter().map(|x| x.to_string())),
            other if STROKE_SIDES.contains(&other) => expanded.push(s),
            other => return Err(EngineError::new(format!("unknown side {other}: use top, bottom, left, right, innerHorizontal, innerVertical, outside, inside or all"))),
        }
    }
    let weight = args::opt_f64(p, "weight").map(|w| w.max(0.0));
    let tint = args::opt_f64(p, "tint").map(|t| (t / 100.0).clamp(0.0, 1.0));
    let kind = match args::opt_str(p, "type") {
        None => None,
        Some(k) if ["solid", "dashed", "dotted", "none"].contains(&k) => Some(k.to_string()),
        Some(_) => return Err(EngineError::new("`type` expected solid, dashed, dotted or none")),
    };
    let color = match args::opt_str(p, "color") {
        None => None,
        Some(name) => {
            let key = formdes_model::color::swatch_key(name);
            if !key.starts_with('#') && !key.eq_ignore_ascii_case("none") && c.doc()?.swatch(name).is_none() {
                return Err(EngineError::new(format!("no swatch named {name}")));
            }
            Some(key)
        }
    };
    let clear = args::opt_bool(p, "clear").unwrap_or(false);
    if !clear && weight.is_none() && tint.is_none() && kind.is_none() && color.is_none() {
        return Err(EngineError::new("give weight, color, tint or type (or clear: true)"));
    }
    edit(c, &cs, |t| {
        let (r0, c0, r1, c1) = norm(&cs, t);
        for cell in t.cells.iter_mut().filter(|x| x.row <= r1 && x.row + x.rows > r0 && x.col <= c1 && x.col + x.cols > c0) {
            let mut targets: Vec<&str> = Vec::new();
            for s in &expanded {
                match s.as_str() {
                    "top" if cell.row <= r0 => targets.push("top"),
                    "bottom" if cell.row + cell.rows > r1 => targets.push("bottom"),
                    "left" if cell.col <= c0 => targets.push("left"),
                    "right" if cell.col + cell.cols > c1 => targets.push("right"),
                    "innerHorizontal" => {
                        if cell.row > r0 {
                            targets.push("top");
                        }
                        if cell.row + cell.rows <= r1 {
                            targets.push("bottom");
                        }
                    }
                    "innerVertical" => {
                        if cell.col > c0 {
                            targets.push("left");
                        }
                        if cell.col + cell.cols <= c1 {
                            targets.push("right");
                        }
                    }
                    _ => {}
                }
            }
            if targets.is_empty() {
                continue;
            }
            let mut strokes = cell.attrs.get("strokes").and_then(Value::as_object).cloned().unwrap_or_default();
            for side in targets {
                if clear {
                    strokes.remove(side);
                    continue;
                }
                let mut one = strokes.get(side).and_then(Value::as_object).cloned().unwrap_or_default();
                if let Some(w) = weight {
                    one.insert("weight".into(), json!(w));
                }
                if let Some(col) = &color {
                    one.insert("color".into(), json!(col));
                }
                if let Some(tn) = tint {
                    one.insert("tint".into(), json!(tn));
                }
                if let Some(k) = &kind {
                    one.insert("type".into(), json!(k));
                }
                strokes.insert(side.into(), Value::Object(one));
            }
            if strokes.is_empty() {
                cell.attrs.remove("strokes");
            } else {
                cell.attrs.insert("strokes".into(), Value::Object(strokes));
            }
        }
        Ok(())
    })?;
    c.label("Cell Strokes");
    ok()
}

fn distribute(c: &mut Ctx, _: &Value) -> Result<Value> {
    let cs = current(c)?;
    edit(c, &cs, |t| {
        let (_, c0, _, c1) = norm(&cs, t);
        let (a, b) = if c0 == c1 { (0, t.cols() - 1) } else { (c0, c1) };
        let total: f64 = t.widths[a..=b].iter().sum();
        let each = total / (b - a + 1) as f64;
        for w in t.widths[a..=b].iter_mut() {
            *w = each;
        }
        Ok(())
    })?;
    c.label("Distribute Columns Evenly");
    ok()
}

fn sort_rows(c: &mut Ctx, p: &Value) -> Result<Value> {
    let cs = current(c)?;
    let desc = args::opt_bool(p, "descending").unwrap_or(false);
    let col = args::opt_u64(p, "column").map(|v| v as usize);
    edit(c, &cs, |t| {
        let col = col.unwrap_or(cs.c0).min(t.cols() - 1);
        let (b0, b1) = (t.header_rows, t.rows().saturating_sub(t.footer_rows));
        if b1 <= b0 + 1 {
            return Ok(());
        }
        if t.cells.iter().any(|x| x.rows > 1 && x.row + x.rows > b0 && x.row < b1) {
            return Err(EngineError::new("cannot sort rows that hold merged cells"));
        }
        let key = |r: usize| t.cell(r, col).map(|x| x.paras.iter().map(|p| p.plain()).collect::<Vec<_>>().join(" ")).unwrap_or_default();
        let mut order: Vec<(usize, String)> = (b0..b1).map(|r| (r, key(r))).collect();
        order.sort_by(|a, b| {
            let o = match (a.1.trim().parse::<f64>(), b.1.trim().parse::<f64>()) {
                (Ok(x), Ok(y)) => x.total_cmp(&y),
                _ => a.1.to_lowercase().cmp(&b.1.to_lowercase()),
            };
            if desc { o.reverse() } else { o }
        });
        let map: std::collections::HashMap<usize, usize> = order.iter().enumerate().map(|(i, (r, _))| (*r, b0 + i)).collect();
        let heights = t.heights.clone();
        for (old, new) in &map {
            t.heights[*new] = heights[*old];
        }
        for x in t.cells.iter_mut() {
            if let Some(n) = map.get(&x.row) {
                x.row = *n;
            }
        }
        t.cells.sort_by_key(|x| (x.row, x.col));
        Ok(())
    })?;
    c.label("Sort");
    ok()
}

fn table_style_create(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "the table style's name")?.to_string();
    let mut attrs = Attrs::new();
    for k in TABLE_STYLE_KEYS {
        if let Some(v) = p.get(k).filter(|v| !v.is_null()) {
            attrs.insert(k.into(), v.clone());
        }
    }
    let given: serde_json::Map<String, Value> = attrs.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    validate_table_style(c.doc()?, &given)?;
    let doc = c.doc_mut()?;
    let st = doc.styles.create(StyleKind::Table, &name, None)?;
    st.attrs = attrs;
    Ok(json!({"name": style_key(&name)}))
}

fn table_style_apply(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = style_key(args::str_(p, "name", "a table style name")?);
    let cs = current(c)?;
    let doc = c.doc()?;
    if !doc.styles.exists(StyleKind::Table, &name) {
        return Err(EngineError::new(format!("no table style named {name}")));
    }
    let attrs = doc.styles.find(StyleKind::Table, &name).map(|s| s.attrs.clone()).unwrap_or_default();
    edit(c, &cs, |t| {
        t.style = if name == "Basic Table" || name == "No Table Style" { None } else { Some(name.clone()) };
        for k in ["border", "altRows"] {
            match attrs.get(k) {
                Some(v) => {
                    t.attrs.insert(k.into(), v.clone());
                }
                None => {
                    t.attrs.remove(k);
                }
            }
        }
        // The other table style keys are read from the style while drawing, so the table keeps no copy of them.
        for k in TABLE_STYLE_KEYS.iter().filter(|k| !matches!(**k, "border" | "altRows")) {
            t.attrs.remove(*k);
        }
        Ok(())
    })?;
    ok()
}

fn cell_style_create(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "the cell style's name")?.to_string();
    let mut attrs = Attrs::new();
    if args::opt_bool(p, "fromSelection").unwrap_or(true)
        && let Ok(cs) = current(c)
        && let Ok(t) = table_of(c, &cs)
        && let Some(cell) = t.cell(cs.r0, cs.c0)
    {
        attrs = cell.attrs.clone();
    }
    if let Some(o) = p.get("attrs").and_then(Value::as_object) {
        validate_cell_style(c.doc()?, o)?;
        attrs.extend(o.clone());
    }
    let doc = c.doc_mut()?;
    let st = doc.styles.create(StyleKind::Cell, &name, None)?;
    st.attrs = attrs;
    Ok(json!({"name": style_key(&name)}))
}

fn cell_style_apply(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = style_key(args::str_(p, "name", "a cell style name, or [None]")?);
    let cs = current(c)?;
    let doc = c.doc()?;
    if name != "None" && !doc.styles.exists(StyleKind::Cell, &name) {
        return Err(EngineError::new(format!("no cell style named {name}")));
    }
    let attrs = doc.styles.find(StyleKind::Cell, &name).map(|s| s.attrs.clone()).unwrap_or_default();
    edit(c, &cs, |t| {
        let (r0, c0, r1, c1) = norm(&cs, t);
        for cell in t.cells.iter_mut().filter(|x| x.row <= r1 && x.row + x.rows > r0 && x.col <= c1 && x.col + x.cols > c0) {
            if name == "None" {
                cell.style = None;
            } else {
                cell.style = Some(name.clone());
                for (k, v) in &attrs {
                    cell.attrs.insert(k.clone(), v.clone());
                }
            }
        }
        Ok(())
    })?;
    ok()
}

pub fn register(r: &mut Registry) {
    cmd("table.get", "Table").needs(Needs::Text).query().sig("{}").reg(r, get);
    cmd("table.select", "Select Cells").needs(Needs::Text).query().sig("{row, col, r1?, c1?, add?: bool (add this range to the selection, or remove it if already selected), rows?: [row numbers] (select whole rows that need not be adjacent)}").help("Selects a range of cells in the current table. add keeps the ranges already selected, so rows, columns or cells that are not next to each other can be selected together (Mod+click in a cell does the same); rows selects whole rows by number in one call. Cell Options, Row Height, Column Width, Cell Strokes, Apply Cell Style, Delete Row and Delete Column act on every selected range.").reg(r, select);
    cmd("table.insert", "Insert Table...").menu(&["Table", "Insert Table..."]).key("Mod+Alt+Shift+T").needs(Needs::Text).sig("{rows, cols, headerRows?, footerRows?, style?}").reg(r, insert);
    cmd("table.options", "Table Options").menu(&["Table", "Table Options..."]).needs(Needs::Text).sig("{headerRows?, footerRows?, border?: {weight, color}, altRows?: {first, next, firstColor, firstTint, nextColor, nextTint, skipFirst, skipLast}, altCols?: {first, next, firstColor, firstTint, nextColor, nextTint, skipFirst, skipLast}, altRowStrokes?: {first, next, firstWeight, firstColor, firstTint, firstType, nextWeight, nextColor, nextTint, nextType, skipFirst, skipLast}, altColStrokes?: {same as altRowStrokes}, headerCellStyle?: cell style name, footerCellStyle?: cell style name, leftColumnCellStyle?: cell style name, rightColumnCellStyle?: cell style name, bodyCellStyle?: cell style name, cornerRadius?: pt (rounded table corners; 0 square), splitRows?: bool (let a row break across frames and pages), continued?: label shown at the top of each later frame (for example Table 3, continued), alt?: alternative text, summary?: a summary or caption for readers and screen readers}").help("Table options of the selected table; each key overrides the applied table style and null clears it. cornerRadius rounds the table's outer corners in every frame it runs through; splitRows lets a row that does not fit break at a line of text and continue in the next frame, under repeated header rows, instead of moving whole; continued adds a full-width label row above the repeated headers in every frame after the first; alt and summary describe the table in HTML, EPUB and website exports. Alternating patterns count rows or columns in groups of first then next, skipFirst and skipLast leave rows at the start and end of the body out; row strokes own a row's top and bottom edges, column strokes the left and right edges.").reg(r, options);
    cmd("table.setCell", "Cell Options").menu(&["Table", "Cell Options..."]).needs(Needs::Text).sig("{vj?, insets?, fill?, tint?, fillOpacity?: 0..100, rotate?: 0|90|180|270, slant?: 10..80 degrees (angled header text; 0 straight),diagonal?: {down?: stroke, up?: stroke, front?}, paraStyle?}").reg(r, |c, p| each(c, p, set_cell, |_| 0, true));
    cmd("table.setRowHeight", "Row Height").needs(Needs::Text).sig("{height, mode?: atLeast|exactly}").reg(r, |c, p| each(c, p, row_height, |_| 0, true));
    cmd("table.setColumnWidth", "Column Width").needs(Needs::Text).sig("{width}").reg(r, |c, p| each(c, p, col_width, |_| 0, true));
    cmd("table.insertRowAbove", "Insert Row Above").menu(&["Table", "Insert", "Row Above"]).needs(Needs::Text).sig("{count?}").reg(r, row_above);
    cmd("table.insertRowBelow", "Insert Row Below").menu(&["Table", "Insert", "Row Below"]).key("Mod+9").needs(Needs::Text).sig("{count?}").reg(r, row_below);
    cmd("table.insertColumnLeft", "Insert Column Left").menu(&["Table", "Insert", "Column Left"]).needs(Needs::Text).sig("{count?}").reg(r, col_left);
    cmd("table.insertColumnRight", "Insert Column Right").menu(&["Table", "Insert", "Column Right"]).key("Mod+Alt+9").needs(Needs::Text).sig("{count?}").reg(r, col_right);
    cmd("table.deleteRow", "Delete Row").menu(&["Table", "Delete", "Row"]).needs(Needs::Text).sig("{}").reg(r, |c, p| each(c, p, delete_rows, |s| s.r0.min(s.r1), false));
    cmd("table.deleteColumn", "Delete Column").menu(&["Table", "Delete", "Column"]).needs(Needs::Text).sig("{}").reg(r, |c, p| each(c, p, delete_cols, |s| s.c0.min(s.c1), false));
    cmd("table.merge", "Merge Cells").menu(&["Table", "Merge Cells"]).needs(Needs::Text).sig("{}").reg(r, merge);
    cmd("table.unmerge", "Unmerge Cells").menu(&["Table", "Unmerge Cells"]).needs(Needs::Text).sig("{}").reg(r, unmerge);
    cmd("table.splitCell", "Split Cell").menu(&["Table", "Split Cell"]).needs(Needs::Text).sig("{direction?: horizontal|vertical}").reg(r, split_cell);
    cmd("table.convertCell", "Convert Cell Type").menu(&["Table", "Convert Cell Type"]).needs(Needs::Text).sig("{to: graphic|text, fitting?: fill|fit|center}").reg(r, convert_cell);
    cmd("table.placeInCell", "Place Image in Cell...").menu(&["Table", "Place Image in Cell..."]).needs(Needs::Text).sig("{path, fitting?: fill|fit|center, alt?, page?}").reg(r, place_in_cell);
    cmd("table.cellStrokes", "Cell Strokes").menu(&["Table", "Cell Strokes..."]).needs(Needs::Text).sig("{sides?: [top|bottom|left|right|innerHorizontal|innerVertical|outside|inside|all], weight?: pt, color?: swatch, tint?: 0..100, type?: solid|dashed|dotted|none, clear?}").reg(r, |c, p| each(c, p, cell_strokes, |_| 0, true));
    cmd("table.distributeColumns", "Distribute Columns Evenly").menu(&["Table", "Distribute Columns Evenly"]).needs(Needs::Text).sig("{}").reg(r, distribute);
    cmd("table.sortRows", "Sort").menu(&["Table", "Sort"]).needs(Needs::Text).sig("{column?, descending?}").reg(r, sort_rows);
    cmd("style.table.create", "New Table Style").sig("{name, border?: {weight, color}, altRows?: {first, next, firstColor, firstTint, nextColor, nextTint, skipFirst, skipLast}, altCols?: {same as altRows}, altRowStrokes?: {first, next, firstWeight, firstColor, firstTint, firstType, nextWeight, nextColor, nextTint, nextType, skipFirst, skipLast}, altColStrokes?: {same as altRowStrokes}, headerCellStyle?: cell style name, footerCellStyle?: cell style name, leftColumnCellStyle?: cell style name, rightColumnCellStyle?: cell style name, bodyCellStyle?: cell style name}").reg(r, table_style_create);
    cmd("style.table.apply", "Apply Table Style").needs(Needs::Text).sig("{name}").reg(r, table_style_apply);
    cmd("style.cell.create", "New Cell Style").sig("{name, fromSelection?, attrs?}").reg(r, cell_style_create);
    cmd("style.cell.apply", "Apply Cell Style").needs(Needs::Text).sig("{name}").reg(r, |c, p| each(c, p, cell_style_apply, |_| 0, true));
}
