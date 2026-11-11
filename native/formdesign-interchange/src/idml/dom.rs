use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

#[derive(Debug, Clone)]
pub enum Node {
    El(El),
    Text(String),
    Pi(String),
}

#[derive(Debug, Clone, Default)]
pub struct El {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub kids: Vec<Node>,
}

impl El {
    pub fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
    }

    pub fn num(&self, k: &str) -> Option<f64> {
        self.attr(k).and_then(|v| v.trim().parse().ok())
    }

    pub fn flag(&self, k: &str) -> Option<bool> {
        self.attr(k).map(|v| v == "true")
    }

    pub fn els(&self) -> impl Iterator<Item = &El> {
        self.kids.iter().filter_map(|n| if let Node::El(e) = n { Some(e) } else { None })
    }

    pub fn child(&self, name: &str) -> Option<&El> {
        self.els().find(|e| e.name == name)
    }

    pub fn all(&self, name: &str) -> Vec<&El> {
        self.els().filter(|e| e.name == name).collect()
    }

    pub fn find(&self, name: &str) -> Option<&El> {
        for e in self.els() {
            if e.name == name {
                return Some(e);
            }
            if let Some(f) = e.find(name) {
                return Some(f);
            }
        }
        None
    }

    pub fn find_all<'a>(&'a self, name: &str, out: &mut Vec<&'a El>) {
        for e in self.els() {
            if e.name == name {
                out.push(e);
            }
            e.find_all(name, out);
        }
    }

    pub fn text(&self) -> String {
        let mut s = String::new();
        for k in &self.kids {
            match k {
                Node::Text(t) => s.push_str(t),
                Node::El(e) => s.push_str(&e.text()),
                Node::Pi(_) => {}
            }
        }
        s
    }

    pub fn prop(&self, name: &str) -> Option<&El> {
        self.child("Properties").and_then(|p| p.child(name))
    }
}

fn start(e: &BytesStart) -> El {
    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
    let attrs = e
        .attributes()
        .flatten()
        .map(|a| (String::from_utf8_lossy(a.key.as_ref()).to_string(), a.unescape_value().map(|v| v.to_string()).unwrap_or_else(|_| String::from_utf8_lossy(&a.value).to_string())))
        .collect();
    El { name, attrs, kids: Vec::new() }
}

fn entity(n: &str) -> String {
    match n {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        _ => {
            if let Some(h) = n.strip_prefix("#x").or_else(|| n.strip_prefix("#X")) {
                u32::from_str_radix(h, 16).ok().and_then(char::from_u32).map(String::from).unwrap_or_default()
            } else if let Some(d) = n.strip_prefix('#') {
                d.parse().ok().and_then(char::from_u32).map(String::from).unwrap_or_default()
            } else {
                String::new()
            }
        }
    }
}

pub fn parse(xml: &str) -> Result<El, String> {
    let mut r = Reader::from_str(xml);
    let mut stack: Vec<El> = vec![El { name: "#doc".into(), ..Default::default() }];
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) => {
                if stack.len() > 512 {
                    return Err("XML nested too deeply".into());
                }
                stack.push(start(&e));
            }
            Ok(Event::Empty(e)) => {
                let el = start(&e);
                if let Some(top) = stack.last_mut() {
                    top.kids.push(Node::El(el));
                }
            }
            Ok(Event::End(_)) => {
                if stack.len() > 1
                    && let Some(el) = stack.pop()
                    && let Some(top) = stack.last_mut()
                {
                    top.kids.push(Node::El(el));
                }
            }
            Ok(Event::Text(t)) => {
                let s = t.decode().map(|c| c.into_owned()).unwrap_or_default();
                if let Some(top) = stack.last_mut() {
                    top.kids.push(Node::Text(s));
                }
            }
            Ok(Event::CData(t)) => {
                let s = String::from_utf8_lossy(&t.into_inner()).to_string();
                if let Some(top) = stack.last_mut() {
                    top.kids.push(Node::Text(s));
                }
            }
            Ok(Event::GeneralRef(g)) => {
                let n = g.decode().map(|c| c.into_owned()).unwrap_or_default();
                if let Some(top) = stack.last_mut() {
                    top.kids.push(Node::Text(entity(&n)));
                }
            }
            Ok(Event::PI(p)) => {
                let s = String::from_utf8_lossy(&p.into_inner()).to_string();
                if let Some(top) = stack.last_mut() {
                    top.kids.push(Node::Pi(s));
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML at byte {}: {e}", r.buffer_position())),
            _ => {}
        }
    }
    while stack.len() > 1 {
        let el = stack.pop().unwrap_or_default();
        if let Some(top) = stack.last_mut() {
            top.kids.push(Node::El(el));
        }
    }
    Ok(stack.pop().unwrap_or_default())
}
