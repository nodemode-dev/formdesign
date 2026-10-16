use std::fmt::Write as _;
use std::io::{Cursor, Write};

use formdes_model::doc::Doc;
use formdes_model::ids::{AssetId, ItemId};
use formdes_model::item::ItemKind;
use formdes_model::story::{Inline, Para, Seg};
use formdes_model::style::StyleKind;

use crate::idml::b64;
use crate::idml::write::esc;

pub struct Opts {
    pub title: String,
    pub author: String,
    pub language: String,
}

enum Block {
    Story(Vec<Para>),
    Image(AssetId, String),
}

fn slug(s: &str) -> String {
    let mut o: String = s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    while o.contains("--") {
        o = o.replace("--", "-");
    }
    let o = o.trim_matches('-').to_string();
    if o.is_empty() { "p".into() } else { format!("s-{o}") }
}

fn order(doc: &Doc) -> Vec<Block> {
    let mut picks: Vec<(usize, f64, f64, ItemId)> = Vec::new();
    for (si, s) in doc.spreads.iter().enumerate() {
        for id in doc.walk(&s.items) {
            let Some(it) = doc.item(id) else { continue };
            if it.hidden || it.nonprinting {
                continue;
            }
            let keep = match &it.kind {
                ItemKind::Text(t) => doc.story(t.story).is_some_and(|st| st.frames.first() == Some(&id)),
                ItemKind::Graphic(g) => g.asset.is_some(),
                _ => false,
            };
            if !keep {
                continue;
            }
            let b = doc.bounds(id).unwrap_or_default();
            let page = doc.page_of(id).unwrap_or(doc.first_page_of_spread(si));
            picks.push((page, (b.y0 / 12.0).round(), b.x0, id));
        }
    }
    if !doc.articles.is_empty() {
        let mut out = Vec::new();
        for a in doc.articles.iter().filter(|a| a.export) {
            for id in &a.items {
                if let Some(b) = block(doc, *id) {
                    out.push(b);
                }
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    picks.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.total_cmp(&b.2)));
    picks.into_iter().filter_map(|p| block(doc, p.3)).collect()
}

fn block(doc: &Doc, id: ItemId) -> Option<Block> {
    let it = doc.item(id)?;
    match &it.kind {
        ItemKind::Text(t) => Some(Block::Story(doc.story(t.story)?.paras.clone())),
        ItemKind::Graphic(g) => Some(Block::Image(g.asset?, it.alt_text.clone())),
        _ => None,
    }
}

fn tag_for(doc: &Doc, style: &str) -> &'static str {
    if let Some(e) = doc.styles.find(StyleKind::Paragraph, style).and_then(|s| s.export.as_ref()) {
        return match e.tag.as_str() {
            "h1" => "h1",
            "h2" => "h2",
            "h3" => "h3",
            "h4" => "h4",
            "li" => "li",
            "blockquote" => "blockquote",
            _ => "p",
        };
    }
    let l = style.to_ascii_lowercase();
    if l == "title" || l.starts_with("heading 1") || l == "headline" || l == "head" || l.starts_with("h1") {
        "h1"
    } else if l.starts_with("heading 2") || l.starts_with("subhead") || l.starts_with("deck") {
        "h2"
    } else if l.starts_with("heading 3") {
        "h3"
    } else {
        "p"
    }
}

fn run_html(doc: &Doc, para: &Para, seg: &Seg, text: &str, out: &mut String) {
    let cp = doc.styles.char_props(&para.style, &para.over, seg.style(), seg.over());
    let base = doc.styles.char_props(&para.style, &para.over, None, &Default::default());
    let bold = cp.weight() >= 600 && base.weight() < 600;
    let italic = cp.italic() && !base.italic();
    let e = esc(text).replace('\u{2028}', "<br/>");
    let mut s = e;
    if italic {
        s = format!("<em>{s}</em>");
    }
    if bold {
        s = format!("<strong>{s}</strong>");
    }
    if let Some(cs) = seg.style() {
        s = format!("<span class=\"{}\">{s}</span>", slug(cs));
    }
    out.push_str(&s);
}

