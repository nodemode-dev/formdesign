use std::sync::Arc;

use formdes_model::geom::Affine;
use formdes_render::scene::{Target, spread_ops};
use formdes_render::view::{Placed, ViewOpts, render_view};
use formdes_render::{SceneOpts, encode_png};
use serde_json::{Value, json};

use crate::core::Core;
use crate::proto::Reply;

fn unbase64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        acc = (acc << 6) | val(c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

pub fn snippet(core: &mut Core, p: &Value) -> Result<Reply, String> {
    let b64 = p.get("base64").and_then(Value::as_str).ok_or("missing `base64`")?;
    let bytes = unbase64(b64).ok_or("the snippet is not valid base64")?;
    let doc = formdes_model::format::load(&bytes).map_err(|e| format!("not a snippet: {}", e.0))?;
    let si = doc.spreads.iter().position(|s| !s.items.is_empty()).ok_or("the snippet is empty")?;
    let b = doc.spreads[si].items.iter().filter_map(|i| doc.bounds(*i)).reduce(|a, c| a.union(c)).ok_or("the snippet is empty")?;
    let size = p.get("size").and_then(Value::as_f64).unwrap_or(160.0).clamp(16.0, 1024.0);
    let pad = (size * 0.04).round();
    let (bw, bh) = (b.width().max(1.0), b.height().max(1.0));
    let s = (size - 2.0 * pad) / bw.max(bh);
    let (w, h) = ((bw * s + 2.0 * pad).ceil() as u32, (bh * s + 2.0 * pad).ceil() as u32);
    let view = Affine::translate((pad, pad)) * Affine::scale(s) * Affine::translate((-b.x0, -b.y0));
    let r = &core.ses.renderer;
    let lay = r.layout(&doc);
    let ops = Arc::new(spread_ops(&doc, &lay, Target::Spread(si), &SceneOpts { printing: true, bleed: false }));
    let placed = [Placed { xf: Affine::IDENTITY, pages: vec![b], ops }];
    let opts = ViewOpts { background: [0, 0, 0, 0], paper: false, shadow: false, printing_only: true, greek_px: 0.0, plate: None };
    let img = render_view(&doc, &placed, w, h, view, &opts, &r.outlines, &r.images);
    let png = encode_png(&img).ok_or("could not encode the thumbnail")?;
    Ok(Reply { result: json!({"width": img.width, "height": img.height, "mime": "image/png"}), bin: Some(png) })
}
