use std::path::PathBuf;

use formdesign_engine::{Session, assets};
use serde_json::json;

struct Header {
    version: u16,
    channels: u16,
    height: u32,
    width: u32,
    depth: u16,
    mode: u16,
}

const RGB: Header = Header { version: 1, channels: 3, height: 3, width: 4, depth: 8, mode: 3 };

fn psd(h: &Header, comp: u16, body: &[u8]) -> Vec<u8> {
    let mut b = b"8BPS".to_vec();
    b.extend(h.version.to_be_bytes());
    b.extend([0u8; 6]);
    b.extend(h.channels.to_be_bytes());
    b.extend(h.height.to_be_bytes());
    b.extend(h.width.to_be_bytes());
    b.extend(h.depth.to_be_bytes());
    b.extend(h.mode.to_be_bytes());
    b.extend([0u8; 12]);
    b.extend(comp.to_be_bytes());
    b.extend(body);
    b
}

fn raw(h: &Header) -> Vec<u8> {
    let n = h.width as usize * h.height as usize * h.channels as usize * (h.depth as usize / 8);
    psd(h, 0, &vec![128u8; n])
}

fn rle(h: &Header, keep_rows: usize) -> Vec<u8> {
    let rows = h.height as usize * h.channels as usize;
    let line = [(1 - h.width as i16 * (h.depth as i16 / 8)) as u8, 200u8];
    let mut body = Vec::new();
    for _ in 0..rows {
        body.extend(2u16.to_be_bytes());
    }
    for _ in 0..keep_rows.min(rows) {
        body.extend(line);
    }
    psd(h, 1, &body)
}

fn refusal(name: &str, data: Vec<u8>) -> String {
    match assets::make(name, data, None, 0) {
        Ok(a) => panic!("{name} was placed as {:?} pixels", a.pixels),
        Err(e) => e.0,
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("formdesign-psd-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn a_whole_psd_places_at_its_header_size() {
    let a = assets::make("ok.psd", raw(&RGB), None, 0).expect("a raw RGB PSD places");
    assert_eq!(a.pixels, Some((4, 3)));
    let a = assets::make("ok-rle.psd", rle(&RGB, usize::MAX), None, 0).expect("an RLE RGB PSD places");
    assert_eq!(a.pixels, Some((4, 3)));
    let grey16 = Header { channels: 1, depth: 16, mode: 1, ..RGB };
    assert_eq!(assets::make("grey16.psd", raw(&grey16), None, 0).expect("a 16-bit greyscale PSD places").pixels, Some((4, 3)));
    let cmyk = Header { channels: 5, mode: 4, ..RGB };
    assert!(assets::make("cmyk-alpha.psd", raw(&cmyk), None, 0).is_ok());
}

#[test]
fn a_truncated_psd_at_the_size_limit_is_refused() {
    let huge = Header { width: 30_000, height: 30_000, ..RGB };
    let why = refusal("huge.psd", psd(&huge, 0, &[0u8; 64]));
    assert!(why.contains("cut short"), "{why}");
    let why = refusal("huge-rle.psd", rle(&huge, 10));
    assert!(why.contains("cut short"), "{why}");
}

#[test]
fn a_psd_missing_rows_or_header_bytes_is_refused() {
    let mut cut = raw(&RGB);
    cut.pop();
    assert!(refusal("cut.psd", cut).contains("cut short"));
    let rows = RGB.height as usize * RGB.channels as usize;
    assert!(refusal("cut-rle.psd", rle(&RGB, rows - 1)).contains("cut short"));
    assert!(refusal("header.psd", raw(&RGB)[..20].to_vec()).contains("cut short"));
    let mut no_sections = raw(&RGB);
    no_sections.truncate(30);
    assert!(refusal("sections.psd", no_sections).contains("cut short"));
}

#[test]
fn a_psb_file_is_refused_by_name() {
    let why = refusal("big.psd", raw(&Header { version: 2, ..RGB }));
    assert!(why.contains("PSB"), "{why}");
    assert!(refusal("v3.psd", raw(&Header { version: 3, ..RGB })).contains("version 3"));
}

#[test]
fn unsupported_header_values_are_refused() {
    for (label, h) in [
        ("no channels", Header { channels: 0, ..RGB }),
        ("57 channels", Header { channels: 57, ..RGB }),
        ("1-bit", Header { depth: 1, ..RGB }),
        ("32-bit", Header { depth: 32, ..RGB }),
        ("bitmap", Header { channels: 1, mode: 0, ..RGB }),
        ("indexed", Header { channels: 1, mode: 2, ..RGB }),
        ("multichannel", Header { mode: 7, ..RGB }),
        ("Lab", Header { mode: 9, ..RGB }),
        ("RGB in one channel", Header { channels: 1, ..RGB }),
        ("zero width", Header { width: 0, ..RGB }),
        ("too wide", Header { width: 30_001, ..RGB }),
        ("too tall", Header { height: 30_001, ..RGB }),
    ] {
        let why = refusal("bad.psd", psd(&h, 0, &[0u8; 64]));
        assert!(why.contains("cannot read this PSD file"), "{label}: {why}");
    }
    assert!(refusal("comp.psd", psd(&RGB, 4, &[0u8; 64])).contains("compression 4"));
}

#[test]
fn placing_a_damaged_psd_into_a_frame_is_refused() {
    let mut ses = Session::default();
    ses.execute("file.new", &json!({"preset": "A4", "pages": 1, "facingPages": false})).expect("file.new");
    let frame = ses.execute("frame.create", &json!({"spread": 0, "rect": [60, 60, 300, 300], "content": "graphic"})).expect("frame.create")["id"].as_u64().unwrap();
    let path = scratch("damaged.psd");
    std::fs::write(&path, psd(&Header { width: 30_000, height: 30_000, ..RGB }, 0, &[0u8; 64])).unwrap();
    let err = ses.execute("file.place", &json!({"path": path.to_string_lossy(), "frame": frame})).expect_err("a damaged PSD is not placed");
    assert!(err.0.contains("damaged.psd") && err.0.contains("cut short"), "{}", err.0);
    let ok = scratch("ok.psd");
    std::fs::write(&ok, raw(&RGB)).unwrap();
    ses.execute("file.place", &json!({"path": ok.to_string_lossy(), "frame": frame})).expect("a whole PSD places");
}
