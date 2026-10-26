use std::io::Read;
use std::path::PathBuf;

use formdesign_engine::Session;
use formdesign_model::format;
use serde_json::{Value, json};

const SERIF: &[u8] = include_bytes!("../../formdesign-core/fonts/SourceSerif4-Regular.ttf");

fn session() -> Session {
    let ses = Session::default();
    ses.renderer.fonts.load_data(SERIF.to_vec());
    ses
}

fn run(ses: &mut Session, id: &str, p: Value) -> Value {
    ses.execute(id, &p).unwrap_or_else(|e| panic!("{id} failed: {}", e.0))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("formdesign-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn sample(ses: &mut Session) -> (u64, u64) {
    run(ses, "file.new", json!({"preset": "A4", "pages": 2, "facingPages": false, "title": "Round trip"}));
    let body = (0..400).map(|i| ["sample", "text", "flows", "on"][i % 4]).collect::<Vec<_>>().join(" ");
    let a = run(ses, "frame.create", json!({"spread": 0, "rect": [60, 60, 535, 420], "content": "text", "text": body}))["id"].as_u64().unwrap();
    let b = run(ses, "frame.create", json!({"spread": 1, "rect": [60, 60, 535, 780], "content": "text"}))["id"].as_u64().unwrap();
    run(ses, "frame.thread", json!({"from": a, "to": b}));
    run(ses, "frame.create", json!({"spread": 0, "rect": [60, 440, 535, 780], "content": "graphic"}));
    (a, b)
}

fn as_json(doc: &formdesign_model::doc::Doc) -> Value {
    serde_json::to_value(doc).unwrap()
}

#[test]
fn native_file_saves_loads_and_saves_again_unchanged() {
    let mut ses = session();
    sample(&mut ses);
    let original = ses.active().unwrap().doc.clone();
    let first = format::save(&original).unwrap();
    let loaded = format::load(&first).unwrap();
    let second = format::save(&loaded).unwrap();
    assert_eq!(first, second, "the second save is byte-identical");
    assert_eq!(as_json(&loaded), as_json(&original), "the loaded document equals the original");
}

#[test]
fn save_as_and_open_keep_the_thread() {
    let mut ses = session();
    let (a, b) = sample(&mut ses);
    let path = scratch("thread.fdes");
    run(&mut ses, "file.saveAs", json!({"path": path}));
    run(&mut ses, "file.open", json!({"path": path}));
    let story = run(&mut ses, "story.get", json!({"frame": a}));
    assert_eq!(story["frames"], json!([a, b]));
    assert!(story["overset"].is_null());
    assert_eq!(ses.active().unwrap().doc.page_count(), 2);
}

#[test]
fn a_file_that_is_not_a_document_is_refused() {
    assert!(format::load(b"not a document").is_err());
    assert!(format::load(&[]).is_err());
}

#[test]
fn idml_export_is_a_valid_package() {
    let mut ses = session();
    sample(&mut ses);
    let path = scratch("package.idml");
    run(&mut ses, "file.saveAs", json!({"path": path}));
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"PK"));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).unwrap();
    let names: Vec<String> = zip.file_names().map(str::to_string).collect();
    assert_eq!(names.first().map(String::as_str), Some("mimetype"), "mimetype comes first");
    let mut mime = String::new();
    zip.by_name("mimetype").unwrap().read_to_string(&mut mime).unwrap();
    assert_eq!(mime, "application/vnd.adobe.indesign-idml-package");
    assert!(names.iter().any(|n| n == "designmap.xml"));
    assert!(names.iter().any(|n| n.starts_with("Spreads/") && n.ends_with(".xml")));
    assert!(names.iter().any(|n| n.starts_with("Stories/") && n.ends_with(".xml")));
    let mut map = String::new();
    zip.by_name("designmap.xml").unwrap().read_to_string(&mut map).unwrap();
    for n in names.iter().filter(|n| n.starts_with("Spreads/") || n.starts_with("Stories/")) {
        assert!(map.contains(n.as_str()), "designmap.xml lists {n}");
    }
}

#[test]
fn idml_export_reopens_with_the_same_pages_and_text() {
    let mut ses = session();
    let (a, _) = sample(&mut ses);
    let text = run(&mut ses, "story.get", json!({"frame": a}))["text"].as_str().unwrap().to_string();
    let path = scratch("reopen.idml");
    run(&mut ses, "file.saveAs", json!({"path": path}));
    run(&mut ses, "file.open", json!({"path": path}));
    let doc = ses.active().unwrap().doc.clone();
    assert_eq!(doc.page_count(), 2);
    assert!(doc.stories.values().any(|s| s.text().trim() == text.trim()), "the story text survives");
}