fn para_html(doc: &Doc, p: &Para, out: &mut String, notes: &mut Vec<String>) {
    let tag = tag_for(doc, &p.style);
    let _ = write!(out, "<{tag} class=\"{}\">", slug(&p.style));
    for seg in &p.segs {
        match seg {
            Seg::Text { text, .. } => run_html(doc, p, seg, text, out),
            Seg::Inline { inline, .. } => match inline {
                Inline::Footnote { paras } => {
                    notes.push(paras.iter().map(|q| q.plain()).collect::<Vec<_>>().join(" "));
                    let n = notes.len();
                    let _ = write!(out, "<sup><a href=\"#fn{n}\" id=\"fr{n}\">{n}</a></sup>");
                }
                Inline::Variable { name } => out.push_str(&esc(&doc.variable_text(name, None))),
                Inline::Xref { text, .. } => out.push_str(&esc(text)),
                Inline::Anchor { name } => {
                    let _ = write!(out, "<a id=\"{}\"></a>", slug(name));
                }
                Inline::RightTab => out.push(' '),
                Inline::Table { table } => {
                    let attr = |k: &str| table.attrs.get(k).and_then(|v| v.as_str()).map(str::to_string).filter(|s| !s.is_empty());
                    match attr("alt") {
                        Some(a) => { let _ = write!(out, "</p><table aria-label=\"{}\">", esc(&a)); }
                        None => out.push_str("</p><table>"),
                    }
                    if let Some(s) = attr("summary") {
                        let _ = write!(out, "<caption>{}</caption>", esc(&s));
                    }
                    for r in 0..table.rows() {
                        out.push_str("<tr>");
                        for c in 0..table.cols() {
                            if let Some(cell) = table.cells.iter().find(|x| x.row == r && x.col == c) {
                                let td = if r < table.header_rows { "th" } else { "td" };
                                let _ = write!(out, "<{td} colspan=\"{}\" rowspan=\"{}\">{}</{td}>", cell.cols, cell.rows, esc(&cell.paras.iter().map(|q| q.plain()).collect::<Vec<_>>().join(" ")));
                            }
                        }
                        out.push_str("</tr>");
                    }
                    out.push_str("</table><p>");
                }
                _ => {}
            },
        }
    }
    let _ = write!(out, "</{tag}>");
}

fn css(doc: &Doc) -> String {
    let mut s = String::from("body{margin:2em auto;max-width:42em;padding:0 1em}img{max-width:100%;height:auto;display:block;margin:1em 0}table{border-collapse:collapse}td,th{border:1px solid #999;padding:.25em .5em}\n");
    let empty = Default::default();
    for st in doc.styles.list(StyleKind::Paragraph) {
        let pp = doc.styles.para_props(&st.name, &empty);
        let cp = doc.styles.char_props(&st.name, &empty, None, &empty);
        let align = match pp.align {
            formdes_model::attrs::Align::Center | formdes_model::attrs::Align::JustifyCenter => "center",
            formdes_model::attrs::Align::Right | formdes_model::attrs::Align::JustifyRight => "right",
            a if a.justified() => "justify",
            _ => "left",
        };
        let color = doc.paint_color(&cp.fill).map(|c| c.hex()).unwrap_or_else(|| "#000000".into());
        let _ = writeln!(
            s,
            ".{}{{font-family:\"{}\",serif;font-size:{:.2}pt;line-height:{:.2}pt;font-weight:{};font-style:{};text-align:{align};margin:{:.2}pt {:.2}pt {:.2}pt {:.2}pt;text-indent:{:.2}pt;color:{color}}}",
            slug(&st.name),
            cp.font,
            cp.size,
            cp.line_height(pp.auto_leading),
            cp.weight(),
            if cp.italic() { "italic" } else { "normal" },
            pp.space_before,
            pp.right_indent,
            pp.space_after,
            pp.left_indent,
            pp.first_indent
        );
    }
    s
}

fn ext_of(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => "jpg",
    }
}

