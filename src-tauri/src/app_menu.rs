//! Clic droit sur une application de la vue applications (demande de Funsaiki, 17/10/2026) :
//! Exécuter en administrateur, Emplacement du fichier, Épingler à la pilule (sans l'ouvrir
//! d'abord) et, pour les applications du menu Démarrer, Masquer. Le front n'envoie que l'id de
//! la tuile ; Rust retrouve sa cible (config.rs) et revérifie ce qu'elle permet avant d'agir.
//!
//! Épinglables : un programme (.exe) lancé sans arguments. Pas une app du Store (le programme de
//! ses fenêtres ne se devine pas : Spotify se lance par SpotifyMigrator.exe), ni un raccourci
//! avec arguments (le lanceur de Riot ouvre League of Legends ou Valorant) : la pilule ne
//! saurait pas les relancer. Celles-là s'épinglent depuis leur tuile de fenêtre dans la pilule.

use std::path::Path;

use serde::Serialize;
use tauri::AppHandle;

use crate::bar_pins::{self, Pin};
use crate::config::{self, MenuItem};
use crate::i18n::tr;
use crate::{bar, shell, start_menu, tray_apps, usage};

/// Entrées du menu, en plus d'Ouvrir
#[derive(Serialize)]
pub struct AppMenu {
    admin: bool,
    location: bool,
    /// Épinglée ou non ; absent : barre pastel coupée, ou rien à épingler
    pinned: Option<bool>,
    /// Masquée ou non ; absent : pas une application du menu Démarrer
    hidden: Option<bool>,
}

/// Ce que lance une tuile
struct Program {
    /// Fichier existant : programme, script ou document
    file: Option<String>,
    /// « Exécuter en tant qu'administrateur » proposé par Windows
    admin: bool,
    /// Pour la pilule : programme lancé sans arguments
    pin: Option<Pin>,
}

impl Program {
    /// Avec COM (propriétés des raccourcis du menu Démarrer : groupe `auto: "start"` et tuiles
    /// `app`)
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
            // « wt.exe » sans chemin aussi : Windows le trouve
            admin: is_program(&path),
            pin: file.as_deref().filter(|_| item.args.is_empty()).and_then(pin_of),
            file,
        }
    }

    fn none() -> Program {
        Program { file: None, admin: false, pin: None }
    }
}

/// Ce que Windows exécute en administrateur
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

/// Épinglée, à épingler, ou rien : barre pastel coupée, rien à épingler, ou un autre programme
/// du même nom déjà épinglé (« launcher.exe » de HoYoPlay et de Wuthering Waves : la pilule
/// reconnaît les fenêtres au nom du programme, elle les confondrait)
fn pin_state(app: &AppHandle, pin: Option<&Pin>) -> Option<bool> {
    let pin = pin.filter(|_| bar::is_active(app))?;
    match bar::pins(app).iter().find(|p| p.exe == pin.exe) {
        None => Some(false),
        Some(pinned) => (pinned.name == pin.name).then_some(true),
    }
}

/// `action` : admin, location, pin, unpin, hide, show
#[tauri::command]
pub async fn app_action(app: AppHandle, id: String, action: String) -> Result<(), String> {
    let (item, program) = program(item_of(&app, &id)?).await?;
    match action.as_str() {
        "admin" if program.admin => {
            let (target, args) = (item.target.clone(), item.args.clone());
            // Hors du fil principal : l'invite UAC attend la réponse de Funsaiki
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
        // Programme sans arguments : tout
        let notepad = Program::of(&item("%SystemRoot%/System32/notepad.exe", &[]));
        assert_eq!(notepad.file.as_deref(), Some(format!(r"{windir}\System32\notepad.exe").as_str()));
        assert!(notepad.admin);
        assert_eq!(notepad.pin.map(|p| p.exe).as_deref(), Some("notepad.exe"));
        // Avec arguments : pas épinglable
        let with_args = Program::of(&item("%SystemRoot%/System32/notepad.exe", &["a.txt"]));
        assert!(with_args.admin && with_args.file.is_some() && with_args.pin.is_none());
        // Sans chemin : Windows le trouve, mais pas d'emplacement
        let bare = Program::of(&item("wt.exe", &[]));
        assert!(bare.admin && bare.file.is_none() && bare.pin.is_none());
        // Adresse, dossier : Ouvrir seulement (le dossier, Emplacement)
        let uri = Program::of(&item("steam://open/main", &[]));
        assert!(!uri.admin && uri.file.is_none() && uri.pin.is_none());
        let folder = Program::of(&item("%USERPROFILE%", &[]));
        assert!(!folder.admin && folder.file.is_none());
    }

    /// Diagnostic sur le PC (lecture seule, rien n'est lancé) :
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
                eprintln!("{:<34} admin {:<5} emplacement {:<5} épingle {pin:?}", a.name, p.admin, p.file.is_some());
            }
            eprintln!("{} applications en {:?}", apps.len(), started.elapsed());
        });
    }
}
