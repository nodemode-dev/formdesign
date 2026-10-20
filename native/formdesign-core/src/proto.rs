use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::core::Core;

pub struct Reply {
    pub result: Value,
    pub bin: Option<Vec<u8>>,
}

impl From<Value> for Reply {
    fn from(result: Value) -> Self {
        Reply { result, bin: None }
    }
}

struct StderrLog;

impl log::Log for StderrLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            eprintln!("[formdesign-core] {} {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}

static LOGGER: StderrLog = StderrLog;

pub fn init_logger() {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
}

pub fn serve(core: &mut Core) {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut line = String::new();
    loop {
        line.clear();
        match input.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let text = line.trim();
        if text.is_empty() {
            continue;
        }
        let (id, out) = match serde_json::from_str::<Value>(text) {
            Ok(req) => {
                let id = req.get("id").cloned().unwrap_or(Value::Null);
                let method = req.get("method").and_then(Value::as_str).unwrap_or("").to_string();
                let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| core.call(&method, params)));
                (id, r.unwrap_or_else(|_| Err(format!("`{method}` failed inside the layout core; the document is unchanged where the engine could roll it back"))))
            }
            Err(e) => (Value::Null, Err(format!("not a JSON request: {e}"))),
        };
        let meta = core.meta();
        let (header, bin) = match out {
            Ok(reply) => {
                let mut h = json!({"id": id, "ok": true, "result": reply.result});
                if let Some(b) = &reply.bin {
                    h["bin"] = json!(b.len());
                }
                (h, reply.bin)
            }
            Err(e) => (json!({"id": id, "ok": false, "error": e}), None),
        };
        let mut header = header;
        if let (Some(h), Value::Object(m)) = (header.as_object_mut(), meta) {
            for (k, v) in m {
                h.insert(k, v);
            }
        }
        let mut o = stdout.lock();
        let line_out = serde_json::to_string(&header).unwrap_or_else(|_| "{\"ok\":false,\"error\":\"unserializable reply\"}".into());
        if o.write_all(line_out.as_bytes()).is_err() || o.write_all(b"\n").is_err() {
            break;
        }
        if let Some(b) = bin
            && o.write_all(&b).is_err()
        {
            break;
        }
        if o.flush().is_err() {
            break;
        }
    }
}
