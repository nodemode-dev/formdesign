use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use kurbo::BezPath;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::FontRef;
use skrifa::{GlyphId, MetadataProvider};

use formdes_text::fonts::{Face, FaceId};

struct Pen(BezPath);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, y as f64));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to((cx0 as f64, cy0 as f64), (x as f64, y as f64));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to((cx0 as f64, cy0 as f64), (cx1 as f64, cy1 as f64), (x as f64, y as f64));
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}

#[derive(Default)]
pub struct Outlines {
    cache: RwLock<HashMap<(FaceId, u32), Arc<BezPath>>>,
}

impl Outlines {
    pub fn get(&self, face: &Face, gid: u32) -> Arc<BezPath> {
        if let Some(p) = self.cache.read().ok().and_then(|m| m.get(&(face.id, gid)).cloned()) {
            return p;
        }
        let path = Arc::new(outline(&face.font(), gid));
        if let Ok(mut m) = self.cache.write() {
            m.insert((face.id, gid), path.clone());
        }
        path
    }
}

fn outline(font: &FontRef, gid: u32) -> BezPath {
    let mut pen = Pen(BezPath::new());
    if let Some(g) = font.outline_glyphs().get(GlyphId::new(gid)) {
        let _ = g.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen);
    }
    pen.0
}
