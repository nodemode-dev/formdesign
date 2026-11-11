use std::collections::HashMap;
use std::io::{Cursor, Read};

use formdesign_model::attrs::Attrs;
use formdesign_model::story::{Para, Seg};
use quick_xml::Reader;
use quick_xml::events::Event;
use serde_json::json;

pub struct Imported {
    pub paras: Vec<Para>,
    pub para_styles: Vec<String>,
    pub char_styles: Vec<String>,
}

fn push_style(list: &mut Vec<String>, s: &str) {
    if !s.is_empty() && !list.iter().any(|x| x == s) {
        list.push(s.to_string());
    }
}

fn seg(text: String, style: Option<String>, over: Attrs) -> Seg {
    Seg::Text { text, style, over }
}

pub fn plain(text: &str) -> Imported {
    let t = text.replace("\r\n", "\n").replace('\r', "\n");
    let paras = t.split('\n').map(|l| Para::from_text("", l)).collect();
    Imported { paras, para_styles: Vec::new(), char_styles: Vec::new() }
}

pub fn is_tagged(text: &str) -> bool {
    let head = text.trim_start_matches('\u{feff}').trim_start();
    head.starts_with("<ASCII-") || head.starts_with("<UNICODE-") || head.starts_with("<ANSI-")
}

pub fn tagged(text: &str) -> Imported {
    let body = text.trim_start_matches('\u{feff}');
    let mut out = Imported { paras: Vec::new(), para_styles: Vec::new(), char_styles: Vec::new() };
    let mut lines: Vec<&str> = body.split(['\r', '\n']).collect();
    if lines.first().is_some_and(|l| is_tagged(l)) {
        let first = lines[0];
        let rest = first.find('>').map(|i| &first[i + 1..]).unwrap_or("");
        lines[0] = rest;
        if rest.is_empty() {
            lines.remove(0);
        }
    }
    while lines.last().is_some_and(|l| l.is_empty()) && lines.len() > 1 {
        lines.pop();
    }
    for line in lines {
        let mut para_style = String::new();
        let mut cstyle: Option<String> = None;
        let mut segs: Vec<Seg> = Vec::new();
        let mut buf = String::new();
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        let flush = |segs: &mut Vec<Seg>, buf: &mut String, cs: &Option<String>| {
            if !buf.is_empty() {
                segs.push(seg(std::mem::take(buf), cs.clone(), Attrs::new()));
            }
        };
        while i < chars.len() {
            let ch = chars[i];
            if ch == '\\' && i + 1 < chars.len() {
                buf.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if ch == '<' {
                let end = chars[i..].iter().position(|c| *c == '>').map(|p| i + p);
                if let Some(e) = end {
                    let tag: String = chars[i + 1..e].iter().collect();
                    i = e + 1;
                    if let Some(hex) = tag.strip_prefix("0x") {
                        if let Some(c) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) {
                            buf.push(c);
                        }
                    } else if let Some(n) = tag.strip_prefix("ParaStyle:") {
                        para_style = n.to_string();
                        push_style(&mut out.para_styles, n);
                    } else if let Some(n) = tag.strip_prefix("CharStyle:") {
                        flush(&mut segs, &mut buf, &cstyle);
                        cstyle = if n.is_empty() { None } else { Some(n.to_string()) };
                        push_style(&mut out.char_styles, n);
                    }
                    continue;
                }
            }
            buf.push(ch);
            i += 1;
        }
        flush(&mut segs, &mut buf, &cstyle);
        if segs.is_empty() {
            segs.push(Seg::text(""));
        }
        let mut p = Para { style: para_style, over: Attrs::new(), segs };
        p.normalize();
        out.paras.push(p);
    }
    if out.paras.is_empty() {
        out.paras.push(Para::from_text("", ""));
    }
    out
}

