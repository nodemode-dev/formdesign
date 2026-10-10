pub mod images;
pub mod outline;
pub mod pdf;
pub mod raster;
pub mod scene;
pub mod view;

use formdes_model::doc::Doc;
use formdes_text::fonts::FontBook;
use formdes_text::layout::{DocLayout, layout_doc};

pub use images::Images;
pub use outline::Outlines;
pub use raster::{Raster, RasterOpts, encode_jpeg, encode_png, rasterize};
pub use scene::{Op, PageScene, SceneOpts, page_scene};

#[derive(Default)]
pub struct Renderer {
    pub fonts: FontBook,
    pub outlines: Outlines,
    pub images: Images,
}

impl Renderer {
    pub fn layout(&self, doc: &Doc) -> DocLayout {
        layout_doc(doc, &self.fonts)
    }

    pub fn page_png(&self, doc: &Doc, lay: &DocLayout, page: usize, scale: f64) -> Option<Vec<u8>> {
        let scene = page_scene(doc, lay, page, &SceneOpts::default())?;
        let r = rasterize(&scene, doc, &self.outlines, &self.images, &RasterOpts { scale, ..RasterOpts::default() });
        encode_png(&r)
    }

    pub fn pdf(&self, doc: &Doc, lay: &DocLayout, pages: Option<Vec<usize>>, bleed: bool) -> Result<Vec<u8>, String> {
        self.pdf_with(doc, lay, pages, bleed, &pdf::PdfOpts { title: doc.title.clone(), ..Default::default() })
    }

    pub fn pdf_with(&self, doc: &Doc, lay: &DocLayout, pages: Option<Vec<usize>>, bleed: bool, popts: &pdf::PdfOpts) -> Result<Vec<u8>, String> {
        let list = pages.unwrap_or_else(|| (0..doc.page_count()).collect());
        let opts = SceneOpts { printing: true, bleed, overprint: false };
        let scenes: Vec<PageScene> = list.into_iter().filter_map(|p| page_scene(doc, lay, p, &opts)).collect();
        pdf::export(&scenes, doc, &self.images, popts)
    }
}
