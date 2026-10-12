use std::collections::HashSet;
use std::sync::Arc;

use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::object::dict::keys::{BASE_STATE, CONTENTS, D, NAME, OCGS, OCPROPERTIES, OFF, ON};
use hayro::hayro_syntax::object::{Array, Dict, Name, ObjRef, Stream};

#[derive(Clone, Debug, PartialEq)]
pub struct PdfLayer {
    pub id: i32,
    pub name: String,
    pub visible: bool,
}

fn text_of(b: &[u8]) -> String {
    if b.starts_with(&[0xFE, 0xFF]) {
        let u: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        b.iter().map(|c| *c as char).collect()
    }
}

fn refs_of(arr: &Array<'_>) -> Vec<i32> {
    arr.raw_iter().filter_map(|o| o.as_obj_ref()).map(|r| r.obj_number).collect()
}

pub fn layers(data: &Arc<Vec<u8>>) -> Vec<PdfLayer> {
    let Ok(pdf) = Pdf::new(data.clone()) else { return Vec::new() };
    layers_of(&pdf)
}

fn layers_of(pdf: &Pdf) -> Vec<PdfLayer> {
    let xref = pdf.xref();
    let Some(catalog) = xref.get::<Dict<'_>>(xref.root_id()) else { return Vec::new() };
    let Some(props) = catalog.get::<Dict<'_>>(OCPROPERTIES) else { return Vec::new() };
    let Some(all) = props.get::<Array<'_>>(OCGS) else { return Vec::new() };
    let config = props.get::<Dict<'_>>(D).unwrap_or_default();
    let base_off = config.get::<Name<'_>>(BASE_STATE).is_some_and(|n| n.as_str() == "OFF");
    let off: HashSet<i32> = config.get::<Array<'_>>(OFF).map(|a| refs_of(&a)).unwrap_or_default().into_iter().collect();
    let on: HashSet<i32> = config.get::<Array<'_>>(ON).map(|a| refs_of(&a)).unwrap_or_default().into_iter().collect();
    let mut out = Vec::new();
    for r in all.raw_iter().filter_map(|o| o.as_obj_ref()) {
        let name = xref
            .get::<Dict<'_>>(r.into())
            .and_then(|d| d.get::<hayro::hayro_syntax::object::String<'_>>(NAME).map(|s| text_of(s.as_bytes())))
            .unwrap_or_else(|| format!("Layer {}", r.obj_number));
        let visible = if off.contains(&r.obj_number) { false } else if base_off { on.contains(&r.obj_number) } else { true };
        if !out.iter().any(|l: &PdfLayer| l.id == r.obj_number) {
            out.push(PdfLayer { id: r.obj_number, name, visible });
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
enum K {
    Operand,
    Word,
}

struct Tok {
    s: usize,
    e: usize,
    k: K,
}

fn is_ws(c: u8) -> bool {
    matches!(c, 0 | 9 | 10 | 12 | 13 | 32)
}

fn is_delim(c: u8) -> bool {
    matches!(c, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

fn skip_string(b: &[u8], mut i: usize) -> usize {
    let mut depth = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    b.len()
}

fn skip_nested(b: &[u8], mut i: usize) -> usize {
    let mut depth = 0i32;
    while i < b.len() {
        match b[i] {
            b'(' => {
                i = skip_string(b, i);
                continue;
            }
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            b'<' if b.get(i + 1) == Some(&b'<') => {
                depth += 1;
                i += 2;
                continue;
            }
            b'>' if b.get(i + 1) == Some(&b'>') => {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    return i;
                }
                continue;
            }
            b'<' => {
                while i < b.len() && b[i] != b'>' {
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    b.len()
}

fn tokens(b: &[u8]) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if is_ws(c) {
            i += 1;
        } else if c == b'%' {
            while i < b.len() && b[i] != b'\n' && b[i] != b'\r' {
                i += 1;
            }
        } else if c == b'(' {
            let e = skip_string(b, i);
            out.push(Tok { s: i, e, k: K::Operand });
            i = e;
        } else if c == b'[' || (c == b'<' && b.get(i + 1) == Some(&b'<')) {
            let e = skip_nested(b, i);
            out.push(Tok { s: i, e, k: K::Operand });
            i = e;
        } else if c == b'<' {
            let mut e = i;
            while e < b.len() && b[e] != b'>' {
                e += 1;
            }
            out.push(Tok { s: i, e: (e + 1).min(b.len()), k: K::Operand });
            i = e + 1;
        } else if c == b'/' {
            let mut e = i + 1;
            while e < b.len() && !is_ws(b[e]) && !is_delim(b[e]) {
                e += 1;
            }
            out.push(Tok { s: i, e, k: K::Operand });
            i = e;
        } else if is_delim(c) {
            i += 1;
        } else {
            let mut e = i;
            while e < b.len() && !is_ws(b[e]) && !is_delim(b[e]) {
                e += 1;
            }
            let w = &b[i..e];
            let numeric = w.first().is_some_and(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.')) || matches!(w, b"true" | b"false" | b"null");
            if w == b"BI" {
                let mut j = e;
                let mut end = b.len();
                while j < b.len() {
                    if let Some(p) = b[j..].windows(2).position(|x| x == b"ID") {
                        let at = j + p;
                        if at > 0 && is_ws(b[at - 1]) && b.get(at + 2).is_none_or(|c| is_ws(*c)) {
                            let mut q = at + 3;
                            while q + 2 <= b.len() {
                                if b[q] == b'E' && b[q + 1] == b'I' && is_ws(b[q - 1]) && b.get(q + 2).is_none_or(|c| is_ws(*c)) {
                                    end = q + 2;
                                    break;
                                }
                                q += 1;
                            }
                            break;
                        }
                        j = at + 2;
                    } else {
                        break;
                    }
                }
                out.push(Tok { s: i, e: end, k: K::Word });
                i = end;
                continue;
            }
            out.push(Tok { s: i, e, k: if numeric { K::Operand } else { K::Word } });
            i = e;
        }
    }
    out
}

struct Ctx<'a> {
    props: &'a Dict<'a>,
    xobjs: &'a Dict<'a>,
    pdf: &'a Pdf,
    show: &'a dyn Fn(i32) -> bool,
}

impl Ctx<'_> {
    fn group_visible(&self, id: i32) -> bool {
        (self.show)(id)
    }

    fn oc_visible(&self, r: ObjRef) -> bool {
        let xref = self.pdf.xref();
        if let Some(d) = xref.get::<Dict<'_>>(r.into())
            && d.get::<Name<'_>>(hayro::hayro_syntax::object::dict::keys::TYPE).is_some_and(|t| t.as_str() == "OCMD")
        {
            let ids: Vec<i32> = if let Some(a) = d.get::<Array<'_>>(OCGS) { refs_of(&a) } else { d.get_ref(OCGS).map(|r| vec![r.obj_number]).unwrap_or_default() };
            if ids.is_empty() {
                return true;
            }
            let policy = d.get::<Name<'_>>(hayro::hayro_syntax::object::dict::keys::P).map(|n| n.as_str().to_string()).unwrap_or_else(|| "AnyOn".into());
            return match policy.as_str() {
                "AllOn" => ids.iter().all(|i| self.group_visible(*i)),
                "AnyOff" => ids.iter().any(|i| !self.group_visible(*i)),
                "AllOff" => ids.iter().all(|i| !self.group_visible(*i)),
                _ => ids.iter().any(|i| self.group_visible(*i)),
            };
        }
        self.group_visible(r.obj_number)
    }
}

fn filter(content: &[u8], cx: &Ctx) -> Option<Vec<u8>> {
    let toks = tokens(content);
    let mut out: Vec<u8> = Vec::with_capacity(content.len());
    let mut stack: Vec<bool> = Vec::new();
    let mut operands: Vec<usize> = Vec::new();
    let mut changed = false;
    for (ti, t) in toks.iter().enumerate() {
        if t.k == K::Operand {
            operands.push(ti);
            continue;
        }
        let word = &content[t.s..t.e];
        let first = operands.first().map(|i| toks[*i].s).unwrap_or(t.s);
        let suppressed = stack.iter().any(|h| *h);
        let span = &content[first..t.e];
        match word {
            b"BMC" => {
                stack.push(false);
                if !suppressed {
                    out.extend_from_slice(span);
                    out.push(b'\n');
                }
            }
            b"BDC" => {
                let is_oc = operands.len() == 2 && &content[toks[operands[0]].s..toks[operands[0]].e] == b"/OC";
                let mut hidden = false;
                let mut forced = false;
                if is_oc {
                    let nm = &content[toks[operands[1]].s + 1..toks[operands[1]].e];
                    if let Some(r) = cx.props.get_ref(nm) {
                        let shown = cx.oc_visible(r);
                        hidden = !shown;
                        forced = shown;
                    }
                }
                stack.push(hidden);
                if forced {
                    changed = true;
                    if !suppressed {
                        out.extend_from_slice(b"/Layer BMC\n");
                    }
                } else if hidden {
                    changed = true;
                } else if !suppressed {
                    out.extend_from_slice(span);
                    out.push(b'\n');
                }
            }
            b"EMC" => {
                if !stack.is_empty() && !suppressed {
                    out.extend_from_slice(span);
                    out.push(b'\n');
                }
                stack.pop();
            }
            b"Do" => {
                let mut drop = false;
                if let Some(o) = operands.last() {
                    let nm = &content[toks[*o].s + 1..toks[*o].e];
                    if let Some(x) = cx.xobjs.get::<Stream<'_>>(nm)
                        && let Some(r) = x.dict().get_ref(hayro::hayro_syntax::object::dict::keys::OC)
                    {
                        drop = !cx.oc_visible(r);
                    }
                }
                if drop {
                    changed = true;
                } else if !suppressed {
                    out.extend_from_slice(span);
                    out.push(b'\n');
                }
            }
            _ => {
                if !suppressed {
                    out.extend_from_slice(span);
                    out.push(b'\n');
                }
            }
        }
        operands.clear();
    }
    changed.then_some(out)
}

fn size_of(data: &[u8]) -> i64 {
    let mut best = 0i64;
    let mut i = 0;
    while let Some(p) = data[i..].windows(5).position(|w| w == b"/Size") {
        let at = i + p + 5;
        let mut j = at;
        while j < data.len() && is_ws(data[j]) {
            j += 1;
        }
        let mut k = j;
        while k < data.len() && data[k].is_ascii_digit() {
            k += 1;
        }
        if let Some(n) = std::str::from_utf8(&data[j..k]).ok().and_then(|s| s.parse::<i64>().ok()) {
            best = best.max(n);
        }
        i = at;
    }
    best
}

fn last_startxref(data: &[u8]) -> Option<u64> {
    let p = data.windows(9).rposition(|w| w == b"startxref")?;
    let mut j = p + 9;
    while j < data.len() && is_ws(data[j]) {
        j += 1;
    }
    let mut k = j;
    while k < data.len() && data[k].is_ascii_digit() {
        k += 1;
    }
    std::str::from_utf8(&data[j..k]).ok()?.parse().ok()
}

pub fn apply(data: &Arc<Vec<u8>>, page: usize, show: &dyn Fn(i32) -> bool) -> Option<Vec<u8>> {
    let pdf = Pdf::new(data.clone()).ok()?;
    let pages = pdf.pages();
    let pg = pages.get(page).or_else(|| pages.first())?;
    let page_id = pg.raw().obj_id()?;
    let content = pg.page_stream()?;
    let res = pg.resources();
    let cx = Ctx { props: &res.properties, xobjs: &res.x_objects, pdf: &pdf, show };
    let filtered = filter(content, &cx)?;
    let raw = pg.raw().data();
    let open = raw.windows(2).position(|w| w == b"<<")?;
    let close = raw.windows(2).rposition(|w| w == b">>")?;
    if close <= open {
        return None;
    }
    let body = &raw[open + 2..close];
    let next = size_of(data).max(pdf.len() as i64 + 1);
    let prev = last_startxref(data)?;
    let root = pdf.xref().root_id();
    let mut out: Vec<u8> = data.as_ref().clone();
    if out.last() != Some(&b'\n') {
        out.push(b'\n');
    }
    let stream_off = out.len();
    out.extend_from_slice(format!("{next} 0 obj\n<< /Length {} >>\nstream\n", filtered.len()).as_bytes());
    out.extend_from_slice(&filtered);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    let page_off = out.len();
    out.extend_from_slice(format!("{} {} obj\n<<", page_id.obj_number, page_id.gen_number).as_bytes());
    let mut body = body.to_vec();
    strip_key(&mut body, CONTENTS);
    out.extend_from_slice(&body);
    out.extend_from_slice(format!(" /Contents {next} 0 R >>\nendobj\n").as_bytes());
    let xref_off = out.len();
    let mut entries = vec![(page_id.obj_number as i64, page_off, page_id.gen_number), (next, stream_off, 0)];
    entries.sort_by_key(|e| e.0);
    out.extend_from_slice(b"xref\n0 1\n0000000000 65535 f \n");
    for (n, off, gen_) in entries {
        out.extend_from_slice(format!("{n} 1\n{off:010} {gen_:05} n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root {} {} R /Prev {prev} >>\nstartxref\n{xref_off}\n%%EOF\n", next + 1, root.obj_number, root.gen_number).as_bytes());
    Some(out)
}

fn strip_key(body: &mut Vec<u8>, key: &[u8]) {
    let needle: Vec<u8> = std::iter::once(b'/').chain(key.iter().copied()).collect();
    let mut i = 0;
    while i + needle.len() <= body.len() {
        let top = depth_at(body, i) == 0;
        if top && body[i..].starts_with(&needle) && body.get(i + needle.len()).is_none_or(|c| is_ws(*c) || is_delim(*c)) {
            let mut j = i + needle.len();
            while j < body.len() && is_ws(body[j]) {
                j += 1;
            }
            let end = value_end(body, j);
            body.drain(i..end);
            return;
        }
        i += 1;
    }
}

fn depth_at(b: &[u8], upto: usize) -> i32 {
    let mut depth = 0i32;
    let mut i = 0;
    while i < upto.min(b.len()) {
        match b[i] {
            b'(' => {
                i = skip_string(b, i);
                continue;
            }
            b'<' if b.get(i + 1) == Some(&b'<') => {
                depth += 1;
                i += 2;
                continue;
            }
            b'>' if b.get(i + 1) == Some(&b'>') => {
                depth -= 1;
                i += 2;
                continue;
            }
            b'[' => depth += 1,
            b']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    depth
}

fn value_end(b: &[u8], i: usize) -> usize {
    if i >= b.len() {
        return b.len();
    }
    match b[i] {
        b'[' | b'(' => skip_nested_or_string(b, i),
        b'<' => skip_nested(b, i),
        b'/' => {
            let mut e = i + 1;
            while e < b.len() && !is_ws(b[e]) && !is_delim(b[e]) {
                e += 1;
            }
            e
        }
        _ => {
            let mut e = i;
            while e < b.len() && !is_ws(b[e]) && !is_delim(b[e]) {
                e += 1;
            }
            let word_end = e;
            let mut k = e;
            let mut gen_seen = false;
            loop {
                while k < b.len() && is_ws(b[k]) {
                    k += 1;
                }
                let s = k;
                while k < b.len() && !is_ws(b[k]) && !is_delim(b[k]) {
                    k += 1;
                }
                if s == k {
                    break;
                }
                let w = &b[s..k];
                if !gen_seen && w.iter().all(|c| c.is_ascii_digit()) {
                    gen_seen = true;
                } else if gen_seen && w == b"R" {
                    return k;
                } else {
                    break;
                }
            }
            word_end
        }
    }
}

fn skip_nested_or_string(b: &[u8], i: usize) -> usize {
    if b[i] == b'(' { skip_string(b, i) } else { skip_nested(b, i) }
}

pub fn for_export(data: &Arc<Vec<u8>>, page: u32) -> Arc<Vec<u8>> {
    let ls = layers(data);
    if ls.iter().all(|l| l.visible) {
        return data.clone();
    }
    let vis: HashSet<i32> = ls.iter().filter(|l| l.visible).map(|l| l.id).collect();
    match apply(data, page as usize, &|id| vis.contains(&id)) {
        Some(v) => Arc::new(v),
        None => data.clone(),
    }
}
