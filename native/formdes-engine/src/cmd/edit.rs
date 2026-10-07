use serde_json::{Value, json};

use crate::args;
use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection};
use crate::Result;

fn undo(c: &mut Ctx, _: &Value) -> Result<Value> {
    c.session.undo()
}

fn redo(c: &mut Ctx, _: &Value) -> Result<Value> {
    c.session.redo()
}

fn deselect_all(c: &mut Ctx, _: &Value) -> Result<Value> {
    c.set_selection(Selection::default());
    Ok(json!({"items": []}))
}

fn select_set(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = crate::target::ids_param(p, "ids").or_else(|| crate::target::ids_param(p, "items")).unwrap_or_default();
    let doc = c.doc()?;
    let mut ids: Vec<_> = ids.into_iter().filter(|i| doc.item(*i).is_some()).collect();
    if args::opt_bool(p, "add") == Some(true) {
        let mut cur = c.selection().items;
        for i in ids {
            if !cur.contains(&i) {
                cur.push(i);
            }
        }
        ids = cur;
    }
    c.set_selection(Selection::items(ids.clone()));
    Ok(json!({"items": ids}))
}

fn select_toggle(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = crate::target::ids_param(p, "id").or_else(|| crate::target::ids_param(p, "ids")).or_else(|| crate::target::ids_param(p, "items")).unwrap_or_default();
    let mut sel = c.selection();
    sel.text = None;
    sel.cells = None;
    for id in ids {
        if let Some(i) = sel.items.iter().position(|x| *x == id) {
            sel.items.remove(i);
        } else if c.doc()?.item(id).is_some() {
            sel.items.push(id);
        }
    }
    let out = json!({"items": sel.items});
    c.set_selection(sel);
    Ok(out)
}

fn script_run(c: &mut Ctx, p: &Value) -> Result<Value> {
    let cmds: Vec<Value> = match (p.get("commands"), p.get("text").and_then(Value::as_str)) {
        (Some(Value::Array(a)), _) => a.clone(),
        (_, Some(t)) => match serde_json::from_str::<Value>(t) {
            Ok(Value::Array(a)) => a,
            Ok(Value::Object(o)) => o.get("commands").and_then(Value::as_array).cloned().ok_or_else(|| crate::EngineError::new("the script expected a JSON array of {command, params}"))?,
            Ok(_) => return Err(crate::EngineError::new("the script expected a JSON array of {command, params}")),
            Err(e) => return Err(crate::EngineError::new(format!("the script is not valid JSON: {e}"))),
        },
        _ => return Err(crate::EngineError::new("missing `text` (a JSON array of {command, params}) or `commands`")),
    };
    let label = args::opt_str(p, "label").map(str::to_string);
    c.session.batch(&cmds, label.as_deref())
}

pub fn register(r: &mut Registry) {
    r.add(cmd("edit.undo", "Undo").menu(&["Edit", "Undo"]).key("Mod+Z").query().build(), undo);
    r.add(cmd("edit.redo", "Redo").menu(&["Edit", "Redo"]).key("Mod+Shift+Z").query().build(), redo);
    r.add(cmd("edit.deselectAll", "Deselect All").menu(&["Edit", "Deselect All"]).key("Mod+Shift+A").query().build(), deselect_all);
    r.add(cmd("selection.set", "Select").sig("{ids: [id], add?}").query().build(), select_set);
    r.add(cmd("selection.toggle", "Toggle Selection").sig("{id}").needs(Needs::Document).query().build(), select_toggle);
    r.add(cmd("script.run", "Run Script").sig("{text: JSON array of {command, params}, label?} or {commands: [...]}").query().build(), script_run);
}
