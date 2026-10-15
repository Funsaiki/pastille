//! Onglet « Apps » de la vue réglages (demandé par Funsaiki) : les applications qui ont une icône
//! dans la zone de notification. Windows 11 ne donne cette liste à personne : sa barre des
//! tâches (XAML) n'est lisible par l'automatisation que sortie, et celle de Funsaiki se masque.
//! Il garde en revanche une entrée par programme qui a déjà eu une icône
//! (HKCU\Control Panel\NotifyIconSettings, lu seulement) : on garde ceux qui tournent, sauf les
//! composants de Windows et le lanceur lui-même. Le menu de chaque icône n'est pas accessible
//! non plus : le lanceur a le sien (ouvrir, emplacement du fichier, forcer à quitter).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Serialize;
use windows::core::{GUID, HSTRING, PWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::ProcessStatus::K32EnumProcesses;
use windows::Win32::System::Threading::{
    OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE,
};
use windows::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems};
use windows_registry::CURRENT_USER;

use crate::mixer::exe_path;

#[derive(Serialize)]
pub struct TrayApp {
    /// Chemin du programme : le front le renvoie pour agir, Rust vérifie qu'il est dans la liste
    key: String,
    name: String,
    icon: Option<crate::icon::Icon>,
    /// Application du Microsoft Store : lancée par son identifiant, son dossier est fermé
    packaged: bool,
}

/// Par nom
#[tauri::command]
pub async fn tray_apps() -> Vec<TrayApp> {
    crate::com::com_task(|| Ok(list())).await.unwrap_or_default()
}

/// `action` : « open », « location » ou « quit » (arrêt forcé, confirmé dans le front)
#[tauri::command]
pub async fn tray_app_action(key: String, action: String) -> Result<(), String> {
    crate::blocking(move || {
        // Seulement un programme de la liste : le front ne peut rien lancer ni arrêter d'autre
        let program = programs().into_iter().find(|p| p.path == key).ok_or("Cette application n'est plus ouverte.")?;
        match action.as_str() {
            "open" => open(&program.path),
            "location" => show_in_folder(&program.path),
            "quit" => quit(&program.path, &name_of(&program)),
            _ => Err(format!("Action inconnue : {action}.")),
        }
    })
    .await?
}

struct Program {
    path: String,
    /// Infobulle de sa première icône
    tooltip: Option<String>,
}

fn list() -> Vec<TrayApp> {
    let mut apps: Vec<TrayApp> = programs()
        .into_iter()
        .map(|p| TrayApp {
            name: name_of(&p),
            icon: crate::icon::of(&p.path),
            packaged: is_packaged(&p.path),
            key: p.path,
        })
        .collect();
    apps.sort_by_key(|app| crate::steam::normalize(&app.name));
    apps
}

