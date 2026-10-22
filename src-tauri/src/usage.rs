//! Launch counts (number and date of the last one) to rank search results and fill the
//! Recent row. File usage.json next to apps.json; key = target and arguments, which
//! don't change when apps.json is reorganized (unlike the ids).

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Entry {
    pub count: u32,
    /// Seconds since 1970
    pub last: u64,
}

#[derive(Default)]
pub struct Usage(Mutex<HashMap<String, Entry>>);

pub fn key(target: &str, args: &[String]) -> String {
    std::iter::once(target).chain(args.iter().map(String::as_str)).collect::<Vec<_>>().join("\u{1f}")
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("usage.json"))
}

/// At startup; file missing or unreadable: start from scratch
pub fn load(app: &AppHandle) -> Usage {
    let map = file(app)
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    Usage(Mutex::new(map))
}

pub fn record(app: &AppHandle, key: String) {
    let usage = app.state::<Usage>();
    let mut map = usage.0.lock().unwrap();
    let entry = map.entry(key).or_default();
    entry.count += 1;
    entry.last = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    // Under the lock: two launches close together don't write the file at the same time
    if let (Some(path), Ok(text)) = (file(app), serde_json::to_string(&*map)) {
        if let Err(e) = fs::write(path, text) {
            eprintln!("[pastille] usage.json: {e}");
        }
    }
}

pub fn get(app: &AppHandle, key: &str) -> Option<Entry> {
    app.state::<Usage>().0.lock().unwrap().get(key).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_include_arguments() {
        assert_eq!(key("Code.exe", &[]), "Code.exe");
        assert_ne!(key("launcher.exe", &["--game=a".into()]), key("launcher.exe", &["--game=b".into()]));
    }
}
