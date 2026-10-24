//! Apps pinned to the pastel bar (requested by Funsaiki, 2026-10-17): as in the Windows
//! taskbar, one tile always there per pinned app, even when closed; a click
//! launches it. Pinned from the right-click menu of a window tile, kept in bar.json
//! (`pinned`, bar.rs).
//!
//! Recognized by the name of their program ("discord.exe"): Discord's folder changes with
//! each update (app-1.0.x), its path is updated when one of its windows shows up
//! in the bar (`follow`, same app name: not another "launcher.exe"). Launched
//! by their app identifier for Store apps (Spotify, Terminal: their folder
//! changes too), otherwise by their path. The front never sends a path: a window (to
//! pin) or a program name. Apps hosted by ApplicationFrameHost (Windows Settings,
//! Calculator) can't be pinned: they share that program.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::bar::{self, BarWindow};
use crate::i18n::tr;
use crate::{open_windows, switch, tray_apps};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pin {
    /// Program name in lowercase ("discord.exe"): recognizes its windows
    pub exe: String,
    pub name: String,
    /// Program path: its icon, and how it is launched outside the Store
    pub path: String,
    /// Store app: launched by `shell:AppsFolder\<identifier>`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
}

/// Program name of a path, in lowercase
pub fn exe_of(path: &str) -> Option<String> {
    Path::new(path).file_name().map(|name| name.to_string_lossy().to_lowercase())
}

#[derive(Serialize)]
pub struct PinnedApp {
    exe: String,
    name: String,
    /// Not for programs whose icon the front already has (`known`)
    icon: Option<crate::icon::Icon>,
}

/// Pinned apps, in their order
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

/// Pins the program of the window `id`, at the end
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

/// Pin for the program `path`, named like its windows in the bar (`follow` compares them)
pub fn program_pin(path: &str, app_id: Option<String>) -> Option<Pin> {
    let exe = exe_of(path)?;
    Some(Pin { exe, name: crate::mixer::program_name(path), path: path.to_string(), app_id })
}

/// At the end, unless this program is already pinned; also from the launcher's right-click menu (app_menu.rs)
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

/// Launches the pinned app `exe` (when closed: its open windows go through their own tile)
#[tauri::command]
pub async fn bar_open_pin(app: AppHandle, exe: String) -> Result<(), String> {
    let pin = bar::pins(&app).into_iter().find(|p| p.exe == exe).ok_or(tr("Cette application n'est plus épinglée.", "This app is no longer pinned."))?;
    crate::blocking(move || match &pin.app_id {
        Some(id) => crate::shell::open_as_is(&format!(r"shell:AppsFolder\{id}")),
        None => crate::shell::open_as_is(&pin.path),
    })
    .await?
}

/// Windows seen in the bar: the path of a pinned app that has changed (Discord
/// update) is brought up to date
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
        // Old bar.json, without pins; pin without a Store identifier
        let pin: Pin = serde_json::from_str(r#"{ "exe": "code.exe", "name": "Code", "path": "C:/Code.exe" }"#).unwrap();
        assert!(pin.app_id.is_none());
        assert!(!serde_json::to_string(&pin).unwrap().contains("appId"));
    }
}
