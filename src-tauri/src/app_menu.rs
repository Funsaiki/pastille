//! Right-click on an app in the apps view (requested by Funsaiki, 2026-10-17):
//! Run as admin, Open file location, Pin to the pill (without opening it
//! first) and, for Start menu apps, Hide. The front sends only the tile's id;
//! Rust finds its target (config.rs) and checks again what it allows before acting.
//!
//! Pinnable: a program (.exe) launched without arguments. Not a Store app (the program of
//! its windows can't be guessed: Spotify launches through SpotifyMigrator.exe), nor a shortcut
//! with arguments (the Riot launcher opens League of Legends or Valorant): the pill wouldn't
//! know how to relaunch them. Those get pinned from their window tile in the pill.

use std::path::Path;

use serde::Serialize;
use tauri::AppHandle;

use crate::bar_pins::{self, Pin};
use crate::config::{self, MenuItem};
use crate::i18n::tr;
use crate::{bar, shell, start_menu, tray_apps, usage};

/// Menu entries, besides Open
#[derive(Serialize)]
pub struct AppMenu {
    admin: bool,
    location: bool,
    /// Pinned or not; missing: pastel bar off, or nothing to pin
    pinned: Option<bool>,
    /// Hidden or not; missing: not a Start menu app
    hidden: Option<bool>,
}

/// What a tile launches
struct Program {
    /// Existing file: program, script or document
    file: Option<String>,
    /// "Run as administrator" offered by Windows
    admin: bool,
    /// For the pill: program launched without arguments
    pin: Option<Pin>,
}

impl Program {
    /// With COM (properties of Start menu shortcuts: `auto: "start"` group and `app`
    /// tiles)
    fn of(item: &MenuItem) -> Program {
        if item.target.starts_with(start_menu::PREFIX) {
            let Some(shortcut) = start_menu::shortcut(&item.target) else { return Program::none() };
            let file = shortcut.target.filter(|t| Path::new(t).is_file());
            return Program {
                admin: shortcut.packaged_desktop || file.as_deref().is_some_and(is_program),
                pin: file.as_deref().filter(|_| !shortcut.args).and_then(pin_of),
                file,
            };
        }
        let target = shell::expand_env(&item.target);
        if shell::is_uri(&target) {
            return Program::none();
        }
        let path = target.replace('/', "\\");
        let file = Path::new(&path).is_file().then_some(path.clone());
        Program {
            // "wt.exe" without a path too: Windows finds it
            admin: is_program(&path),
            pin: file.as_deref().filter(|_| item.args.is_empty()).and_then(pin_of),
            file,
        }
    }

    fn none() -> Program {
        Program { file: None, admin: false, pin: None }
    }
}

/// What Windows runs as administrator
fn is_program(path: &str) -> bool {
    let extension = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(extension.as_str(), "exe" | "bat" | "cmd" | "msc")
}

fn pin_of(path: &str) -> Option<Pin> {
    path.to_ascii_lowercase().ends_with(".exe").then(|| bar_pins::program_pin(path, None)).flatten()
}

fn item_of(app: &AppHandle, id: &str) -> Result<MenuItem, String> {
    config::menu_item(app, id).ok_or_else(|| {
        tr(format!("Élément {id} introuvable, la config a peut-être changé."), format!("Item {id} not found, the config may have changed."))
    })
}

async fn program(item: MenuItem) -> Result<(MenuItem, Program), String> {
    crate::com::com_task(move || {
        let program = Program::of(&item);
        Ok((item, program))
    })
    .await
}

#[tauri::command]
pub async fn app_menu(app: AppHandle, id: String) -> Result<AppMenu, String> {
    let (item, program) = program(item_of(&app, &id)?).await?;
    Ok(AppMenu {
        admin: program.admin,
        location: program.file.is_some(),
        pinned: pin_state(&app, program.pin.as_ref()),
        hidden: item.hideable.then_some(item.hidden),
    })
}

