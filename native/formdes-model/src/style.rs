use serde::{Deserialize, Serialize};

use crate::attrs::{Attrs, CharProps, ParaProps, Stack};
use crate::color::swatch_key;
use crate::{Result, err};

pub const BASIC_PARA: &str = "Basic Paragraph";
pub const NO_CHAR: &str = "None";
pub const BASIC_GRAPHIC: &str = "Basic Graphics Frame";
pub const BASIC_TEXT: &str = "Basic Text Frame";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ExportTag {
    pub tag: String,
    #[serde(default)]
    pub class: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedStyle {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(default)]
    pub attrs: Attrs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export: Option<ExportTag>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub xml_tag: String,
}

impl NamedStyle {
    pub fn new(name: &str) -> NamedStyle {
        NamedStyle { name: name.into(), based_on: None, next: None, attrs: Attrs::new(), group: None, export: None, xml_tag: String::new() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StyleKind {
    Paragraph,
    Character,
    Object,
    Table,
    Cell,
}

impl StyleKind {
    pub fn parse(s: &str) -> Option<StyleKind> {
        Some(match s {
            "paragraph" | "para" => StyleKind::Paragraph,
            "character" | "char" => StyleKind::Character,
            "object" => StyleKind::Object,
            "table" => StyleKind::Table,
            "cell" => StyleKind::Cell,
            _ => return None,
        })
    }

    pub fn base(self) -> &'static str {
        match self {
            StyleKind::Paragraph => BASIC_PARA,
            StyleKind::Character => NO_CHAR,
            StyleKind::Object => "None",
            StyleKind::Table => "Basic Table",
            StyleKind::Cell => "None",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Styles {
    pub paragraph: Vec<NamedStyle>,
    pub character: Vec<NamedStyle>,
    pub object: Vec<NamedStyle>,
    pub table: Vec<NamedStyle>,
    pub cell: Vec<NamedStyle>,
}

impl Default for Styles {
    fn default() -> Self {
        let mut basic = NamedStyle::new(BASIC_PARA);
        basic.attrs.insert("font".into(), serde_json::json!("Source Serif 4"));
        basic.attrs.insert("size".into(), serde_json::json!(12.0));
        Styles {
            paragraph: vec![NamedStyle::new("No Paragraph Style"), basic],
            character: vec![NamedStyle::new(NO_CHAR)],
            object: vec![NamedStyle::new("None"), NamedStyle::new(BASIC_GRAPHIC), NamedStyle::new(BASIC_TEXT)],
            table: vec![NamedStyle::new("No Table Style"), NamedStyle::new("Basic Table")],
            cell: vec![NamedStyle::new("None")],
        }
    }
}

pub fn style_key(name: &str) -> String {
    swatch_key(name)
}

/// Several character styles on the same text are kept in the run's one style name, bottom first, joined by this.
pub const CHAR_STACK: &str = " + ";

/// The character styles of a run's style name, bottom first, without [None].
pub fn char_stack_names(name: &str) -> Vec<String> {
    name.split(CHAR_STACK).map(style_key).filter(|s| !s.is_empty() && s != NO_CHAR).collect()
}

impl Styles {
    pub fn list(&self, kind: StyleKind) -> &Vec<NamedStyle> {
        match kind {
            StyleKind::Paragraph => &self.paragraph,
            StyleKind::Character => &self.character,
            StyleKind::Object => &self.object,
            StyleKind::Table => &self.table,
            StyleKind::Cell => &self.cell,
        }
    }

    pub fn list_mut(&mut self, kind: StyleKind) -> &mut Vec<NamedStyle> {
        match kind {
            StyleKind::Paragraph => &mut self.paragraph,
            StyleKind::Character => &mut self.character,
            StyleKind::Object => &mut self.object,
            StyleKind::Table => &mut self.table,
            StyleKind::Cell => &mut self.cell,
        }
    }

    pub fn find(&self, kind: StyleKind, name: &str) -> Option<&NamedStyle> {
        let k = style_key(name);
        self.list(kind).iter().find(|s| s.name == k)
    }

    pub fn find_mut(&mut self, kind: StyleKind, name: &str) -> Option<&mut NamedStyle> {
        let k = style_key(name);
        self.list_mut(kind).iter_mut().find(|s| s.name == k)
    }

    pub fn exists(&self, kind: StyleKind, name: &str) -> bool {
        self.find(kind, name).is_some()
    }

    pub fn chain(&self, kind: StyleKind, name: &str) -> Vec<&Attrs> {
        let mut out = Vec::new();
        let mut cur = self.find(kind, name);
        let mut guard = 0;
        while let Some(s) = cur {
            out.push(&s.attrs);
            guard += 1;
            if guard > 32 {
                break;
            }
            cur = s.based_on.as_deref().and_then(|b| self.find(kind, b));
        }
        if kind == StyleKind::Paragraph && !out.is_empty() && style_key(name) != BASIC_PARA && !self.chain_has(kind, name, BASIC_PARA)
            && let Some(b) = self.find(kind, BASIC_PARA)
        {
            out.push(&b.attrs);
        }
        out.reverse();
        out
    }

    fn chain_has(&self, kind: StyleKind, name: &str, target: &str) -> bool {
        let mut cur = self.find(kind, name);
        let mut guard = 0;
        while let Some(s) = cur {
            if s.name == target {
                return true;
            }
            guard += 1;
            if guard > 32 {
                return false;
            }
            cur = s.based_on.as_deref().and_then(|b| self.find(kind, b));
        }
        false
    }

    pub fn para_stack<'a>(&'a self, para_style: &str, para_over: &'a Attrs) -> Stack<'a> {
        let mut v = self.chain(StyleKind::Paragraph, para_style);
        if v.is_empty()
            && let Some(b) = self.find(StyleKind::Paragraph, BASIC_PARA)
        {
            v.push(&b.attrs);
        }
        v.push(para_over);
        Stack(v)
    }

    pub fn char_stack<'a>(&'a self, para_style: &str, para_over: &'a Attrs, char_style: Option<&str>, run_over: &'a Attrs) -> Stack<'a> {
        let mut s = self.para_stack(para_style, para_over);
        for cs in char_style.map(char_stack_names).unwrap_or_default() {
            s.0.extend(self.chain(StyleKind::Character, &cs));
        }
        s.0.push(run_over);
        s
    }

    pub fn para_props(&self, para_style: &str, para_over: &Attrs) -> ParaProps {
        ParaProps::resolve(&self.para_stack(para_style, para_over))
    }

    pub fn char_props(&self, para_style: &str, para_over: &Attrs, char_style: Option<&str>, run_over: &Attrs) -> CharProps {
        CharProps::resolve(&self.char_stack(para_style, para_over, char_style, run_over))
    }

    pub fn create(&mut self, kind: StyleKind, name: &str, based_on: Option<&str>) -> Result<&mut NamedStyle> {
        let k = style_key(name);
        if k.is_empty() {
            return err("a style needs a name");
        }
        if self.exists(kind, &k) {
            return err(format!("there is already a {} style named {k}", kind_word(kind)));
        }
        if let Some(b) = based_on
            && !self.exists(kind, b)
        {
            return err(format!("no {} style named {b} to base it on", kind_word(kind)));
        }
        let mut st = NamedStyle::new(&k);
        st.based_on = based_on.map(style_key);
        let list = self.list_mut(kind);
        list.push(st);
        Ok(list.last_mut().expect("just pushed"))
    }

    pub fn delete(&mut self, kind: StyleKind, name: &str, replace: Option<&str>) -> Result<String> {
        let k = style_key(name);
        if k == kind.base() || k == "No Paragraph Style" || k == "No Table Style" {
            return err(format!("{k} cannot be deleted"));
        }
        let repl = replace.map(style_key).unwrap_or_else(|| kind.base().to_string());
        let list = self.list_mut(kind);
        let before = list.len();
        list.retain(|s| s.name != k);
        if list.len() == before {
            return err(format!("no {} style named {k}", kind_word(kind)));
        }
        for s in list.iter_mut() {
            if s.based_on.as_deref() == Some(k.as_str()) {
                s.based_on = Some(repl.clone());
            }
            if s.next.as_deref() == Some(k.as_str()) {
                s.next = None;
            }
        }
        Ok(repl)
    }

    pub fn rename(&mut self, kind: StyleKind, from: &str, to: &str) -> Result<()> {
        let (f, t) = (style_key(from), style_key(to));
        if self.exists(kind, &t) {
            return err(format!("there is already a {} style named {t}", kind_word(kind)));
        }
        let Some(s) = self.find_mut(kind, &f) else { return err(format!("no {} style named {f}", kind_word(kind))) };
        s.name = t.clone();
        for s in self.list_mut(kind).iter_mut() {
            if s.based_on.as_deref() == Some(f.as_str()) {
                s.based_on = Some(t.clone());
            }
            if s.next.as_deref() == Some(f.as_str()) {
                s.next = Some(t.clone());
            }
        }
        Ok(())
    }
}

pub fn kind_word(kind: StyleKind) -> &'static str {
    match kind {
        StyleKind::Paragraph => "paragraph",
        StyleKind::Character => "character",
        StyleKind::Object => "object",
        StyleKind::Table => "table",
        StyleKind::Cell => "cell",
    }
}