fn entity(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail[1..].find(';').map(|e| e + 1).filter(|e| *e <= 12);
        let decoded = end.and_then(|e| {
            let name = &tail[1..e];
            if let Some(h) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                u32::from_str_radix(h, 16).ok().and_then(char::from_u32)
            } else if let Some(d) = name.strip_prefix('#') {
                d.parse().ok().and_then(char::from_u32)
            } else {
                Some(match name {
                    "nbsp" => '\u{a0}',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    "amp" => '&',
                    "lsquo" => '\u{2018}',
                    "rsquo" => '\u{2019}',
                    "ldquo" => '\u{201C}',
                    "rdquo" => '\u{201D}',
                    "ndash" => '\u{2013}',
                    "mdash" => '\u{2014}',
                    "hellip" => '\u{2026}',
                    "bull" => '\u{2022}',
                    "copy" => '\u{a9}',
                    "reg" => '\u{ae}',
                    "trade" => '\u{2122}',
                    "euro" => '\u{20ac}',
                    "shy" => '\u{ad}',
                    _ => return None,
                })
            }
        });
        match (decoded, end) {
            (Some(c), Some(e)) => {
                out.push(c);
                rest = &tail[e + 1..];
            }
            _ => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn html(text: &str) -> Imported {
    let lower = text.to_ascii_lowercase();
    let start = lower.find("<body").and_then(|b| lower[b..].find('>').map(|e| b + e + 1)).unwrap_or(0);
    let end = lower.rfind("</body>").unwrap_or(text.len());
    let body = &text[start..end.max(start)];
    let mut out = Imported { paras: Vec::new(), para_styles: Vec::new(), char_styles: Vec::new() };
    let mut cur = String::new();
    let mut style = String::new();
    let mut i = 0;
    let bytes = body.as_bytes();
    let mut skip_depth = 0;
    let close = |out: &mut Imported, cur: &mut String, style: &str| {
        let t = entity(cur.split_whitespace().collect::<Vec<_>>().join(" ").as_str());
        if !t.is_empty() {
            out.paras.push(Para::from_text(style, &t));
            push_style(&mut out.para_styles, style);
        }
        cur.clear();
    };
    while i < bytes.len() {
        if bytes[i] == b'<'
            && let Some(e) = body[i..].find('>')
        {
            let tag = body[i + 1..i + e].trim().to_ascii_lowercase();
            i += e + 1;
            let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            let closing = tag.starts_with('/');
            if name == "script" || name == "style" {
                skip_depth = if closing { 0 } else { 1 };
                continue;
            }
            match name.as_str() {
                "p" | "div" | "li" | "tr" | "blockquote" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    close(&mut out, &mut cur, &style);
                    style = if closing {
                        String::new()
                    } else {
                        match name.as_str() {
                            "h1" => "Heading 1".into(),
                            "h2" => "Heading 2".into(),
                            "h3" => "Heading 3".into(),
                            "h4" | "h5" | "h6" => "Heading 4".into(),
                            "li" => "List".into(),
                            _ => String::new(),
                        }
                    };
                }
                "br" => close(&mut out, &mut cur, &style),
                "td" | "th" if !closing && !cur.is_empty() => cur.push('\t'),
                _ => {}
            }
            continue;
        }
        let next = body[i..].find('<').map(|p| i + p).unwrap_or(body.len());
        if skip_depth == 0 {
            cur.push_str(&body[i..next]);
        }
        i = next;
    }
    close(&mut out, &mut cur, &style);
    if out.paras.is_empty() {
        out.paras.push(Para::from_text("", ""));
    }
    out
}

fn pipe_cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().replace("**", "").replace("__", "")).collect()
}

/// A Markdown pipe table starting at `i`: a row of cells, a separator of dashes (with optional colons), then rows.
/// Returns the rows without the separator and how many lines it used.
fn pipe_table(lines: &[&str], i: usize) -> Option<(Vec<Vec<String>>, usize)> {
    let head = lines.get(i)?.trim();
    let sep = lines.get(i + 1)?.trim();
    if !head.starts_with('|') || !sep.starts_with('|') || !sep.contains('-') || !sep.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
        return None;
    }
    let mut rows = vec![pipe_cells(head)];
    let mut k = i + 2;
    while k < lines.len() && lines[k].trim().starts_with('|') {
        rows.push(pipe_cells(lines[k]));
        k += 1;
    }
    Some((rows, k - i))
}

