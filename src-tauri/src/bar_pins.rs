//! Applications épinglées à la barre pastel (demande de Funsaiki, 17/10/2026) : comme dans la barre
//! de Windows, une tuile toujours présente par application épinglée, même fermée ; un clic la
//! lance. Épinglées depuis le clic droit d'une tuile de fenêtre, gardées dans barre.json
//! (`pinned`, bar.rs).
//!
//! Reconnues par le nom de leur programme (« discord.exe ») : le dossier de Discord change à
//! chaque mise à jour (app-1.0.x), son chemin est remis à jour quand une de ses fenêtres passe
//! dans la barre (`follow`, même nom d'application : pas un autre « launcher.exe »). Lancées
//! par leur identifiant d'application pour celles du Store (Spotify, Terminal : leur dossier
//! change aussi), sinon par leur chemin. Le front n'envoie jamais de chemin : une fenêtre (pour
//! épingler) ou un nom de programme. Les apps hébergées par ApplicationFrameHost (Paramètres,
//! Calculatrice) ne s'épinglent pas : elles partagent ce programme.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::bar::{self, BarWindow};
use crate::i18n::tr;
use crate::{open_windows, switch, tray_apps};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pin {
    /// Nom du programme en minuscules (« discord.exe ») : reconnaît ses fenêtres
    pub exe: String,
    pub name: String,
    /// Chemin du programme : son icône, et son lancement hors Store
    pub path: String,
    /// App du Store : lancée par `shell:AppsFolder\<identifiant>`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
}

/// Nom du programme d'un chemin, en minuscules
pub fn exe_of(path: &str) -> Option<String> {
    Path::new(path).file_name().map(|name| name.to_string_lossy().to_lowercase())
}

#[derive(Serialize)]
pub struct PinnedApp {
    exe: String,
    name: String,
    /// Pas pour les programmes dont le front a déjà l'icône (`known`)
    icon: Option<crate::icon::Icon>,
}

/// Applications épinglées, dans leur ordre
#[tauri::command]
pub async fn bar_pins(app: AppHandle, known: Vec<String>) -> Vec<PinnedApp> {
    let pins = bar::pins(&app);
    crate::com::com_task(move || {
        Ok(pins
            .into_iter()
            .map(|pin| PinnedApp {
                icon: if known.contains(&pin.exe) { None } else { crate::icon::of(&pin.path) },
                exe: pin.exe,
                name: pin.name,
            })
            .collect())
    })
    .await
    .unwrap_or_default()
}

/// Épingle le programme de la fenêtre `id`, à la fin
#[tauri::command]
pub async fn bar_pin(app: AppHandle, id: String) -> Result<(), String> {
    let pin = crate::blocking(move || {
        let hwnd = open_windows::app_window(&id)?;
        let (path, pid) = switch::process_of(hwnd);
        let missing = || tr("Programme de la fenêtre introuvable.", "The window's program was not found.").to_string();
        let path = path.ok_or_else(missing)?;
        if bar::is_hosted(&path) {
            return Err(tr("Cette application ne peut pas être épinglée.", "This app can't be pinned.").to_string());
        }
        let app_id = if tray_apps::is_packaged(&path) {
            Some(tray_apps::app_id(pid).ok_or(tr("Identifiant de l'application introuvable.", "The app's identifier was not found."))?)
        } else {
            None
        };
        program_pin(&path, app_id).ok_or_else(missing)
    })
    .await??;
    add(&app, pin);
    Ok(())
}

#[tauri::command]
pub fn bar_unpin(app: AppHandle, exe: String) {
    remove(&app, &exe);
}

/// Épingle du programme `path`, nommé comme ses fenêtres dans la barre (`follow` les compare)
pub fn program_pin(path: &str, app_id: Option<String>) -> Option<Pin> {
    let exe = exe_of(path)?;
    let name = crate::mixer::description(path).unwrap_or_else(|| {
        Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| exe.clone())
    });
    Some(Pin { exe, name, path: path.to_string(), app_id })
}

/// À la fin, sauf si ce programme est déjà épinglé ; aussi depuis le clic droit du lanceur (app_menu.rs)
pub fn add(app: &AppHandle, pin: Pin) {
    bar::update(app, |settings| {
        if !settings.pinned.iter().any(|p| p.exe == pin.exe) {
            settings.pinned.push(pin);
        }
    });
    bar::emit_all(app, "bar:pins");
}

pub fn remove(app: &AppHandle, exe: &str) {
    bar::update(app, |settings| settings.pinned.retain(|p| p.exe != exe));
    bar::emit_all(app, "bar:pins");
}

/// Lance l'application épinglée `exe` (fermée : ses fenêtres ouvertes passent par leur tuile)
#[tauri::command]
pub async fn bar_open_pin(app: AppHandle, exe: String) -> Result<(), String> {
    let pin = bar::pins(&app).into_iter().find(|p| p.exe == exe).ok_or(tr("Cette application n'est plus épinglée.", "This app is no longer pinned."))?;
    crate::blocking(move || match &pin.app_id {
        Some(id) => crate::shell::open_as_is(&format!(r"shell:AppsFolder\{id}")),
        None => crate::shell::open_as_is(&pin.path),
    })
    .await?
}

/// Fenêtres vues dans la barre : le chemin d'une application épinglée qui a changé (mise à
/// jour de Discord) est remis à jour
pub fn follow(app: &AppHandle, windows: &[BarWindow]) {
    let moved: Vec<(String, String)> = bar::pins(app)
        .iter()
        .filter(|pin| pin.app_id.is_none())
        .filter_map(|pin| {
            let w = windows.iter().find(|w| w.exe == pin.exe && w.app == pin.name)?;
            (!w.path.eq_ignore_ascii_case(&pin.path)).then(|| (pin.exe.clone(), w.path.clone()))
        })
        .collect();
    if moved.is_empty() {
        return;
    }
    bar::update(app, |settings| {
        for pin in &mut settings.pinned {
            if let Some((_, path)) = moved.iter().find(|(exe, _)| *exe == pin.exe) {
                pin.path = path.clone();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs() {
        assert_eq!(exe_of(r"C:\Users\x\AppData\Local\Discord\app-1.0.9\Discord.exe").as_deref(), Some("discord.exe"));
        assert!(bar::is_hosted(r"C:\Windows\System32\ApplicationFrameHost.exe"));
        assert!(!bar::is_hosted(r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe"));
        // Ancien barre.json, sans épingles ; épingle sans identifiant du Store
        let pin: Pin = serde_json::from_str(r#"{ "exe": "code.exe", "name": "Code", "path": "C:/Code.exe" }"#).unwrap();
        assert!(pin.app_id.is_none());
        assert!(!serde_json::to_string(&pin).unwrap().contains("appId"));
    }
}