fn body(doc: &Doc, image_src: &dyn Fn(AssetId) -> Option<String>) -> String {
    let mut out = String::new();
    let mut notes = Vec::new();
    for b in order(doc) {
        match b {
            Block::Story(paras) => {
                for p in &paras {
                    para_html(doc, p, &mut out, &mut notes);
                    out.push('\n');
                }
            }
            Block::Image(a, alt) => {
                if let Some(src) = image_src(a) {
                    let _ = writeln!(out, "<img src=\"{src}\" alt=\"{}\"/>", esc(&alt));
                }
            }
        }
    }
    if !notes.is_empty() {
        out.push_str("<section class=\"notes\"><ol>");
        for (i, n) in notes.iter().enumerate() {
            let _ = write!(out, "<li id=\"fn{}\">{} <a href=\"#fr{}\">&#8617;</a></li>", i + 1, esc(n), i + 1);
        }
        out.push_str("</ol></section>");
    }
    out
}

fn web_image(doc: &Doc, a: AssetId) -> bool {
    doc.assets.get(&a).is_some_and(|x| matches!(x.mime.as_str(), "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/svg+xml") && !x.data.is_empty())
}

pub fn html(doc: &Doc, o: &Opts) -> String {
    let src = |a: AssetId| {
        if !web_image(doc, a) {
            return None;
        }
        let x = doc.assets.get(&a)?;
        Some(format!("data:{};base64,{}", x.mime, b64(&x.data)))
    };
    format!(
        "<!DOCTYPE html>\n<html lang=\"{}\"><head><meta charset=\"utf-8\"/><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"/><title>{}</title><style>{}</style></head><body>\n{}</body></html>\n",
        esc(&o.language),
        esc(&o.title),
        css(doc),
        body(doc, &src)
    )
}

fn zerr(e: impl std::fmt::Display) -> String {
    format!("EPUB: {e}")
}

pub struct FixedPage {
    pub png: Vec<u8>,
    pub width: f64,
    pub height: f64,
    pub text: String,
}

