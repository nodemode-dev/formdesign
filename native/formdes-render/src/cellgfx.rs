use formdes_model::doc::Doc;
use formdes_model::geom::{Affine, KShape, Rect};
use formdes_model::ids::AssetId;
use formdes_model::story::TableCell;

use crate::scene::{Op, Tagging};

pub fn cell_graphic(cell: &TableCell) -> Option<(Option<AssetId>, String)> {
    let g = cell.attrs.get("graphic")?;
    let asset = g.get("asset").and_then(|v| v.as_u64()).filter(|a| *a > 0).map(AssetId);
    let fitting = g.get("fitting").and_then(|v| v.as_str()).unwrap_or("fit").to_string();
    Some((asset, fitting))
}

pub fn cell_image_ops(doc: &Doc, cell: &TableCell, rect: Rect, xf: Affine) -> Vec<Op> {
    let Some((Some(aid), fitting)) = cell_graphic(cell) else { return Vec::new() };
    let Some(asset) = doc.assets.get(&aid) else { return Vec::new() };
    let ins = formdes_text::flow::cell_insets(cell);
    let area = Rect::new(rect.x0 + ins[1], rect.y0 + ins[0], (rect.x1 - ins[3]).max(rect.x0 + ins[1] + 1.0), (rect.y1 - ins[2]).max(rect.y0 + ins[0] + 1.0));
    let (w, h) = asset.natural_size();
    if w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }
    let scale = match fitting.as_str() {
        "fill" => (area.width() / w).max(area.height() / h),
        "center" => 1.0,
        _ => (area.width() / w).min(area.height() / h),
    };
    let (cx, cy) = ((area.x0 + area.x1) / 2.0, (area.y0 + area.y1) / 2.0);
    let local = Affine::translate((cx - w * scale / 2.0, cy - h * scale / 2.0)) * Affine::scale(scale);
    let alt = cell.attrs.get("graphic").and_then(|g| g.get("alt")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    vec![Op::Tag(Tagging::Figure(alt)), Op::PushClip(xf * area.to_path(0.1)), Op::Image { asset: aid, xf: xf * local, w, h }, Op::Pop, Op::Tag(Tagging::End)]
}
