use std::sync::Arc;

use formdesign_engine::Session;
use formdesign_model::doc::Doc;
use formdesign_model::ids::ItemId;
use formdesign_text::layout::DocLayout;
use serde_json::{Value, json};

const SERIF: &[u8] = include_bytes!("../../formdesign-core/fonts/SourceSerif4-Regular.ttf");
const SANS: &[u8] = include_bytes!("../../formdesign-core/fonts/SourceSans3-Regular.ttf");

fn session() -> Session {
    let ses = Session::default();
    ses.renderer.fonts.load_data(SERIF.to_vec());
    ses.renderer.fonts.load_data(SANS.to_vec());
    ses
}

fn doc(ses: &Session) -> Arc<Doc> {
    ses.active().expect("a document is open").doc.clone()
}

fn layout(ses: &mut Session) -> Arc<DocLayout> {
    let r = ses.renderer.clone();
    ses.active_mut().expect("a document is open").layout(&r)
}

fn run(ses: &mut Session, id: &str, p: Value) -> Value {
    ses.execute(id, &p).unwrap_or_else(|e| panic!("{id} failed: {}", e.0))
}

fn words(n: usize) -> String {
    let mut out = String::new();
    for i in 0..n {
        if i > 0 {
            out.push(if i % 60 == 0 { '\n' } else { ' ' });
        }
        out.push_str(["layout", "column", "frame", "thread", "story", "page"][i % 6]);
    }
    out
}

fn frame(ses: &mut Session, spread: usize, rect: [f64; 4], text: Option<&str>) -> u64 {
    let mut p = json!({"spread": spread, "rect": rect, "content": "text"});
    if let Some(t) = text {
        p["text"] = json!(t);
    }
    run(ses, "frame.create", p)["id"].as_u64().expect("frame id")
}

fn overset(ses: &mut Session, frame: u64) -> Option<u64> {
    run(ses, "story.get", json!({"frame": frame}))["overset"].as_u64()
}

fn lines_per_frame(ses: &mut Session, first: u64) -> Vec<usize> {
    let lay = layout(ses);
    let (sl, _) = lay.frame(ItemId(first)).expect("frame in layout");
    let mut counts = vec![0; sl.frames.len()];
    for l in &sl.flow.lines {
        counts[l.frame] += 1;
    }
    counts
}

#[test]
fn page_geometry_matches_three_paper_sizes() {
    for (preset, w, h) in [("Letter", 612.0, 792.0), ("A4", 595.28, 841.89), ("Tabloid", 792.0, 1224.0)] {
        let mut ses = session();
        run(&mut ses, "file.new", json!({"preset": preset, "pages": 2, "facingPages": false}));
        let doc = doc(&ses);
        assert_eq!(doc.page_count(), 2, "{preset}");
        for i in 0..2 {
            let r = doc.page_rect(i).unwrap();
            assert!((r.width() - w).abs() < 0.01 && (r.height() - h).abs() < 0.01, "{preset} page {i}: {} x {}", r.width(), r.height());
        }
    }
}

#[test]
fn facing_pages_sit_side_by_side_in_a_spread() {
    let mut ses = session();
    run(&mut ses, "file.new", json!({"preset": "A4", "pages": 3, "facingPages": true}));
    let doc = doc(&ses);
    assert_eq!(doc.page_count(), 3);
    let (a, b) = (doc.page_rect(1).unwrap(), doc.page_rect(2).unwrap());
    assert!((b.x0 - a.x1).abs() < 0.01, "the right page starts where the left page ends");
    assert!(doc.is_right_page(0) && !doc.is_right_page(1) && doc.is_right_page(2));
}

#[test]
fn custom_size_and_unknown_preset() {
    let mut ses = session();
    run(&mut ses, "file.new", json!({"width": 300, "height": 500}));
    let r = doc(&ses).page_rect(0).unwrap();
    assert!((r.width() - 300.0).abs() < 0.01 && (r.height() - 500.0).abs() < 0.01);
    let e = ses.execute("file.new", &json!({"preset": "NoSuchPaper"})).unwrap_err();
    assert!(e.0.contains("no page preset"), "{}", e.0);
}

#[test]
fn short_text_is_not_overset() {
    let mut ses = session();
    run(&mut ses, "file.new", json!({"preset": "Letter"}));
    let f = frame(&mut ses, 0, [72.0, 72.0, 540.0, 720.0], Some("A short line."));
    assert_eq!(overset(&mut ses, f), None);
}

#[test]
fn long_text_in_one_small_frame_is_overset() {
    let mut ses = session();
    run(&mut ses, "file.new", json!({"preset": "Letter"}));
    let text = words(600);
    let f = frame(&mut ses, 0, [72.0, 72.0, 300.0, 200.0], Some(&text));
    let at = overset(&mut ses, f).expect("overset reported");
    let len = run(&mut ses, "story.get", json!({"frame": f}))["length"].as_u64().unwrap();
    assert!(at > 0 && at < len, "overset at {at} of {len}");
}