/// Description du programme (« Discord »), sinon son infobulle (« Wallpaper Engine » pour
/// wallpaper32.exe, qui n'a pas de description), sinon le nom du fichier
fn name_of(p: &Program) -> String {
    crate::mixer::description(&p.path).or_else(|| p.tooltip.clone()).unwrap_or_else(|| {
        Path::new(&p.path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    })
}

fn is_packaged(path: &str) -> bool {
    path.to_lowercase().contains(r"\windowsapps\")
}

/// Programmes en cours qui ont déjà eu une icône, sans doublons
fn programs() -> Vec<Program> {
    let running: HashMap<String, String> =
        processes().into_iter().map(|(_, path)| (path.to_lowercase(), path)).collect();
    let mut seen = HashSet::new();
    registered()
        .into_iter()
        .filter_map(|(path, tooltip)| {
            let lower = path.to_lowercase();
            let path = running.get(&lower)?.clone();
            (!excluded(&lower) && seen.insert(lower)).then_some(Program { path, tooltip })
        })
        .collect()
}

/// Entrées de la zone de notification : (chemin du programme, infobulle)
fn registered() -> Vec<(String, Option<String>)> {
    let Ok(settings) = CURRENT_USER.open(r"Control Panel\NotifyIconSettings") else { return Vec::new() };
    let Ok(names) = settings.keys() else { return Vec::new() };
    names
        .filter_map(|name| {
            let entry = settings.open(&name).ok()?;
            let path = resolve(&entry.get_string("ExecutablePath").ok()?)?;
            let tooltip = entry
                .get_string("InitialTooltip")
                .ok()
                .and_then(|t| t.lines().next().map(|line| line.trim().to_string()))
                .filter(|t| !t.is_empty());
            Some((path, tooltip))
        })
        .collect()
}

/// « {GUID}\Discord\Discord.exe » : le GUID désigne un dossier connu (Program Files, System32…)
fn resolve(path: &str) -> Option<String> {
    let Some(rest) = path.strip_prefix('{') else { return Some(path.to_string()) };
    let (id, rest) = rest.split_once('}')?;
    let folder = crate::feed::known_folder(&GUID::try_from(id).ok()?)?;
    Some(format!("{}{rest}", folder.display()))
}

/// Composants de Windows (Explorateur, Sécurité Windows…) et le lanceur lui-même
fn excluded(lower: &str) -> bool {
    let windows = std::env::var("SystemRoot").unwrap_or_default().to_lowercase() + "\\";
    let own = std::env::current_exe().map(|p| p.to_string_lossy().to_lowercase()).unwrap_or_default();
    lower.starts_with(&windows) || lower == own
}

/// Processus en cours : (identifiant, chemin du programme). Ceux qu'on ne peut pas interroger
/// (protégés, d'un autre compte) sont ignorés.
fn processes() -> Vec<(u32, String)> {
    let mut pids = vec![0u32; 4096];
    let mut bytes = 0u32;
    if !unsafe { K32EnumProcesses(pids.as_mut_ptr(), (pids.len() * 4) as u32, &mut bytes) }.as_bool() {
        return Vec::new();
    }
    pids.truncate(bytes as usize / 4);
    pids.into_iter().filter_map(|pid| exe_path(pid).map(|path| (pid, path))).collect()
}

fn pids_of(path: &str) -> Vec<u32> {
    let path = path.to_lowercase();
    processes().into_iter().filter(|(_, p)| p.to_lowercase() == path).map(|(pid, _)| pid).collect()
}

/// Ramène sa fenêtre ; sinon (rangée dans la zone de notification), relance le programme, qui
/// réaffiche alors la sienne (Discord, Steam…). Une app du Store se lance par son identifiant.
fn open(path: &str) -> Result<(), String> {
    if crate::switch::focus_existing(path) {
        return Ok(());
    }
    if is_packaged(path) {
        let id = pids_of(path).into_iter().find_map(app_id).ok_or("Identifiant de l'application introuvable.")?;
        return crate::shell::open_as_is(&format!(r"shell:AppsFolder\{id}"));
    }
    crate::shell::open_as_is(path)
}

/// Identifiant d'application d'un processus du Store (« SpotifyAB.SpotifyMusic_…!Spotify »)
fn app_id(pid: u32) -> Option<String> {
    let mut buf = [0u16; 512];
    let mut len = buf.len() as u32;
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let found = GetApplicationUserModelId(process, &mut len, Some(PWSTR(buf.as_mut_ptr())));
        let _ = CloseHandle(process);
        found.ok().ok()?;
    }
    // La longueur compte le caractère nul final
    (len > 1).then(|| String::from_utf16_lossy(&buf[..len as usize - 1]))
}

/// Explorateur ouvert sur le dossier du programme, le fichier sélectionné
fn show_in_folder(path: &str) -> Result<(), String> {
    unsafe {
        // Le shell attend un appartement monothread, comme pour ShellExecuteEx (shell.rs)
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let item = ILCreateFromPathW(&HSTRING::from(path));
        let result = if item.is_null() {
            Err(format!("{path} : introuvable."))
        } else {
            let shown = SHOpenFolderAndSelectItems(item, None, 0);
            ILFree(Some(item));
            shown.map_err(|e| format!("{path} : {}", e.message().trim_end()))
        };
        if com.is_ok() {
            CoUninitialize();
        }
        result
    }
}

/// Arrêt forcé de tous les processus du programme : une application de la zone de notification
/// ne se ferme pas avec sa fenêtre, elle y retourne. Elle n'a pas le temps de sauvegarder (le
/// front demande confirmation). Attend la fin de chacun, 2 s au plus.
fn quit(path: &str, name: &str) -> Result<(), String> {
    let pids = pids_of(path);
    if pids.is_empty() {
        return Err(format!("{name} n'est plus ouvert."));
    }
    eprintln!("[lanceur] forcer à quitter {path}");
    for pid in pids {
        // Déjà terminé (enfant d'un processus arrêté juste avant) ou refusé : vu à la fin
        let Ok(process) = (unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid) }) else {
            continue;
        };
        unsafe {
            if TerminateProcess(process, 1).is_ok() {
                WaitForSingleObject(process, 2000);
            }
            let _ = CloseHandle(process);
        }
    }
    if pids_of(path).is_empty() {
        Ok(())
    } else {
        Err(format!("{name} n'a pas pu être arrêté : il tourne peut-être en administrateur."))
    }
}

