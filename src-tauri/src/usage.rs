//! Lancements comptés (nombre et date du dernier) pour classer la recherche et remplir la
//! rangée Récents. Fichier usage.json à côté de apps.json ; clé = cible et arguments, qui
//! ne changent pas quand apps.json est réorganisé (contrairement aux identifiants).

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
    /// Secondes depuis 1970
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

/// Au démarrage ; fichier absent ou illisible : on repart de zéro
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
    // Sous le verrou : deux lancements rapprochés n'écrivent pas le fichier en même temps
    if let (Some(path), Ok(text)) = (file(app), serde_json::to_string(&*map)) {
        if let Err(e) = fs::write(path, text) {
            eprintln!("[lanceur] usage.json : {e}");
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
