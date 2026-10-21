//! Page Paramètres de Pastille (demandée par Funsaiki le 21/10/2026) : ce que réglait le menu de
//! l'icône (démarrer avec Windows, barre pastel, Spotify, apps.json, quitter), et la langue.
//! Le menu de l'icône ne garde qu'Afficher, Paramètres et Quitter (tray.rs).
//!
//! La langue est gardée dans reglages.json, à côté de apps.json (pas dans apps.json : une version
//! plus ancienne le refuserait). Sans choix, celle de Windows : français si Windows est en
//! français, sinon anglais. Rust la suit (i18n.rs) ; les pages en gardent une copie (i18n.ts).

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

/* ── Réglages : reglages.json ── */

#[derive(Default, Serialize, Deserialize)]
struct Settings {
    /// Absente : celle de Windows. Une valeur inconnue (version plus récente) rend le fichier
    /// illisible : on revient à celle de Windows.
    #[serde(default, rename = "langue", skip_serializing_if = "Option::is_none")]
    language: Option<Language>,
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("reglages.json"))
}

fn settings(app: &AppHandle) -> Settings {
    file(app)
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(text.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

/// Fichier temporaire puis renommage, comme barre.json
fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = file(app).ok_or(tr("Dossier de config introuvable.", "Config folder not found."))?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, serde_json::to_string_pretty(settings).unwrap_or_default())
        .and_then(|_| fs::rename(&temp, &path))
        .map_err(|e| tr(format!("{} non enregistré : {e}", path.display()), format!("{} not saved: {e}", path.display())))
}

/// Langue de Windows (celle de son interface) : français pour toutes ses variantes
fn windows_language() -> Language {
    const LANG_FRENCH: u16 = 0x0c;
    if unsafe { GetUserDefaultUILanguage() } & 0x3ff == LANG_FRENCH { Language::Fr } else { Language::En }
}

/// Au démarrage, avant tout texte (problèmes de la config, menu de l'icône)
pub fn init(app: &AppHandle) {
    let language = settings(app).language.unwrap_or_else(windows_language);
    i18n::set_english(language == Language::En);
}

pub fn language() -> Language {
    if i18n::english() { Language::En } else { Language::Fr }
}

/// Demandée par chaque page à son chargement : elle se recharge si elle en affiche une autre
#[tauri::command]
pub fn get_language() -> Language {
    language()
}

/* ── Démarrer avec Windows ── */

/// Écrit dans le registre (HKCU\…\Run) : seul Funsaiki l'active. Refusé en développement :
/// l'entrée pointerait vers l'exe de debug.
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

/// Notification de bienvenue (premier lancement, pour les amis de Funsaiki) : « Démarrer avec
/// Windows » d'un clic, comme l'interrupteur de la page
#[tauri::command]
pub fn enable_autostart(app: AppHandle) -> Result<(), String> {
    set_autostart(&app, true)
}

/* ── Commandes de la page ── */

#[derive(Serialize)]
pub struct Prefs {
    language: Language,
    /// Absent en développement (interrupteur grisé)
    autostart: Option<bool>,
    bar: bool,
    /// Celui de apps.json (« Alt+Space »)
    shortcut: String,
    /// « connected », « disconnected », « outdated » (d'avant les playlists) ou « pending »
    /// (connexion en cours dans le navigateur)
    spotify: &'static str,
    version: String,
    /// Mise à jour trouvée, en attente d'un clic
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

/// Les textes de Rust passent à la nouvelle langue (config refaite : groupe « Masquées »,
/// problèmes ; playlists relues : « Titres likés ») ; les pages se rechargent
/// (« language:changed »). Fil principal (commande synchrone) : le menu de l'icône y est refait.
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

/// Interrupteurs. Fil principal (commande synchrone), comme l'était le menu de l'icône.
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

/// Boutons. Fil principal : config::reload doit y être appelée.
#[tauri::command]
pub fn pref_action(app: AppHandle, action: String) -> Result<(), String> {
    match action.as_str() {
        // Connexion dans le navigateur, déconnexion, ou la page de la connexion en cours rouverte
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

/// apps.json dans l'éditeur associé aux fichiers .json, sinon dans le Bloc-notes
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
        assert_eq!(read(r#"{ "langue": "en" }"#), Some(Language::En));
        assert_eq!(read(r#"{ "langue": "fr" }"#), Some(Language::Fr));
        assert_eq!(read("{}"), None);
        assert_eq!(read(r#"{ "langue": "de" }"#), None);
        assert_eq!(serde_json::to_string(&Settings { language: Some(Language::En) }).unwrap(), r#"{"langue":"en"}"#);
        assert_eq!(serde_json::to_string(&Settings::default()).unwrap(), "{}");
    }
}
