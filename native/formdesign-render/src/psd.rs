use std::ops::Range;

use kurbo::BezPath;

pub struct Resource {
    pub id: u16,
    pub name: String,
    pub range: Range<usize>,
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub index: usize,
    pub name: String,
    pub top: i32,
    pub left: i32,
    pub bottom: i32,
    pub right: i32,
    pub opacity: u8,
    pub visible: bool,
    pub kind: u8,
    pub blend: [u8; 4],
    chans: Vec<(i16, Range<usize>)>,
}

pub struct Psd<'a> {
    data: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub depth: u16,
    pub mode: u16,
    pub channels: u16,
    pub resources: Vec<Resource>,
    pub layers: Vec<Layer>,
    merged: usize,
}

struct R<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.d.get(self.p..self.p.checked_add(n)?)?;
        self.p += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_be_bytes(self.take(2)?.try_into().ok()?))
    }
    fn i16(&mut self) -> Option<i16> {
        Some(i16::from_be_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }
}

pub fn is_psd(b: &[u8]) -> bool {
    b.starts_with(b"8BPS")
}

pub fn header_size(b: &[u8]) -> Option<(u32, u32)> {
    if !is_psd(b) || b.len() < 26 {
        return None;
    }
    let h = u32::from_be_bytes(b[14..18].try_into().ok()?);
    let w = u32::from_be_bytes(b[18..22].try_into().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

pub fn parse_resources(d: &[u8], base: usize) -> Vec<Resource> {
    let mut out = Vec::new();
    let mut r = R { d, p: 0 };
    while r.p + 12 <= d.len() {
        if r.take(4) != Some(b"8BIM") {
            break;
        }
        let Some(id) = r.u16() else { break };
        let Some(nl) = r.u8() else { break };
        let Some(name) = r.take(nl as usize) else { break };
        if (1 + nl as usize) % 2 == 1 {
            r.p += 1;
        }
        let Some(size) = r.u32() else { break };
        let start = r.p;
        let end = start + size as usize;
        if end > d.len() {
            break;
        }
        out.push(Resource { id, name: String::from_utf8_lossy(name).into_owned(), range: base + start..base + end });
        r.p = end + (size as usize % 2);
    }
    out
}

impl<'a> Psd<'a> {
    pub fn parse(data: &'a [u8]) -> Option<Psd<'a>> {
        let mut r = R { d: data, p: 0 };
        if r.take(4)? != b"8BPS" || r.u16()? != 1 {
            return None;
        }
        r.take(6)?;
        let channels = r.u16()?;
        let height = r.u32()?;
        let width = r.u32()?;
        let depth = r.u16()?;
        let mode = r.u16()?;
        if width == 0 || height == 0 || width > 30000 || height > 30000 {
            return None;
        }
        let n = r.u32()? as usize;
        r.take(n)?;
        let n = r.u32()? as usize;
        let res_at = r.p;
        let res = r.take(n)?;
        let resources = parse_resources(res, res_at);
        let n = r.u32()? as usize;
        let sect_end = (r.p + n).min(data.len());
        let mut layers = Vec::new();
        if n > 0 {
            let info = r.u32()? as usize;
            if info > 0 {
                let info_end = (r.p + info).min(data.len());
                let count = r.i16()?.unsigned_abs() as usize;
                let mut recs: Vec<(Layer, Vec<(i16, usize)>)> = Vec::new();
                for index in 0..count {
                    let (top, left, bottom, right) = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
                    let nch = r.u16()? as usize;
                    let mut ch = Vec::new();
                    for _ in 0..nch {
                        ch.push((r.i16()?, r.u32()? as usize));
                    }
                    if r.take(4)? != b"8BIM" {
                        return None;
                    }
                    let blend: [u8; 4] = r.take(4)?.try_into().ok()?;
                    let opacity = r.u8()?;
                    let _clip = r.u8()?;
                    let flags = r.u8()?;
                    let _ = r.u8()?;
                    let extra = r.u32()? as usize;
                    let extra_end = r.p + extra;
                    let mut er = R { d: data.get(..extra_end)?, p: r.p };
                    let ml = er.u32()? as usize;
                    er.take(ml)?;
                    let bl = er.u32()? as usize;
                    er.take(bl)?;
                    let nl = er.u8()? as usize;
                    let name_bytes = er.take(nl)?;
                    let mut name: String = name_bytes.iter().map(|b| *b as char).collect();
                    let pad = (4 - (1 + nl) % 4) % 4;
                    er.take(pad)?;
                    let mut kind = 0u8;
                    while er.p + 12 <= extra_end {
                        let sig = er.take(4)?;
                        if sig != b"8BIM" && sig != b"8B64" {
                            break;
                        }
                        let key: [u8; 4] = er.take(4)?.try_into().ok()?;
                        let len = er.u32()? as usize;
                        let start = er.p;
                        match &key {
                            b"lsct" | b"lsdk" => {
                                if let Some(b) = data.get(start..start + 4) {
                                    let t = u32::from_be_bytes(b.try_into().ok()?);
                                    kind = match t {
                                        1 => 1,
                                        2 => 2,
                                        3 => 3,
                                        _ => 0,
                                    };
                                }
                            }
                            b"luni" => {
                                let mut ur = R { d: data, p: start };
                                if let Some(cnt) = ur.u32()
                                    && let Some(raw) = ur.take(cnt as usize * 2)
                                {
                                    let u: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                                    name = String::from_utf16_lossy(&u);
                                }
                            }
                            _ => {}
                        }
                        er.p = start + len + (len % 2);
                    }
                    r.p = extra_end;
                    recs.push((Layer { index, name, top, left, bottom, right, opacity, visible: flags & 2 == 0, kind, blend, chans: Vec::new() }, ch));
                }
                let mut pos = r.p;
                for (mut l, ch) in recs {
                    for (id, len) in ch {
                        let end = pos + len;
                        if end > data.len() {
                            return None;
                        }
                        l.chans.push((id, pos..end));
                        pos = end;
                    }
                    layers.push(l);
                }
                r.p = info_end;
            }
        }
        r.p = sect_end;
        Some(Psd { data, width, height, depth, mode, channels, resources, layers, merged: r.p })
    }

    pub fn resource(&self, id: u16) -> Option<&'a [u8]> {
        self.resources.iter().find(|r| r.id == id).map(|r| &self.data[r.range.clone()])
    }

    pub fn paths(&self) -> Vec<NamedPath> {
        paths_from(self.data, &self.resources)
    }

    pub fn density(&self) -> Option<(f64, f64)> {
        resolution(self.resource(1005)?)
    }

    pub fn color_channels(&self) -> usize {
        match self.mode {
            1 | 2 | 8 => 1,
            4 => 4,
            _ => 3,
        }
    }

    pub fn extra_channel_names(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(b) = self.resource(1006) {
            let mut r = R { d: b, p: 0 };
            while let Some(n) = r.u8() {
                let Some(s) = r.take(n as usize) else { break };
                out.push(String::from_utf8_lossy(s).into_owned());
            }
        }
        out
    }

    fn decode(&self, comp: u16, body: &[u8], w: usize, h: usize, chans: usize) -> Option<Vec<Vec<u8>>> {
        if !matches!(self.depth, 8 | 16) || chans == 0 || chans > 56 {
            return None;
        }
        let bps = if self.depth == 16 { 2 } else { 1 };
        let row = w.checked_mul(bps)?;
        let need = row.checked_mul(h)?.checked_mul(chans)?;
        match comp {
            0 => {
                let body = body.get(..need)?;
                Some(body.chunks_exact(row * h).map(|c| to8(c, self.depth)).collect())
            }
            1 => {
                if need / 64 > body.len() {
                    return None;
                }
                let mut r = R { d: body, p: 0 };
                let mut counts = Vec::new();
                for _ in 0..h * chans {
                    counts.push(r.u16()? as usize);
                }
                let mut planes = Vec::new();
                let mut i = 0;
                for _ in 0..chans {
                    let mut plane = Vec::with_capacity(row * h);
                    for _ in 0..h {
                        let line = r.take(counts[i])?;
                        i += 1;
                        unpack_bits(line, row, &mut plane);
                    }
                    planes.push(to8(&plane, self.depth));
                }
                Some(planes)
            }
            _ => None,
        }
    }

    fn layer_plane(&self, l: &Layer, id: i16) -> Option<Vec<u8>> {
        let (_, range) = l.chans.iter().find(|(c, _)| *c == id)?;
        let b = &self.data[range.clone()];
        let comp = u16::from_be_bytes([*b.first()?, *b.get(1)?]);
        let (w, h) = ((l.right - l.left).max(0) as usize, (l.bottom - l.top).max(0) as usize);
        if w == 0 || h == 0 {
            return None;
        }
        self.decode(comp, &b[2..], w, h, 1)?.into_iter().next()
    }

    pub fn merged_planes(&self) -> Option<Vec<Vec<u8>>> {
        let b = self.data.get(self.merged..)?;
        let comp = u16::from_be_bytes([*b.first()?, *b.get(1)?]);
        self.decode(comp, &b[2..], self.width as usize, self.height as usize, self.channels as usize)
    }

    pub fn merged_rgba(&self) -> Option<Vec<u8>> {
        let planes = self.merged_planes()?;
        let n = (self.width * self.height) as usize;
        let mut out = vec![255u8; n * 4];
        let cc = self.color_channels();
        if planes.len() < cc {
            return None;
        }
        for i in 0..n {
            let (r, g, b) = match self.mode {
                1 | 2 | 8 => (planes[0][i], planes[0][i], planes[0][i]),
                4 => {
                    let k = planes[3][i] as u32;
                    (((planes[0][i] as u32 * k) / 255) as u8, ((planes[1][i] as u32 * k) / 255) as u8, ((planes[2][i] as u32 * k) / 255) as u8)
                }
                _ => (planes[0][i], planes[1][i], planes[2][i]),
            };
            out[i * 4] = r;
            out[i * 4 + 1] = g;
            out[i * 4 + 2] = b;
        }
        Some(out)
    }

    pub fn extra_channel(&self, index: usize) -> Option<Vec<u8>> {
        let planes = self.merged_planes()?;
        planes.into_iter().nth(self.color_channels() + index)
    }

    pub fn effective(&self, show: &dyn Fn(&Layer) -> bool) -> Vec<bool> {
        let mut vis = vec![false; self.layers.len()];
        let mut stack: Vec<bool> = Vec::new();
        for (i, l) in self.layers.iter().enumerate().rev() {
            let parent = stack.last().copied().unwrap_or(true);
            match l.kind {
                1 | 2 => {
                    let v = show(l) && parent;
                    vis[i] = false;
                    stack.push(v);
                }
                3 => {
                    stack.pop();
                }
                _ => vis[i] = show(l) && parent,
            }
        }
        vis
    }

    pub fn composite(&self, show: &dyn Fn(&Layer) -> bool) -> Option<Vec<u8>> {
        if self.layers.is_empty() {
            return self.merged_rgba();
        }
        let vis = self.effective(show);
        let file: Vec<bool> = self.effective(&|l: &Layer| l.visible);
        if vis == file
            && let Some(m) = self.merged_rgba()
        {
            return Some(m);
        }
        if !matches!(self.mode, 1 | 3) || self.depth == 32 {
            return self.merged_rgba();
        }
        let (w, h) = (self.width as i64, self.height as i64);
        let mut acc = vec![0f32; (w * h * 4) as usize];
        for (i, l) in self.layers.iter().enumerate() {
            if !vis[i] || l.kind != 0 {
                continue;
            }
            let (lw, lh) = ((l.right - l.left) as i64, (l.bottom - l.top) as i64);
            if lw <= 0 || lh <= 0 {
                continue;
            }
            let rgb: Vec<Option<Vec<u8>>> = if self.mode == 1 { vec![self.layer_plane(l, 0), None, None] } else { (0..3).map(|c| self.layer_plane(l, c)).collect() };
            let Some(r0) = rgb[0].as_ref() else { continue };
            let alpha = self.layer_plane(l, -1);
            let op = l.opacity as f32 / 255.0;
            for y in 0..lh {
                let cy = y + l.top as i64;
                if cy < 0 || cy >= h {
                    continue;
                }
                for x in 0..lw {
                    let cx = x + l.left as i64;
                    if cx < 0 || cx >= w {
                        continue;
                    }
                    let li = (y * lw + x) as usize;
                    let a = alpha.as_ref().map(|a| a[li] as f32 / 255.0).unwrap_or(1.0) * op;
                    if a <= 0.0 {
                        continue;
                    }
                    let px = [r0[li], rgb[1].as_ref().map(|p| p[li]).unwrap_or(r0[li]), rgb[2].as_ref().map(|p| p[li]).unwrap_or(r0[li])];
                    let ci = ((cy * w + cx) * 4) as usize;
                    let da = acc[ci + 3];
                    let out_a = a + da * (1.0 - a);
                    for k in 0..3 {
                        acc[ci + k] = (px[k] as f32 * a + acc[ci + k] * da * (1.0 - a)) / out_a.max(1e-6);
                    }
                    acc[ci + 3] = out_a;
                }
            }
        }
        Some(acc.chunks_exact(4).flat_map(|p| [p[0].round().clamp(0.0, 255.0) as u8, p[1].round().clamp(0.0, 255.0) as u8, p[2].round().clamp(0.0, 255.0) as u8, (p[3] * 255.0).round().clamp(0.0, 255.0) as u8]).collect())
    }
}

fn to8(b: &[u8], depth: u16) -> Vec<u8> {
    if depth == 16 { b.chunks_exact(2).map(|c| c[0]).collect() } else { b.to_vec() }
}

fn unpack_bits(line: &[u8], want: usize, out: &mut Vec<u8>) {
    let start = out.len();
    let mut i = 0;
    while i < line.len() && out.len() - start < want {
        let n = line[i] as i8;
        i += 1;
        if n >= 0 {
            let c = n as usize + 1;
            if let Some(s) = line.get(i..i + c) {
                out.extend_from_slice(s);
            }
            i += c;
        } else if n != -128 {
            let c = 1 - n as isize;
            if let Some(v) = line.get(i) {
                out.extend(std::iter::repeat_n(*v, c as usize));
            }
            i += 1;
        }
    }
    out.resize(start + want, 0);
}

fn resolution(b: &[u8]) -> Option<(f64, f64)> {
    let mut r = R { d: b, p: 0 };
    let h = r.u32()? as f64 / 65536.0;
    let hu = r.u16()?;
    r.u16()?;
    let v = r.u32()? as f64 / 65536.0;
    let vu = r.u16()?;
    let k = |x: f64, u: u16| if u == 2 { x * 2.54 } else { x };
    (h > 0.0 && v > 0.0).then(|| (k(h, hu), k(v, vu)))
}

pub struct NamedPath {
    pub name: String,
    pub id: u16,
    pub path: BezPath,
    pub clipping: bool,
}

fn fixed(v: i32) -> f64 {
    v as f64 / 16_777_216.0
}

fn path_of(rec: &[u8]) -> Option<BezPath> {
    struct Knot {
        pre: (f64, f64),
        a: (f64, f64),
        post: (f64, f64),
    }
    let mut subs: Vec<(bool, Vec<Knot>, usize)> = Vec::new();
    for c in rec.chunks_exact(26) {
        let sel = u16::from_be_bytes([c[0], c[1]]);
        let f = |o: usize| -> (f64, f64) {
            let y = i32::from_be_bytes([c[o], c[o + 1], c[o + 2], c[o + 3]]);
            let x = i32::from_be_bytes([c[o + 4], c[o + 5], c[o + 6], c[o + 7]]);
            (fixed(x), fixed(y))
        };
        match sel {
            0 | 3 => subs.push((sel == 0, Vec::new(), u16::from_be_bytes([c[2], c[3]]) as usize)),
            1 | 2 | 4 | 5 => {
                if let Some((_, knots, n)) = subs.last_mut()
                    && knots.len() < *n
                {
                    knots.push(Knot { pre: f(2), a: f(10), post: f(18) });
                }
            }
            _ => {}
        }
    }
    let mut p = BezPath::new();
    for (_, k, _) in subs {
        if k.len() < 2 {
            continue;
        }
        p.move_to(k[0].a);
        for i in 1..k.len() {
            p.curve_to(k[i - 1].post, k[i].pre, k[i].a);
        }
        let last = k.len() - 1;
        p.curve_to(k[last].post, k[0].pre, k[0].a);
        p.close_path();
    }
    (!p.elements().is_empty()).then_some(p)
}

fn paths_from(data: &[u8], resources: &[Resource]) -> Vec<NamedPath> {
    let clip_name = resources.iter().find(|r| r.id == 2999).and_then(|r| {
        let b = &data[r.range.clone()];
        let n = *b.first()? as usize;
        Some(String::from_utf8_lossy(b.get(1..1 + n)?).into_owned())
    });
    resources
        .iter()
        .filter(|r| (2000..=2997).contains(&r.id))
        .filter_map(|r| {
            let path = path_of(&data[r.range.clone()])?;
            let clipping = clip_name.as_deref().is_some_and(|n| n == r.name);
            Some(NamedPath { name: r.name.clone(), id: r.id, path, clipping })
        })
        .collect()
}

fn jpeg_8bim(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 2;
    while i + 4 <= b.len() && b[i] == 0xFF {
        let m = b[i + 1];
        if m == 0xD8 || (0xD0..=0xD7).contains(&m) || m == 0x01 {
            i += 2;
            continue;
        }
        if m == 0xDA || m == 0xD9 {
            break;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if m == 0xED
            && let Some(seg) = b.get(i + 4..i + 2 + len)
            && seg.starts_with(b"Photoshop 3.0\0")
        {
            out.extend_from_slice(&seg[14..]);
        }
        i += 2 + len;
    }
    out
}

fn tiff_8bim(b: &[u8]) -> Vec<u8> {
    let le = match b.get(..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return Vec::new(),
    };
    let u16_ = |o: usize| b.get(o..o + 2).map(|s| if le { u16::from_le_bytes([s[0], s[1]]) } else { u16::from_be_bytes([s[0], s[1]]) });
    let u32_ = |o: usize| b.get(o..o + 4).map(|s| if le { u32::from_le_bytes([s[0], s[1], s[2], s[3]]) } else { u32::from_be_bytes([s[0], s[1], s[2], s[3]]) });
    let Some(ifd) = u32_(4) else { return Vec::new() };
    let ifd = ifd as usize;
    let Some(n) = u16_(ifd) else { return Vec::new() };
    for e in 0..n as usize {
        let at = ifd + 2 + e * 12;
        if u16_(at) == Some(34377) {
            let (Some(count), Some(off)) = (u32_(at + 4), u32_(at + 8)) else { break };
            return b.get(off as usize..off as usize + count as usize).map(<[u8]>::to_vec).unwrap_or_default();
        }
    }
    Vec::new()
}

pub fn psd_clip_paths(bytes: &[u8]) -> Vec<NamedPath> {
    if is_psd(bytes) {
        return Psd::parse(bytes).map(|p| p.paths()).unwrap_or_default();
    }
    let block = if bytes.starts_with(&[0xFF, 0xD8]) {
        jpeg_8bim(bytes)
    } else if bytes.starts_with(b"II") || bytes.starts_with(b"MM") {
        tiff_8bim(bytes)
    } else {
        return Vec::new();
    };
    let res = parse_resources(&block, 0);
    paths_from(&block, &res)
}

pub fn density(bytes: &[u8]) -> Option<(f64, f64)> {
    Psd::parse(bytes)?.density()
}

pub fn decode(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let p = Psd::parse(bytes)?;
    if !matches!(p.mode, 1 | 3 | 4 | 8) {
        return None;
    }
    let rgba = p.composite(&|l: &Layer| l.visible)?;
    Some((p.width, p.height, rgba))
}

pub fn composite_png(bytes: &[u8], show: &dyn Fn(&Layer) -> bool) -> Option<Vec<u8>> {
    let p = Psd::parse(bytes)?;
    let rgba = p.composite(show)?;
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_raw(p.width, p.height, rgba)?.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}
