//! Pastille's Settings page (requested by Funsaiki on 2026-10-21): what the tray icon menu
//! used to set (Start with Windows, pastel bar, Spotify, apps.json, Quit), and the language.
//! The tray icon menu keeps only Show, Settings and Quit (tray.rs).
//!
//! The language is kept in settings.json, next to apps.json (not in apps.json: an older version
//! would refuse it). With no choice, the Windows language: French if Windows is in
//! French, otherwise English. Rust follows it (i18n.rs); the pages keep a copy (i18n.ts).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;
use windows::Win32::Globalization::GetUserDefaultUILanguage;

use crate::i18n::{self, tr};
use crate::{bar, config, shell, spotify, tray, update};

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Fr,
    En,
}

/* ── Settings: settings.json (reglages.json until 2026-10-22) ── */

#[derive(Default, Serialize, Deserialize)]
struct Settings {
    /// Missing: the Windows language. An unknown value (newer version) makes the file
    /// unreadable: we fall back to the Windows language.
    #[serde(default, alias = "langue", skip_serializing_if = "Option::is_none")]
    language: Option<Language>,
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("settings.json"))
}

fn settings(app: &AppHandle) -> Settings {
    file(app)
        .and_then(|path| fs::read_to_string(config::to_read(path)).ok())
        .and_then(|text| serde_json::from_str(text.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

/// Temporary file then rename, like bar.json
fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = file(app).ok_or(tr("Dossier de config introuvable.", "Config folder not found."))?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, serde_json::to_string_pretty(settings).unwrap_or_default())
        .and_then(|_| fs::rename(&temp, &path))
        .map_err(|e| tr(format!("{} non enregistré : {e}", path.display()), format!("{} not saved: {e}", path.display())))
}

/// Windows language (that of its interface): French for all its variants
fn windows_language() -> Language {
    const LANG_FRENCH: u16 = 0x0c;
    if unsafe { GetUserDefaultUILanguage() } & 0x3ff == LANG_FRENCH { Language::Fr } else { Language::En }
}

/// At startup, before any text (config problems, tray icon menu)
pub fn init(app: &AppHandle) {
    let language = settings(app).language.unwrap_or_else(windows_language);
    i18n::set_english(language == Language::En);
}

pub fn language() -> Language {
    if i18n::english() { Language::En } else { Language::Fr }
}

/// Asked by each page when it loads: it reloads if it shows another one
#[tauri::command]
pub fn get_language() -> Language {
    language()
}

/* ── Start with Windows ── */

/// Writes to the registry (HKCU\…\Run): only Funsaiki turns it on. Refused in development:
/// the entry would point to the debug exe.
fn set_autostart(app: &AppHandle, on: bool) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err(tr("Seulement dans la version installée.", "Only in the installed version.").into());
    }
    let manager = app.autolaunch();
    if on { manager.enable() } else { manager.disable() }
        .map_err(|e| tr(format!("Démarrage avec Windows impossible : {e}"), format!("Couldn't start with Windows: {e}")))
}

pub fn autostart_enabled(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// Welcome notification (first launch, on other PCs): "Start with
/// Windows" in one click, like the page's switch
#[tauri::command]
pub fn enable_autostart(app: AppHandle) -> Result<(), String> {
    set_autostart(&app, true)
}

/* ── Page commands ── */

#[derive(Serialize)]
pub struct Prefs {
    language: Language,
    /// Missing in development (switch grayed out)
    autostart: Option<bool>,
    bar: bool,
    /// The one from apps.json ("Alt+Space")
    shortcut: String,
    /// "connected", "disconnected", "outdated" (from before the playlists) or "pending"
    /// (connection in progress in the browser)
    spotify: &'static str,
    version: String,
    /// Update found, waiting for a click
    update: Option<String>,
}

#[tauri::command]
pub fn prefs(app: AppHandle) -> Prefs {
    Prefs {
        language: language(),
        autostart: (!cfg!(debug_assertions)).then(|| autostart_enabled(&app)),
        bar: bar::running(),
        shortcut: config::shortcut(&app),
        spotify: spotify::link(&app),
        version: app.package_info().version.to_string(),
        update: update::available(&app),
    }
}

/// Rust's texts switch to the new language (config rebuilt: "Hidden" group,
/// problems; playlists read again: "Liked Songs"); the pages reload
/// ("language:changed"). Main thread (synchronous command): the tray icon menu is rebuilt there.
#[tauri::command]
pub fn set_language(app: AppHandle, language: Language) -> Result<(), String> {
    let mut current = settings(&app);
    current.language = Some(language);
    save(&app, &current)?;
    i18n::set_english(language == Language::En);
    tray::relabel(&app);
    config::reload(&app);
    config::rescan(&app);
    let _ = app.emit("language:changed", language);
    Ok(())
}

/// Switches. Main thread (synchronous command), as the tray icon menu was.
#[tauri::command]
pub fn set_pref_switch(app: AppHandle, key: String, on: bool) -> Result<(), String> {
    match key.as_str() {
        "autostart" => set_autostart(&app, on),
        "bar" => {
            if bar::running() != on {
                bar::toggle(&app);
            }
            Ok(())
        }
        _ => Err(tr(format!("Réglage inconnu : {key}."), format!("Unknown setting: {key}."))),
    }
}

/// Buttons. Main thread: config::reload must be called there.
#[tauri::command]
pub fn pref_action(app: AppHandle, action: String) -> Result<(), String> {
    match action.as_str() {
        // Connecting in the browser, disconnecting, or reopening the page of the connection in progress
        "spotify" => spotify::toggle(&app),
        "edit-config" => return edit_config(&app),
        "reload" => {
            config::reload(&app);
            spotify::reload(&app);
        }
        "quit" => app.exit(0),
        _ => return Err(tr(format!("Action inconnue : {action}."), format!("Unknown action: {action}."))),
    }
    Ok(())
}

/// apps.json in the editor associated with .json files, otherwise in Notepad
fn edit_config(app: &AppHandle) -> Result<(), String> {
    config::path(app)
        .and_then(|path| {
            let path = path.to_string_lossy().into_owned();
            shell::open(&path, &[]).or_else(|_| shell::open("notepad.exe", &[path]))
        })
        .map_err(|e| tr(format!("Ouverture de apps.json impossible : {e}"), format!("Couldn't open apps.json: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_is_read_and_unknown_values_fall_back() {
        let read = |text: &str| serde_json::from_str::<Settings>(text).ok().and_then(|s| s.language);
        assert_eq!(read(r#"{ "language": "en" }"#), Some(Language::En));
        assert_eq!(read(r#"{ "language": "fr" }"#), Some(Language::Fr));
        assert_eq!(read(r#"{ "langue": "en" }"#), Some(Language::En), "name until 2026-10-22");
        assert_eq!(read("{}"), None);
        assert_eq!(read(r#"{ "language": "de" }"#), None);
        assert_eq!(serde_json::to_string(&Settings { language: Some(Language::En) }).unwrap(), r#"{"language":"en"}"#);
        assert_eq!(serde_json::to_string(&Settings::default()).unwrap(), "{}");
    }
}
