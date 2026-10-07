use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::Result;
use crate::session::Ctx;

pub type Handler = fn(&mut Ctx, &Value) -> Result<Value>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    pub name: &'static str,
    pub kind: &'static str,
    pub label: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<&'static str>,
    pub required: bool,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Needs {
    Nothing,
    Document,
    Selection,
    Items,
    Text,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    pub id: &'static str,
    pub label: &'static str,
    pub menu: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<&'static str>,
    pub params: Vec<Param>,
    pub needs: Needs,
    pub undoable: bool,
    #[serde(skip_serializing_if = "str::is_empty")]
    pub help: &'static str,
    pub sig: &'static str,
}

#[derive(Default)]
pub struct Registry {
    map: BTreeMap<&'static str, (Spec, Handler)>,
}

impl Registry {
    pub fn add(&mut self, spec: Spec, h: Handler) {
        self.map.insert(spec.id, (spec, h));
    }

    pub fn get(&self, id: &str) -> Option<&(Spec, Handler)> {
        self.map.get(id)
    }

    pub fn specs(&self) -> Vec<&Spec> {
        self.map.values().map(|(s, _)| s).collect()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

pub struct SpecBuilder(Spec);

pub fn cmd(id: &'static str, label: &'static str) -> SpecBuilder {
    SpecBuilder(Spec { id, label, menu: Vec::new(), shortcut: None, params: Vec::new(), needs: Needs::Document, undoable: true, help: "", sig: "{}" })
}

impl SpecBuilder {
    pub fn menu(mut self, path: &[&'static str]) -> Self {
        self.0.menu = path.to_vec();
        self
    }

    pub fn key(mut self, k: &'static str) -> Self {
        self.0.shortcut = Some(k);
        self
    }

    pub fn needs(mut self, n: Needs) -> Self {
        self.0.needs = n;
        self
    }

    pub fn query(mut self) -> Self {
        self.0.undoable = false;
        self
    }

    pub fn sig(mut self, s: &'static str) -> Self {
        self.0.sig = s;
        self
    }

    pub fn reg(self, r: &mut Registry, h: Handler) {
        r.add(self.0, h);
    }

    pub fn help(mut self, h: &'static str) -> Self {
        self.0.help = h;
        self
    }

    pub fn p(mut self, name: &'static str, kind: &'static str, label: &'static str) -> Self {
        self.0.params.push(Param { name, kind, label, default: None, choices: Vec::new(), required: false });
        self
    }

    pub fn req(mut self, name: &'static str, kind: &'static str, label: &'static str) -> Self {
        self.0.params.push(Param { name, kind, label, default: None, choices: Vec::new(), required: true });
        self
    }

    pub fn choice(mut self, name: &'static str, label: &'static str, choices: &[&'static str]) -> Self {
        self.0.params.push(Param { name, kind: "choice", label, default: choices.first().map(|c| Value::String(c.to_string())), choices: choices.to_vec(), required: false });
        self
    }

    pub fn dflt(mut self, v: Value) -> Self {
        if let Some(p) = self.0.params.last_mut() {
            p.default = Some(v);
        }
        self
    }

    pub fn build(self) -> Spec {
        self.0
    }
}