pub fn markdown(text: &str) -> Imported {
    let mut out = Imported { paras: Vec::new(), para_styles: Vec::new(), char_styles: Vec::new() };
    let src = text.replace("\r\n", "\n");
    let lines: Vec<&str> = src.split('\n').collect();
    let mut li = 0;
    while li < lines.len() {
        if let Some((rows, used)) = pipe_table(&lines, li) {
            out.paras.extend(table_of(rows).paras);
            li += used;
            continue;
        }
        let line = lines[li];
        li += 1;
        let t = line.trim_end();
        let hashes = t.chars().take_while(|c| *c == '#').count();
        let (style, body) = if hashes > 0 && hashes <= 6 && t[hashes..].starts_with(' ') {
            (format!("Heading {}", hashes.min(4)), t[hashes + 1..].to_string())
        } else if let Some(b) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            ("List".to_string(), b.to_string())
        } else {
            (String::new(), t.to_string())
        };
        let mut segs = Vec::new();
        let mut bold = false;
        let mut italic = false;
        let mut buf = String::new();
        let mut chars = body.chars().peekable();
        let emit = |segs: &mut Vec<Seg>, buf: &mut String, bold: bool, italic: bool| {
            if buf.is_empty() {
                return;
            }
            let mut over = Attrs::new();
            match (bold, italic) {
                (true, true) => {
                    over.insert("fontStyle".into(), json!("Bold Italic"));
                }
                (true, false) => {
                    over.insert("fontStyle".into(), json!("Bold"));
                }
                (false, true) => {
                    over.insert("fontStyle".into(), json!("Italic"));
                }
                _ => {}
            }
            segs.push(seg(std::mem::take(buf), None, over));
        };
        let mut prev: Option<char> = None;
        while let Some(c) = chars.next() {
            if c == '_' && prev.is_some_and(char::is_alphanumeric) && chars.peek().is_some_and(|n| n.is_alphanumeric()) {
                buf.push(c);
            } else if (c == '*' || c == '_') && chars.peek() == Some(&c) {
                chars.next();
                emit(&mut segs, &mut buf, bold, italic);
                bold = !bold;
            } else if c == '*' || c == '_' {
                emit(&mut segs, &mut buf, bold, italic);
                italic = !italic;
            } else {
                buf.push(c);
            }
            prev = Some(c);
        }
        emit(&mut segs, &mut buf, bold, italic);
        if segs.is_empty() {
            segs.push(Seg::text(""));
        }
        push_style(&mut out.para_styles, &style);
        out.paras.push(Para { style, over: Attrs::new(), segs });
    }
    out
}

pub fn rtf(text: &str) -> Imported {
    let mut out = Imported { paras: Vec::new(), para_styles: Vec::new(), char_styles: Vec::new() };
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut cur = String::new();
    let mut skip: Vec<bool> = vec![false];
    let mut uc_skip = 1usize;
    let mut cp1252 = true;
    while i < chars.len() {
        let c = chars[i];
        let skipping = *skip.last().unwrap_or(&false);
        match c {
            '{' => {
                skip.push(skipping);
                i += 1;
                if chars.get(i) == Some(&'\\')
                    && chars.get(i + 1) == Some(&'*')
                    && let Some(l) = skip.last_mut()
                {
                    *l = true;
                }
            }
            '}' => {
                skip.pop();
                if skip.is_empty() {
                    skip.push(false);
                }
                i += 1;
            }
            '\\' => {
                i += 1;
                let Some(&n) = chars.get(i) else { break };
                if !n.is_ascii_alphabetic() {
                    i += 1;
                    match n {
                        '\'' => {
                            let hex: String = chars.get(i..i + 2).map(|s| s.iter().collect()).unwrap_or_default();
                            i += 2;
                            if !skipping
                                && let Ok(b) = u8::from_str_radix(&hex, 16)
                            {
                                const HIGH: [u32; 32] = [
                                    0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
                                ];
                                let ch = if cp1252 && (0x80..0xA0).contains(&b) { char::from_u32(HIGH[(b - 0x80) as usize]).unwrap_or(b as char) } else { b as char };
                                cur.push(ch);
                            }
                        }
                        '~' if !skipping => cur.push('\u{a0}'),
                        '-' | '*' => {
                            if n == '*'
                                && let Some(l) = skip.last_mut()
                            {
                                *l = true;
                            }
                        }
                        _ if !skipping => cur.push(n),
                        _ => {}
                    }
                    continue;
                }
                let word: String = chars[i..].iter().take_while(|c| c.is_ascii_alphabetic()).collect();
                i += word.len();
                let num: String = chars[i..].iter().take_while(|c| c.is_ascii_digit() || **c == '-').collect();
                i += num.len();
                if chars.get(i) == Some(&' ') {
                    i += 1;
                }
                if skipping {
                    continue;
                }
                match word.as_str() {
                    "par" | "line" => out.paras.push(Para::from_text("", &std::mem::take(&mut cur))),
                    "tab" => cur.push('\t'),
                    "emdash" => cur.push('\u{2014}'),
                    "endash" => cur.push('\u{2013}'),
                    "lquote" => cur.push('\u{2018}'),
                    "rquote" => cur.push('\u{2019}'),
                    "ldblquote" => cur.push('\u{201C}'),
                    "rdblquote" => cur.push('\u{201D}'),
                    "bullet" => cur.push('\u{2022}'),
                    "uc" => uc_skip = num.parse().unwrap_or(1),
                    "ansicpg" => cp1252 = num == "1252",
                    "u" => {
                        if let Some(ch) = num.parse::<i32>().ok().map(|v| if v < 0 { v + 65536 } else { v }).and_then(|v| char::from_u32(v as u32)) {
                            cur.push(ch);
                        }
                        let mut left = uc_skip;
                        while left > 0 && i < chars.len() && chars[i] != '{' && chars[i] != '}' {
                            if chars[i] != '\\' {
                                i += 1;
                            } else if chars.get(i + 1) == Some(&'\'') {
                                i += 4;
                            } else if chars.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic()) {
                                i += 1;
                                while chars.get(i).is_some_and(|c| c.is_ascii_alphabetic()) {
                                    i += 1;
                                }
                                while chars.get(i).is_some_and(|c| c.is_ascii_digit() || *c == '-') {
                                    i += 1;
                                }
                                if chars.get(i) == Some(&' ') {
                                    i += 1;
                                }
                            } else {
                                i += 2;
                            }
                            left -= 1;
                        }
                        i = i.min(chars.len());
                    }
                    "fonttbl" | "colortbl" | "stylesheet" | "info" | "pict" | "header" | "footer" => {
                        if let Some(l) = skip.last_mut() {
                            *l = true;
                        }
                    }
                    _ => {}
                }
            }
            '\r' | '\n' => i += 1,
            _ => {
                if !skipping {
                    cur.push(c);
                }
                i += 1;
            }
        }
    }
    if !cur.is_empty() || out.paras.is_empty() {
        out.paras.push(Para::from_text("", &cur));
    }
    out
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == key).and_then(|a| a.unescape_value().ok()).map(|v| v.to_string())
}