// Ne jamais ouvrir ni arrêter une application de Funsaiki dans un test.
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    #[test]
    fn known_folders_are_resolved() {
        let windir = std::env::var("SystemRoot").unwrap();
        let path = resolve(r"{F38BF404-1D43-42F2-9305-67DE0B28FC23}\explorer.exe").unwrap();
        assert!(path.eq_ignore_ascii_case(&format!(r"{windir}\explorer.exe")), "{path}");
        assert_eq!(resolve(r"C:\Jeux\jeu.exe").as_deref(), Some(r"C:\Jeux\jeu.exe"));
        assert!(resolve(r"{pas-un-guid}\x.exe").is_none());
    }

    #[test]
    fn windows_and_launcher_are_excluded() {
        let windir = std::env::var("SystemRoot").unwrap();
        assert!(excluded(&format!(r"{windir}\explorer.exe").to_lowercase()));
        assert!(excluded(&std::env::current_exe().unwrap().to_string_lossy().to_lowercase()));
        assert!(!excluded(r"c:\users\johnny\appdata\local\discord\app-1.0\discord.exe"));
    }

    /// Sur un programme lancé par le test : une copie de PING.EXE au nom unique, deux fois
    #[test]
    fn quit_stops_every_process_of_the_program() {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let dir = std::env::temp_dir().join(format!("lanceur-pastel-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("lanceur-test-ping.exe");
        std::fs::copy(format!(r"{}\System32\PING.EXE", std::env::var("SystemRoot").unwrap()), &exe).unwrap();
        let mut children: Vec<_> = (0..2)
            .map(|_| {
                Command::new(&exe)
                    .args(["-n", "60", "127.0.0.1"])
                    .stdout(Stdio::null())
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn()
                    .unwrap()
            })
            .collect();
        let path = exe_path(children[0].id()).unwrap();
        assert_eq!(pids_of(&path).len(), 2);
        quit(&path, "Test").unwrap();
        for child in &mut children {
            assert!(child.try_wait().unwrap().is_some(), "processus arrêté");
        }
        assert_eq!(quit(&path, "Test"), Err("Test n'est plus ouvert.".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Diagnostic, lecture seule : cargo test real_tray_apps -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_tray_apps() {
        crate::com::with_com(|| {
            let start = std::time::Instant::now();
            for app in list() {
                eprintln!("{}  ({})  icône : {}  Store : {}", app.name, app.key, app.icon.is_some(), app.packaged);
            }
            eprintln!("durée : {:?}", start.elapsed());
            let again = std::time::Instant::now();
            list();
            eprintln!("deuxième lecture (icônes en mémoire) : {:?}", again.elapsed());
        });
    }
}
