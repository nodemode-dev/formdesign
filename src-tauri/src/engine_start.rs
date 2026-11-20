use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const APP: &str = "FormDesign";
pub const SHELL_ENV: &str = "FORMDESIGN_SHELL_PID";
pub const ADDRESS_WAIT: Duration = Duration::from_secs(90);
pub const KEY_WAIT: Duration = Duration::from_secs(10);
pub const EXIT_WAIT: Duration = Duration::from_secs(1);
pub const BUSY_CODE: i32 = 2;
const DATA_NAME: &str = "FormDesign";
const DATA_ENV: &str = "FORMDESIGN_DATA_DIR";
const LOG_NAME: &str = "formdesign-engine.log";
const KEY_MARK: &str = "window-key";
const TAIL_LINES: usize = 12;
const LINE_CHARS: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wait {
    Address,
    Key,
}

impl Wait {
    pub fn limit(self) -> Duration {
        match self {
            Wait::Address => ADDRESS_WAIT,
            Wait::Key => KEY_WAIT,
        }
    }

    pub fn what(self) -> &'static str {
        match self {
            Wait::Address => "its engine to say which address it serves on",
            Wait::Key => "its engine to hand over the key for the window",
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Failure {
    Missing,
    NotStarted(String),
    Stopped(Option<i32>),
    TimedOut(Wait),
    Window,
}

#[derive(Default)]
pub struct Tail(VecDeque<String>);

impl Tail {
    pub fn push(&mut self, line: &str) {
        if line.contains(KEY_MARK) {
            return;
        }
        let clean: String = line.chars().filter(|c| !c.is_control()).take(LINE_CHARS).collect();
        if clean.trim().is_empty() {
            return;
        }
        if self.0.len() == TAIL_LINES {
            self.0.pop_front();
        }
        self.0.push_back(clean);
    }

    pub fn lines(&self) -> Vec<String> {
        self.0.iter().cloned().collect()
    }
}

pub fn sentence(failure: &Failure) -> String {
    match failure {
        Failure::Missing => format!("{APP} could not start: its engine program is missing from the install. Installing {APP} again puts it back."),
        Failure::NotStarted(reason) => format!("{APP} could not start its engine: {reason}"),
        Failure::Stopped(Some(BUSY_CODE)) => {
            format!("{APP} could not start: another copy of its engine holds its data folder. Close the other {APP} and open it again.")
        }
        Failure::Stopped(Some(code)) => format!("{APP} could not start: its engine stopped with code {code} before the window could open."),
        Failure::Stopped(None) => format!("{APP} could not start: its engine stopped before the window could open."),
        Failure::TimedOut(wait) => format!("{APP} could not start: it waited {} seconds for {} and that did not happen.", wait.limit().as_secs(), wait.what()),
        Failure::Window => format!("{APP} could not open its window. Please open {APP} again."),
    }
}

pub fn message(failure: &Failure, tail: &[String], log: Option<&Path>) -> String {
    let mut text = sentence(failure);
    if !tail.is_empty() {
        text.push_str("\n\nThe engine's last lines:\n");
        text.push_str(&tail.join("\n"));
    } else if matches!(failure, Failure::Stopped(_) | Failure::TimedOut(_)) {
        text.push_str("\n\nThe engine wrote nothing before that.");
    }
    if let Some(path) = log {
        text.push_str("\n\nThe engine's log: ");
        text.push_str(&path.to_string_lossy());
    }
    text
}

pub fn data_folder(var: &dyn Fn(&str) -> Option<String>, home: Option<&Path>) -> Option<PathBuf> {
    let raw = var(DATA_ENV).map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
    if let Some(raw) = raw {
        return match (raw.strip_prefix('~'), home) {
            (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') => {
                Some(home.join(rest.trim_start_matches(['/', '\\'])))
            }
            _ => Some(PathBuf::from(raw)),
        };
    }
    Some(home?.join(DATA_NAME))
}

pub fn log_path(var: &dyn Fn(&str) -> Option<String>, home: Option<&Path>) -> Option<PathBuf> {
    data_folder(var, home).map(|folder| folder.join("Logs").join(LOG_NAME))
}

pub fn home_folder(os: &str, var: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let name = if os == "windows" { "USERPROFILE" } else { "HOME" };
    var(name).filter(|value| !value.is_empty()).map(PathBuf::from)
}

pub fn engine_log() -> Option<PathBuf> {
    let var = |name: &str| std::env::var(name).ok();
    let home = home_folder(std::env::consts::OS, &var);
    log_path(&var, home.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(name: &'static str, value: &'static str) -> impl Fn(&str) -> Option<String> {
        move |asked| (asked == name).then(|| value.to_string())
    }

    #[test]
    fn the_tail_keeps_the_last_lines_and_never_the_window_key() {
        let mut tail = Tail::default();
        for n in 0..15 {
            tail.push(&format!("line {n}"));
        }
        tail.push("formdesign-window-key: AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
        tail.push("   ");
        tail.push(&"x".repeat(1000));
        let lines = tail.lines();
        assert_eq!(lines.len(), TAIL_LINES);
        assert_eq!(lines[0], "line 4");
        assert_eq!(lines[TAIL_LINES - 1].len(), LINE_CHARS);
        assert!(lines.iter().all(|line| !line.contains("window-key")));
    }

    #[test]
    fn every_failure_has_its_sentence_the_last_lines_and_the_log() {
        let log = Path::new("/data/Logs/formdesign-engine.log");
        let said = vec!["formdesign is already running on /data (engine process 7).".to_string()];
        let busy = message(&Failure::Stopped(Some(BUSY_CODE)), &said, Some(log));
        assert!(busy.starts_with("FormDesign could not start: another copy of its engine holds its data folder."));
        assert!(busy.contains("The engine's last lines:\nformdesign is already running on /data (engine process 7)."));
        assert!(busy.ends_with("The engine's log: /data/Logs/formdesign-engine.log"));
        assert!(message(&Failure::Stopped(Some(3)), &[], Some(log)).contains("stopped with code 3"));
        assert!(message(&Failure::Stopped(None), &[], None).ends_with("The engine wrote nothing before that."));
        assert!(message(&Failure::Missing, &[], None).ends_with("puts it back."));
        assert!(sentence(&Failure::NotStarted("Access is denied.".to_string())).ends_with("its engine: Access is denied."));
        assert!(sentence(&Failure::Window).starts_with("FormDesign could not open its window."));
    }

    #[test]
    fn a_wait_that_runs_out_says_how_long_and_for_what() {
        assert!(sentence(&Failure::TimedOut(Wait::Address)).contains("waited 90 seconds for its engine to say which address it serves on"));
        assert!(sentence(&Failure::TimedOut(Wait::Key)).contains("waited 10 seconds for its engine to hand over the key for the window"));
    }

    #[test]
    fn the_log_is_where_the_engine_writes_it() {
        let home = Path::new("/home/u");
        let none = |_: &str| None;
        let tail = Path::new("Logs").join("formdesign-engine.log");
        assert_eq!(log_path(&none, Some(home)), Some(home.join("FormDesign").join(&tail)));
        assert_eq!(log_path(&only("FORMDESIGN_DATA_DIR", " /d/x "), None), Some(Path::new("/d/x").join(&tail)));
        assert_eq!(log_path(&only("FORMDESIGN_DATA_DIR", "~/x"), Some(home)), Some(home.join("x").join(&tail)));
        assert_eq!(log_path(&only("FORMDESIGN_DATA_DIR", "~"), Some(home)), Some(home.join(&tail)));
        assert_eq!(log_path(&only("FORMDESIGN_DATA_DIR", "   "), Some(home)), Some(home.join("FormDesign").join(&tail)));
        assert_eq!(log_path(&none, None), None);
    }

    #[test]
    fn the_home_folder_is_read_the_way_the_engine_reads_it() {
        assert_eq!(home_folder("windows", &only("USERPROFILE", "C:\\Users\\u")), Some(PathBuf::from("C:\\Users\\u")));
        assert_eq!(home_folder("windows", &only("HOME", "/home/u")), None);
        assert_eq!(home_folder("linux", &only("HOME", "/home/u")), Some(PathBuf::from("/home/u")));
        assert_eq!(home_folder("macos", &only("HOME", "")), None);
    }
}