pub fn docx(bytes: &[u8]) -> Result<Imported, String> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("not a Word file: {e}"))?;
    let mut read = |name: &str| -> Option<String> {
        let cap: u64 = 64 << 20;
        let mut f = z.by_name(name).ok().filter(|f| f.size() <= cap)?;
        let mut s = String::new();
        (&mut f).take(cap + 1).read_to_string(&mut s).ok()?;
        Some(s).filter(|s| s.len() as u64 <= cap)
    };
    let doc = read("word/document.xml").ok_or("not a Word file: no word/document.xml")?;
    let styles_xml = read("word/styles.xml").unwrap_or_default();
    let mut names: HashMap<String, String> = HashMap::new();
    {
        let mut r = Reader::from_str(&styles_xml);
        let mut cur_id: Option<String> = None;
        loop {
            match r.read_event() {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.name().as_ref() {
                    b"w:style" => cur_id = attr(&e, b"w:styleId"),
                    b"w:name" => {
                        if let (Some(id), Some(n)) = (&cur_id, attr(&e, b"w:val")) {
                            names.insert(id.clone(), n);
                        }
                    }
                    _ => {}
                },
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }
    let mut out = Imported { paras: Vec::new(), para_styles: Vec::new(), char_styles: Vec::new() };
    let mut r = Reader::from_str(&doc);
    let mut para: Option<Para> = None;
    let mut run_over = Attrs::new();
    let mut in_text = false;
    let mut bold = false;
    let mut italic = false;
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"w:p" => para = Some(Para { style: String::new(), over: Attrs::new(), segs: Vec::new() }),
                b"w:r" => {
                    bold = false;
                    italic = false;
                }
                b"w:t" => in_text = true,
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"w:pStyle" => {
                    if let (Some(p), Some(v)) = (para.as_mut(), attr(&e, b"w:val")) {
                        let n = names.get(&v).cloned().unwrap_or(v);
                        let n = if n.eq_ignore_ascii_case("normal") { String::new() } else { n };
                        push_style(&mut out.para_styles, &n);
                        p.style = n;
                    }
                }
                b"w:b" => bold = attr(&e, b"w:val").is_none_or(|v| v != "0" && v != "false"),
                b"w:i" => italic = attr(&e, b"w:val").is_none_or(|v| v != "0" && v != "false"),
                b"w:tab" => {
                    if let Some(p) = para.as_mut() {
                        p.segs.push(seg("\t".into(), None, run_over.clone()));
                    }
                }
                b"w:br" | b"w:cr" => {
                    if let Some(p) = para.as_mut() {
                        p.segs.push(seg("\u{2028}".into(), None, run_over.clone()));
                    }
                }
                b"w:p" => out.paras.push(Para::from_text("", "")),
                _ => {}
            },
            Ok(Event::Text(t)) if in_text => {
                if let Some(p) = para.as_mut() {
                    let s = t.decode().map(|c| c.into_owned()).unwrap_or_default();
                    let s = quick_xml::escape::unescape(&s).map(|c| c.into_owned()).unwrap_or(s);
                    run_over.clear();
                    match (bold, italic) {
                        (true, true) => {
                            run_over.insert("fontStyle".into(), json!("Bold Italic"));
                        }
                        (true, false) => {
                            run_over.insert("fontStyle".into(), json!("Bold"));
                        }
                        (false, true) => {
                            run_over.insert("fontStyle".into(), json!("Italic"));
                        }
                        _ => {}
                    }
                    p.segs.push(seg(s, None, run_over.clone()));
                }
            }
            Ok(Event::GeneralRef(g)) if in_text => {
                if let Some(p) = para.as_mut() {
                    let name = g.decode().map(|c| c.into_owned()).unwrap_or_default();
                    let s = entity(&format!("&{name};"));
                    p.segs.push(seg(s, None, run_over.clone()));
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"w:t" => in_text = false,
                b"w:p" => {
                    if let Some(mut p) = para.take() {
                        if p.segs.is_empty() {
                            p.segs.push(Seg::text(""));
                        }
                        p.normalize();
                        out.paras.push(p);
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("Word file: {e}")),
            _ => {}
        }
    }
    if out.paras.is_empty() {
        out.paras.push(Para::from_text("", ""));
    }
    Ok(out)
}

fn table_of(rows: Vec<Vec<String>>) -> Imported {
    let rows: Vec<Vec<String>> = rows.into_iter().filter(|r| r.iter().any(|c| !c.trim().is_empty())).collect();
    let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let rows = &rows[..rows.len().min((1_000_000 / cols).max(1))];
    let mut table = formdesign_model::story::Table::new(rows.len().max(1), cols, 432.0);
    table.header_rows = usize::from(rows.len() > 1);
    for (r, row) in rows.iter().enumerate() {
        for (k, text) in row.iter().enumerate() {
            table.cells[r * cols + k].paras = vec![Para::from_text("", text.trim())];
        }
    }
    let para = Para { style: String::new(), over: Attrs::new(), segs: vec![Seg::Inline { inline: formdesign_model::story::Inline::Table { table: Box::new(table) }, style: None, over: Attrs::new() }] };
    Imported { paras: vec![para], para_styles: Vec::new(), char_styles: Vec::new() }
}

fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let text = text.trim_start_matches('\u{FEFF}');
    let first = text.lines().next().unwrap_or("");
    let sep = if first.matches(';').count() > first.matches(',').count() { ';' } else if first.matches('\t').count() > first.matches(',').count() { '\t' } else { ',' };
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' if quoted => quoted = false,
            '"' if cur.is_empty() => quoted = true,
            c if c == sep && !quoted => row.push(std::mem::take(&mut cur)),
            '\r' if !quoted => {}
            '\n' if !quoted => {
                row.push(std::mem::take(&mut cur));
                rows.push(std::mem::take(&mut row));
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() || !row.is_empty() {
        row.push(cur);
        rows.push(row);
    }
    rows
}

fn xlsx(bytes: &[u8]) -> Result<Imported, String> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("spreadsheet: {e}"))?;
    let mut read = |name: &str| -> Option<String> {
        let cap: u64 = 64 << 20;
        let mut f = z.by_name(name).ok().filter(|f| f.size() <= cap)?;
        let mut s = String::new();
        (&mut f).take(cap + 1).read_to_string(&mut s).ok()?;
        Some(s).filter(|s| s.len() as u64 <= cap)
    };
    let shared_xml = read("xl/sharedStrings.xml").unwrap_or_default();
    let sheet = read("xl/worksheets/sheet1.xml").ok_or("spreadsheet: no first worksheet (xl/worksheets/sheet1.xml)")?;
    let mut shared: Vec<String> = Vec::new();
    let mut r = Reader::from_str(&shared_xml);
    let mut cur: Option<String> = None;
    let mut in_t = false;
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == b"si" => cur = Some(String::new()),
            Ok(Event::Start(e)) if e.name().as_ref() == b"t" => in_t = true,
            Ok(Event::End(e)) if e.name().as_ref() == b"t" => in_t = false,
            Ok(Event::Text(t)) if in_t => {
                if let Some(c) = cur.as_mut() {
                    c.push_str(&t.decode().map(|x| x.into_owned()).unwrap_or_default());
                }
            }
            Ok(Event::GeneralRef(g)) if in_t => {
                if let Some(c) = cur.as_mut() {
                    c.push_str(&entity(&format!("&{};", g.decode().map(|x| x.into_owned()).unwrap_or_default())));
                }
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"si" => shared.push(cur.take().unwrap_or_default()),
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    let col_of = |re: &str| -> usize {
        let letters: String = re.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
        letters.chars().fold(0usize, |acc, c| acc.saturating_mul(26).saturating_add(c.to_ascii_uppercase() as usize - 'A' as usize + 1)).saturating_sub(1)
    };
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut r = Reader::from_str(&sheet);
    let (mut col, mut kind, mut val, mut in_v) = (0usize, String::new(), String::new(), false);
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == b"row" => rows.push(Vec::new()),
            Ok(Event::Start(e)) if e.name().as_ref() == b"c" => {
                col = 0;
                kind.clear();
                val.clear();
                for a in e.attributes().flatten() {
                    let v = String::from_utf8_lossy(&a.value).to_string();
                    match a.key.as_ref() {
                        b"r" => col = col_of(&v),
                        b"t" => kind = v,
                        _ => {}
                    }
                }
            }
            Ok(Event::Start(e)) if e.name().as_ref() == b"v" || e.name().as_ref() == b"t" => in_v = true,
            Ok(Event::End(e)) if e.name().as_ref() == b"v" || e.name().as_ref() == b"t" => in_v = false,
            Ok(Event::Text(t)) if in_v => val.push_str(&t.decode().map(|x| x.into_owned()).unwrap_or_default()),
            Ok(Event::GeneralRef(g)) if in_v => val.push_str(&entity(&format!("&{};", g.decode().map(|x| x.into_owned()).unwrap_or_default()))),
            Ok(Event::End(e)) if e.name().as_ref() == b"c" => {
                let text = if kind == "s" { val.trim().parse::<usize>().ok().and_then(|i| shared.get(i).cloned()).unwrap_or_default() } else { val.clone() };
                if let Some(row) = rows.last_mut()
                    && col <= 16_383
                {
                    if row.len() <= col {
                        row.resize(col + 1, String::new());
                    }
                    row[col] = text;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("spreadsheet: {e}")),
            _ => {}
        }
    }
    Ok(table_of(rows))
}

