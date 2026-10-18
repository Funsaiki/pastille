//! Onglet « Apps » de la vue réglages (demandé par Funsaiki) : les applications qui ont une icône
//! dans la zone de notification. Windows 11 ne donne cette liste à personne : sa barre des
//! tâches (XAML) n'est lisible par l'automatisation que sortie, et celle de Funsaiki se masque.
//! Il garde en revanche une entrée par programme qui a déjà eu une icône
//! (HKCU\Control Panel\NotifyIconSettings, lu seulement) : on garde ceux qui tournent, sauf les
//! composants de Windows et le lanceur lui-même. Le menu de chaque icône n'est pas accessible
//! non plus : le lanceur a le sien (ouvrir, emplacement du fichier, forcer à quitter).
//! Barre pastel cochée, Pastille tient elle-même la zone de notification (tray_host.rs) : la
//! liste est celle des icônes, et leurs clics vont à leur application (son menu compris).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use windows::core::{GUID, HSTRING, PWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
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
    /// Chemin du programme, ou « #n » pour une icône de la zone de notification de Pastille : le
    /// front le renvoie pour agir, Rust vérifie qu'il est dans la liste
    key: String,
    name: String,
    icon: Option<crate::icon::Icon>,
    /// Application du Microsoft Store : lancée par son identifiant, son dossier est fermé
    packaged: bool,
    /// Ses clics vont à l'application, comme sur son icône (tray_host.rs)
    clicks: bool,
    /// Infobulle actuelle de son icône (EarTrumpet y met le volume)
    tip: Option<String>,
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
        let program = program_of(&key)?;
        match action.as_str() {
            // Une icône qui a ses clics : comme un clic dessus
            "open" if key.starts_with('#') && crate::tray_host::click(&key, "left")? => Ok(()),
            "open" => open(&program.path),
            "location" => show_in_folder(&program.path),
            "quit" => quit(&program.path, &name_of(&program)),
            _ => Err(format!("Action inconnue : {action}.")),
        }
    })
    .await?
}

/// Clic sur une icône de la zone de notification de Pastille, envoyé à son application :
/// `kind` left, double (second clic d'un double clic), right, middle ou key (Entrée)
#[tauri::command]
pub async fn tray_click(key: String, kind: String) -> Result<(), String> {
    crate::blocking(move || {
        let program = program_of(&key)?;
        match crate::tray_host::click(&key, &kind)? {
            // Pas de message de rappel : ouverte comme avant
            false if kind == "left" || kind == "key" => open(&program.path),
            _ => Ok(()),
        }
    })
    .await?
}

/// Seulement un programme de la liste : le front ne peut rien lancer ni arrêter d'autre
fn program_of(key: &str) -> Result<Program, String> {
    let gone = || "Cette application n'est plus ouverte.".to_string();
    if key.starts_with('#') {
        let path = crate::tray_host::pid_of(key).and_then(exe_path).ok_or_else(gone)?;
        return match Excluded::new().contains(&path.to_lowercase()) {
            true => Err(gone()),
            false => Ok(Program { path, tooltip: None }),
        };
    }
    programs().into_iter().find(|p| p.path == key).ok_or_else(gone)
}

struct Program {
    path: String,
    /// Infobulle de sa première icône
    tooltip: Option<String>,
}

fn list() -> Vec<TrayApp> {
    let mut apps: Vec<TrayApp> = match crate::tray_host::icons() {
        Some(icons) => live(icons),
        None => programs()
            .into_iter()
            .map(|p| TrayApp {
                name: name_of(&p),
                icon: crate::icon::of(&p.path),
                packaged: is_packaged(&p.path),
                key: p.path,
                clicks: false,
                tip: None,
            })
            .collect(),
    };
    apps.sort_by_key(|app| crate::steam::normalize(&app.name));
    apps
}

/// Icônes de la zone de notification de Pastille, sans celles de Windows ni la sienne ; une
/// tuile par icône (une application peut en avoir deux), l'icône de son programme
fn live(icons: Vec<crate::tray_host::Live>) -> Vec<TrayApp> {
    let excluded = Excluded::new();
    icons
        .into_iter()
        .filter_map(|icon| {
            let path = exe_path(icon.pid)?;
            if excluded.contains(&path.to_lowercase()) {
                return None;
            }
            let tip = icon.tip.trim().to_string();
            let tooltip = tip.lines().next().map(|line| line.trim().to_string()).filter(|t| !t.is_empty());
            let program = Program { path, tooltip };
            Some(TrayApp {
                name: name_of(&program),
                icon: crate::icon::of(&program.path),
                packaged: is_packaged(&program.path),
                key: icon.key,
                clicks: icon.clicks,
                tip: (!tip.is_empty()).then_some(tip),
            })
        })
        .collect()
}