pub fn epub(doc: &Doc, o: &Opts, cover: Option<Vec<u8>>, fixed: Option<Vec<FixedPage>>) -> Result<Vec<u8>, String> {
    let mut buf = Cursor::new(Vec::new());
    let mut z = zip::ZipWriter::new(&mut buf);
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    z.start_file("mimetype", stored).map_err(zerr)?;
    z.write_all(b"application/epub+zip").map_err(zerr)?;
    z.start_file("META-INF/container.xml", deflate).map_err(zerr)?;
    z.write_all(b"<?xml version=\"1.0\"?><container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>").map_err(zerr)?;
    let mut manifest = String::new();
    let mut spine = String::new();
    let mut nav = String::new();
    let xhtml = |title: &str, head: &str, body: &str| format!("<?xml version=\"1.0\" encoding=\"utf-8\"?><!DOCTYPE html><html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" lang=\"{}\"><head><meta charset=\"utf-8\"/><title>{}</title>{head}</head><body>{body}</body></html>", esc(&o.language), esc(title));
    let fixed_mode = fixed.is_some();
    if let Some(c) = &cover {
        z.start_file("OEBPS/images/cover.png", stored).map_err(zerr)?;
        z.write_all(c).map_err(zerr)?;
        manifest.push_str("<item id=\"cover-image\" href=\"images/cover.png\" media-type=\"image/png\" properties=\"cover-image\"/>");
        if !fixed_mode {
            z.start_file("OEBPS/cover.xhtml", deflate).map_err(zerr)?;
            z.write_all(xhtml(&o.title, "", "<img src=\"images/cover.png\" alt=\"Cover\" style=\"max-width:100%\"/>").as_bytes()).map_err(zerr)?;
            manifest.push_str("<item id=\"cover\" href=\"cover.xhtml\" media-type=\"application/xhtml+xml\"/>");
            spine.push_str("<itemref idref=\"cover\"/>");
        }
    }
    if let Some(pages) = fixed {
        for (i, p) in pages.iter().enumerate() {
            let n = i + 1;
            z.start_file(format!("OEBPS/images/page{n}.png"), stored).map_err(zerr)?;
            z.write_all(&p.png).map_err(zerr)?;
            let (w, h) = (p.width.round() as i64, p.height.round() as i64);
            let head = format!("<meta name=\"viewport\" content=\"width={w}, height={h}\"/><style>body{{margin:0;width:{w}px;height:{h}px}}img{{position:absolute;left:0;top:0;width:{w}px;height:{h}px}}.t{{position:absolute;left:0;top:0;width:{w}px;color:transparent}}</style>");
            let body = format!("<img src=\"images/page{n}.png\" alt=\"Page {n}\"/><div class=\"t\">{}</div>", esc(&p.text));
            z.start_file(format!("OEBPS/page{n}.xhtml"), deflate).map_err(zerr)?;
            z.write_all(xhtml(&format!("Page {n}"), &head, &body).as_bytes()).map_err(zerr)?;
            let _ = write!(manifest, "<item id=\"img{n}\" href=\"images/page{n}.png\" media-type=\"image/png\"/><item id=\"p{n}\" href=\"page{n}.xhtml\" media-type=\"application/xhtml+xml\"/>");
            let _ = write!(spine, "<itemref idref=\"p{n}\"/>");
            let _ = write!(nav, "<li><a href=\"page{n}.xhtml\">Page {n}</a></li>");
        }
    } else {
        let mut images = Vec::new();
        let src = |a: AssetId| {
            if !web_image(doc, a) {
                return None;
            }
            let x = doc.assets.get(&a)?;
            Some(format!("images/a{}.{}", a.0, ext_of(&x.mime)))
        };
        let html_body = body(doc, &src);
        for b in order(doc) {
            if let Block::Image(a, _) = b
                && web_image(doc, a)
                && !images.contains(&a)
            {
                images.push(a);
            }
        }
        for a in images {
            let x = &doc.assets[&a];
            let name = format!("a{}.{}", a.0, ext_of(&x.mime));
            z.start_file(format!("OEBPS/images/{name}"), stored).map_err(zerr)?;
            z.write_all(&x.data).map_err(zerr)?;
            let _ = write!(manifest, "<item id=\"i{}\" href=\"images/{name}\" media-type=\"{}\"/>", a.0, x.mime);
        }
        z.start_file("OEBPS/style.css", deflate).map_err(zerr)?;
        z.write_all(css(doc).as_bytes()).map_err(zerr)?;
        z.start_file("OEBPS/text.xhtml", deflate).map_err(zerr)?;
        z.write_all(xhtml(&o.title, "<link rel=\"stylesheet\" href=\"style.css\"/>", &html_body).as_bytes()).map_err(zerr)?;
        manifest.push_str("<item id=\"css\" href=\"style.css\" media-type=\"text/css\"/><item id=\"text\" href=\"text.xhtml\" media-type=\"application/xhtml+xml\"/>");
        spine.push_str("<itemref idref=\"text\"/>");
        let _ = write!(nav, "<li><a href=\"text.xhtml\">{}</a></li>", esc(&o.title));
    }
    z.start_file("OEBPS/nav.xhtml", deflate).map_err(zerr)?;
    z.write_all(xhtml("Contents", "", &format!("<nav epub:type=\"toc\"><ol>{nav}</ol></nav>")).as_bytes()).map_err(zerr)?;
    manifest.push_str("<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>");
    let fixed_meta = if fixed_mode { "<meta property=\"rendition:layout\">pre-paginated</meta><meta property=\"rendition:spread\">auto</meta>" } else { "" };
    let uid = format!("urn:formdes:{:x}", doc.title.bytes().fold(1469598103934665603u64, |h, b| (h ^ b as u64).wrapping_mul(1099511628211)));
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:identifier id=\"uid\">{uid}</dc:identifier><dc:title>{}</dc:title><dc:creator>{}</dc:creator><dc:language>{}</dc:language><meta property=\"dcterms:modified\">2026-01-01T00:00:00Z</meta>{fixed_meta}</metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>",
        esc(&o.title),
        esc(&o.author),
        esc(&o.language)
    );
    z.start_file("OEBPS/content.opf", deflate).map_err(zerr)?;
    z.write_all(opf.as_bytes()).map_err(zerr)?;
    z.finish().map_err(zerr)?;
    Ok(buf.into_inner())
}