/// Pinned, to pin, or nothing: pastel bar off, nothing to pin, or another program
/// of the same name already pinned ("launcher.exe" of HoYoPlay and Wuthering Waves: the pill
/// recognizes windows by program name, it would mix them up)
fn pin_state(app: &AppHandle, pin: Option<&Pin>) -> Option<bool> {
    let pin = pin.filter(|_| bar::is_active(app))?;
    match bar::pins(app).iter().find(|p| p.exe == pin.exe) {
        None => Some(false),
        Some(pinned) => (pinned.name == pin.name).then_some(true),
    }
}

/// `action`: admin, location, pin, unpin, hide, show
#[tauri::command]
pub async fn app_action(app: AppHandle, id: String, action: String) -> Result<(), String> {
    let (item, program) = program(item_of(&app, &id)?).await?;
    match action.as_str() {
        "admin" if program.admin => {
            let (target, args) = (item.target.clone(), item.args.clone());
            // Off the main thread: the UAC prompt waits for Funsaiki's answer
            crate::blocking(move || shell::open_as_admin(&target, &args)).await??;
            usage::record(&app, usage::key(&item.target, &item.args));
            Ok(())
        }
        "location" => {
            let file = program.file.ok_or(tr("Fichier introuvable.", "File not found."))?;
            crate::blocking(move || tray_apps::show_in_folder(&file)).await?
        }
        "pin" | "unpin" => {
            let pin = program.pin.as_ref().filter(|_| pin_state(&app, program.pin.as_ref()) == Some(action == "unpin"));
            let pin = pin.ok_or(tr("La pilule ne peut pas faire cela pour cette application.", "The pill can't do that for this app."))?;
            if action == "pin" {
                bar_pins::add(&app, pin.clone());
            } else {
                bar_pins::remove(&app, &pin.exe);
            }
            Ok(())
        }
        "hide" | "show" if item.hideable => config::set_start_hidden(&app, &item.target, action == "hide"),
        _ => Err(tr(format!("Action « {action} » impossible pour cette application."), format!("Action \"{action}\" not possible for this app."))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(target: &str, args: &[&str]) -> MenuItem {
        MenuItem { target: target.into(), args: args.iter().map(|a| a.to_string()).collect(), hideable: false, hidden: false }
    }

    #[test]
    fn what_a_tile_allows() {
        let windir = std::env::var("SystemRoot").unwrap();
        // Program without arguments: everything
        let notepad = Program::of(&item("%SystemRoot%/System32/notepad.exe", &[]));
        assert_eq!(notepad.file.as_deref(), Some(format!(r"{windir}\System32\notepad.exe").as_str()));
        assert!(notepad.admin);
        assert_eq!(notepad.pin.map(|p| p.exe).as_deref(), Some("notepad.exe"));
        // With arguments: not pinnable
        let with_args = Program::of(&item("%SystemRoot%/System32/notepad.exe", &["a.txt"]));
        assert!(with_args.admin && with_args.file.is_some() && with_args.pin.is_none());
        // Without a path: Windows finds it, but no file location
        let bare = Program::of(&item("wt.exe", &[]));
        assert!(bare.admin && bare.file.is_none() && bare.pin.is_none());
        // Address, folder: Open only (no Open file location for a folder either)
        let uri = Program::of(&item("steam://open/main", &[]));
        assert!(!uri.admin && uri.file.is_none() && uri.pin.is_none());
        let folder = Program::of(&item("%USERPROFILE%", &[]));
        assert!(!folder.admin && folder.file.is_none());
    }

    /// Diagnostic on the PC (read-only, nothing is launched):
    /// cargo test real_start_menu_programs -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_start_menu_programs() {
        crate::com::with_com(|| {
            let started = std::time::Instant::now();
            let apps = start_menu::installed_apps();
            for a in &apps {
                let p = Program::of(&MenuItem { target: a.target.clone(), args: Vec::new(), hideable: true, hidden: false });
                let pin = p.pin.map(|p| format!("{} ({})", p.exe, p.name));
                eprintln!("{:<34} admin {:<5} location {:<5} pin {pin:?}", a.name, p.admin, p.file.is_some());
            }
            eprintln!("{} apps in {:?}", apps.len(), started.elapsed());
        });
    }
}