/// Processus des programmes qui ont déjà eu une icône, sauf Windows et le lanceur : ceux à qui
/// tray_host.rs redemande leurs icônes
pub(crate) fn tray_pids() -> HashSet<u32> {
    let excluded = Excluded::new();
    let registered: HashSet<String> = registered().into_iter().map(|(path, _)| path.to_lowercase()).collect();
    processes()
        .into_iter()
        .filter(|(_, path)| {
            let lower = path.to_lowercase();
            registered.contains(&lower) && !excluded.contains(&lower)
        })
        .map(|(pid, _)| pid)
        .collect()
}

/// Noms déjà lus, par chemin : la liste est relue toutes les 5 s
static NAMES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Description du programme (« Discord »), sinon son infobulle (« Wallpaper Engine » pour
/// wallpaper32.exe, qui n'a pas de description), sinon le nom du fichier
fn name_of(p: &Program) -> String {
    if let Some(name) = NAMES.lock().unwrap().get_or_insert_with(HashMap::new).get(&p.path) {
        return name.clone();
    }
    let name = crate::mixer::description(&p.path).or_else(|| p.tooltip.clone()).unwrap_or_else(|| {
        Path::new(&p.path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    });
    NAMES.lock().unwrap().get_or_insert_with(HashMap::new).insert(p.path.clone(), name.clone());
    name
}

pub(crate) fn is_packaged(path: &str) -> bool {
    path.to_lowercase().contains(r"\windowsapps\")
}

/// Programmes en cours qui ont déjà eu une icône, sans doublons
fn programs() -> Vec<Program> {
    let running: HashMap<String, String> =
        processes().into_iter().map(|(_, path)| (path.to_lowercase(), path)).collect();
    let excluded = Excluded::new();
    let mut seen = HashSet::new();
    registered()
        .into_iter()
        .filter_map(|(path, tooltip)| {
            let lower = path.to_lowercase();
            let path = running.get(&lower)?.clone();
            (!excluded.contains(&lower) && seen.insert(lower)).then_some(Program { path, tooltip })
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
struct Excluded {
    windows: String,
    own: String,
}

impl Excluded {
    fn new() -> Self {
        Excluded {
            windows: std::env::var("SystemRoot").unwrap_or_default().to_lowercase() + "\\",
            own: std::env::current_exe().map(|p| p.to_string_lossy().to_lowercase()).unwrap_or_default(),
        }
    }

    /// `lower` : chemin en minuscules
    fn contains(&self, lower: &str) -> bool {
        lower.starts_with(&self.windows) || lower == self.own
    }
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
pub(crate) fn app_id(pid: u32) -> Option<String> {
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
pub(crate) fn show_in_folder(path: &str) -> Result<(), String> {
    unsafe {
        // Le shell attend un appartement monothread, comme pour ShellExecuteEx (shell.rs)
        let com = CoInitializeEx(None, COINIT(COINIT_APARTMENTTHREADED.0 | COINIT_DISABLE_OLE1DDE.0));
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
/// front demande confirmation). Les arrête tous, puis attend leur fin, 2 s au plus en tout.
fn quit(path: &str, name: &str) -> Result<(), String> {
    let pids = pids_of(path);
    if pids.is_empty() {
        return Err(format!("{name} n'est plus ouvert."));
    }
    eprintln!("[lanceur] forcer à quitter {path}");
    let lower = path.to_lowercase();
    let access = PROCESS_TERMINATE | PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION;
    let mut stopped = Vec::new();
    for pid in pids {
        // Déjà terminé (enfant d'un processus arrêté juste avant) ou refusé : vu à la fin
        let Ok(process) = (unsafe { OpenProcess(access, false, pid) }) else { continue };
        // Programme revérifié sur ce handle : un identifiant réattribué entre-temps à un autre
        // programme n'est pas touché
        let same = crate::mixer::process_path(process).is_some_and(|p| p.to_lowercase() == lower);
        if same && unsafe { TerminateProcess(process, 1) }.is_ok() {
            stopped.push(process);
        } else {
            let _ = unsafe { CloseHandle(process) };
        }
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    for process in stopped {
        let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u32;
        unsafe {
            WaitForSingleObject(process, left);
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
        let excluded = Excluded::new();
        assert!(excluded.contains(&format!(r"{windir}\explorer.exe").to_lowercase()));
        assert!(excluded.contains(&std::env::current_exe().unwrap().to_string_lossy().to_lowercase()));
        assert!(!excluded.contains(r"c:\users\johnny\appdata\local\discord\app-1.0\discord.exe"));
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
