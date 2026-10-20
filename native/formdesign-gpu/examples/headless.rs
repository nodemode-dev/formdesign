//! Composites a synthetic spread on a headless device and reads it back: two tiles of one page uploaded, the page's
//! second row left missing (it must show as white paper), a 640 by 480 frame with the pasteboard, shadow and view
//! transform. Writes /tmp/formdesign-gpu-probe.png and prints a checksum and a few pixels.

use formdesign_gpu::composite::{Compositor, Frame, PageRect};
use formdesign_gpu::{read_rgba, Gpu, TileAtlas, TileKey, TILE};

fn tile_pixels(base: [u8; 3]) -> Vec<u8> {
    let mut px = Vec::with_capacity((TILE * TILE * 4) as usize);
    for y in 0..TILE {
        for x in 0..TILE {
            // A gradient with a 32-pixel grid so scaling is visible.
            let grid = x % 32 == 0 || y % 32 == 0;
            let g = if grid { 0 } else { ((x + y) / 4) as u8 };
            px.extend_from_slice(&[base[0].saturating_sub(g / 2), base[1].saturating_sub(g / 2), base[2].saturating_sub(g / 2), 255]);
        }
    }
    px
}

fn main() {
    let Some(gpu) = Gpu::new() else {
        println!("no GPU adapter");
        return;
    };
    println!("adapter: {} ({:?}, {:?})", gpu.info.name, gpu.info.device_type, gpu.info.backend);
    let mut atlas = TileAtlas::with_system_budget();
    let compositor = Compositor::new(&gpu, Compositor::TEXTURE_FORMAT);
    let left = TileKey { page: 0, level: 0, tx: 0, ty: 0 };
    let right = TileKey { page: 0, level: 0, tx: 1, ty: 0 };
    atlas.upload(&gpu, left, TILE, TILE, &tile_pixels([230, 80, 60]));
    atlas.upload(&gpu, right, TILE, TILE, &tile_pixels([60, 110, 230]));
    println!("atlas: {} tiles, {} bytes, budget {}", atlas.len(), atlas.bytes(), atlas.budget());

    // One page of 512 by 400 units at view scale 1: tiles (0,0) and (1,0) are on the GPU, the row below is not.
    let frame = Frame {
        width: 640,
        height: 480,
        dpr: 1.0,
        pasteboard: [0.2, 0.2, 0.22, 1.0],
        view: [1.0, 64.0, 40.0],
        pages: vec![PageRect { index: 0, x: 0.0, y: 0.0, w: 512.0, h: 400.0 }],
    };
    let (tex, missing) = compositor.render(&gpu, &mut atlas, &frame);
    println!("missing tiles: {missing:?}");
    let px = read_rgba(&gpu, &tex, frame.width, frame.height);
    let sum = px.iter().fold(0u64, |a, &b| a.wrapping_mul(131).wrapping_add(b as u64));
    println!("checksum: {sum:016x}");
    let at = |x: usize, y: usize| {
        let i = (y * frame.width as usize + x) * 4;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    };
    println!("pasteboard (5,5): {:?}", at(5, 5));
    println!("left tile (100,100): {:?}", at(100, 100));
    println!("right tile (400,100): {:?}", at(400, 100));
    println!("missing row, paper (100,350): {:?}", at(100, 350));
    println!("shadow below page (200,445): {:?}", at(200, 445));

    let file = std::fs::File::create("/tmp/formdesign-gpu-probe.png").expect("create the probe image");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), frame.width, frame.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut w| w.write_image_data(&px)).expect("write the probe image");
    println!("wrote /tmp/formdesign-gpu-probe.png");
}