#[test]
fn text_threads_across_frames_on_two_pages() {
    let mut ses = session();
    run(&mut ses, "file.new", json!({"preset": "Letter", "pages": 2, "facingPages": false}));
    let text = words(500);
    let a = frame(&mut ses, 0, [72.0, 72.0, 540.0, 360.0], Some(&text));
    let b = frame(&mut ses, 1, [72.0, 72.0, 540.0, 720.0], None);
    assert!(overset(&mut ses, a).is_some(), "one frame alone cannot hold the story");
    run(&mut ses, "frame.thread", json!({"from": a, "to": b}));
    let story = run(&mut ses, "story.get", json!({"frame": a}));
    assert_eq!(story["frames"], json!([a, b]));
    assert_eq!(run(&mut ses, "story.get", json!({"frame": b}))["id"], story["id"], "both frames show the same story");
    assert_eq!(overset(&mut ses, a), None, "the thread holds all of the text");
    let counts = lines_per_frame(&mut ses, a);
    assert!(counts[0] > 0 && counts[1] > 0, "text in both frames: {counts:?}");
}

#[test]
fn threaded_lines_continue_without_gaps_or_repeats() {
    let mut ses = session();
    run(&mut ses, "file.new", json!({"preset": "A4", "pages": 2, "facingPages": false}));
    let text = words(500);
    let a = frame(&mut ses, 0, [60.0, 60.0, 535.0, 400.0], Some(&text));
    let b = frame(&mut ses, 1, [60.0, 60.0, 535.0, 780.0], None);
    run(&mut ses, "frame.thread", json!({"from": a, "to": b}));
    let lay = layout(&mut ses);
    let (sl, _) = lay.frame(ItemId(a)).unwrap();
    let lines = &sl.flow.lines;
    assert!(!lines.is_empty());
    for w in lines.windows(2) {
        let (p, n) = (&w[0], &w[1]);
        assert!(n.frame >= p.frame, "frames run in thread order");
        if n.para == p.para {
            assert!(n.line.start >= p.line.end, "no text repeated across a line break");
            let gap = &sl.flow.para_texts[p.para][p.line.end..n.line.start];
            assert!(gap.chars().all(char::is_whitespace), "only the break space falls between lines: {gap:?}");
        } else {
            assert_eq!(n.para, p.para + 1, "no paragraph skipped");
        }
    }
    let first_b = lines.iter().position(|l| l.frame == 1).expect("a line in the second frame");
    assert!(first_b > 0 && lines[first_b - 1].frame == 0);
    let end = sl.flow.story_pos(lines.last().unwrap().para, lines.last().unwrap().line.end);
    let len = doc(&ses).story(sl.story).unwrap().text().chars().count();
    assert!(end >= len.saturating_sub(1), "the last line reaches the story end: {end} of {len}");
}

#[test]
fn breaking_the_thread_brings_the_overset_back() {
    let mut ses = session();
    run(&mut ses, "file.new", json!({"preset": "Letter", "pages": 2, "facingPages": false}));
    let text = words(500);
    let a = frame(&mut ses, 0, [72.0, 72.0, 540.0, 360.0], Some(&text));
    let b = frame(&mut ses, 1, [72.0, 72.0, 540.0, 720.0], None);
    run(&mut ses, "frame.thread", json!({"from": a, "to": b}));
    assert_eq!(overset(&mut ses, a), None);
    run(&mut ses, "frame.unthread", json!({"id": a}));
    assert!(overset(&mut ses, a).is_some());
    ses.undo().unwrap();
    assert_eq!(overset(&mut ses, a), None, "undo restores the thread");
}

#[test]
fn thread_to_a_missing_frame_is_refused() {
    let mut ses = session();
    run(&mut ses, "file.new", json!({"preset": "Letter"}));
    let a = frame(&mut ses, 0, [72.0, 72.0, 540.0, 360.0], Some("text"));
    let e = ses.execute("frame.thread", &json!({"from": a, "to": 999999})).unwrap_err();
    assert!(e.0.contains("999999"), "{}", e.0);
    assert_eq!(run(&mut ses, "story.get", json!({"frame": a}))["frames"], json!([a]));
}

#[test]
fn a_taller_frame_holds_more_lines() {
    let mut ses = session();
    run(&mut ses, "file.new", json!({"preset": "Letter"}));
    let text = words(600);
    let short = frame(&mut ses, 0, [72.0, 72.0, 300.0, 200.0], Some(&text));
    let tall = frame(&mut ses, 0, [310.0, 72.0, 538.0, 400.0], Some(&text));
    let (s, t) = (lines_per_frame(&mut ses, short)[0], lines_per_frame(&mut ses, tall)[0]);
    assert!(t > s && s > 0, "short {s}, tall {t}");
}