pub fn csv(text: &str) -> Imported {
    table_of(csv_rows(text))
}

pub fn csv_as_tabs(text: &str) -> Imported {
    let rows: Vec<String> = text
        .replace("\r\n", "\n")
        .split('\n')
        .map(|line| {
            let mut cells = Vec::new();
            let mut cur = String::new();
            let mut quoted = false;
            let mut chars = line.chars().peekable();
            while let Some(c) = chars.next() {
                match c {
                    '"' if quoted && chars.peek() == Some(&'"') => {
                        cur.push('"');
                        chars.next();
                    }
                    '"' => quoted = !quoted,
                    ',' if !quoted => cells.push(std::mem::take(&mut cur)),
                    _ => cur.push(c),
                }
            }
            cells.push(cur);
            cells.join("\t")
        })
        .collect();
    plain(&rows.join("\n"))
}

pub fn import(name: &str, bytes: &[u8]) -> Result<Imported, String> {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let as_text = || String::from_utf8(bytes.to_vec()).unwrap_or_else(|_| bytes.iter().map(|b| *b as char).collect());
    Ok(match ext.as_str() {
        "docx" => docx(bytes)?,
        "rtf" => rtf(&as_text()),
        "html" | "htm" | "xhtml" => html(&as_text()),
        "md" | "markdown" => markdown(&as_text()),
        "csv" => csv(&as_text()),
        "xlsx" => xlsx(bytes)?,
        "txt" | "text" | "tag" | "tagged" => {
            let t = as_text();
            if is_tagged(&t) { tagged(&t) } else { plain(&t) }
        }
        other => return Err(format!("cannot place .{other} files as text")),
    })
}

pub fn is_text_name(name: &str) -> bool {
    matches!(name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str(), "docx" | "rtf" | "html" | "htm" | "xhtml" | "md" | "markdown" | "csv" | "xlsx" | "txt" | "text" | "tag" | "tagged")
}
