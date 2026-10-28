//! "Apps" tab of the quick settings view (requested by Funsaiki): the apps that have an icon
//! in the notification area. Windows 11 gives this list to no one: its taskbar (XAML) can only
//! be read by UI Automation while it is out, and Funsaiki's hides itself.
//! It does keep one entry per program that has ever had an icon
//! (HKCU\Control Panel\NotifyIconSettings, only read): we keep those that are running, except
//! Windows components and the launcher itself. Each icon's menu isn't reachable either:
//! the launcher has its own (Open, Open file location, Force quit).
//! With the pastel bar on, Pastille holds the notification area itself (tray_host.rs): the
//! list starts with its icons, whose clicks go to their app (its menu included).

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

use crate::i18n::tr;
use crate::mixer::exe_path;

#[derive(Serialize)]
pub struct TrayApp {
    /// Program path, or "#n" for an icon of Pastille's notification area: the
    /// front sends it back to act, Rust checks that it is in the list
    key: String,
    name: String,
    icon: Option<crate::icon::Icon>,
    /// Microsoft Store app: launched by its identifier, its folder is off limits
    packaged: bool,
    /// Its clicks go to the app, as on its icon (tray_host.rs)
    clicks: bool,
    /// Current tooltip of its icon (EarTrumpet puts the volume in it)
    tip: Option<String>,
}

/// By name
#[tauri::command]
pub async fn tray_apps() -> Vec<TrayApp> {
    crate::com::com_task(|| Ok(list())).await.unwrap_or_default()
}

/// `action`: "open", "location" or "quit" (forced stop, confirmed in the front)
#[tauri::command]
pub async fn tray_app_action(key: String, action: String) -> Result<(), String> {
    crate::blocking(move || {
        let program = program_of(&key)?;
        match action.as_str() {
            // An icon that has its clicks: like a click on it
            "open" if key.starts_with('#') && !menu_on_click(&program.path) && crate::tray_host::click(&key, "left")? => Ok(()),
            "open" => open(&program.path),
            "location" => show_in_folder(&program.path),
            "quit" => quit(&program.path, &name_of(&program)),
            _ => Err(tr(format!("Action inconnue : {action}."), format!("Unknown action: {action}."))),
        }
    })
    .await?
}

/// Click on an icon of Pastille's notification area, sent to its app:
/// `kind` left, double (second click of a double click), right, middle or key (Enter)
#[tauri::command]
pub async fn tray_click(key: String, kind: String) -> Result<(), String> {
    crate::blocking(move || {
        let program = program_of(&key)?;
        if (kind == "left" || kind == "key") && menu_on_click(&program.path) {
            return open(&program.path);
        }
        match crate::tray_host::click(&key, &kind)? {
            // No callback message: opened as before
            false if kind == "left" || kind == "key" => open(&program.path),
            _ => Ok(()),
        }
    })
    .await?
}

/// Steam shows its menu on a left click on its icon, in the Windows taskbar too (its window
/// on a double click): opened instead, like a tile without clicks (requested by Funsaiki)
fn menu_on_click(path: &str) -> bool {
    Path::new(path).file_name().is_some_and(|name| name.eq_ignore_ascii_case("steam.exe"))
}

/// Only a program of the list: the front can't launch or stop anything else
fn program_of(key: &str) -> Result<Program, String> {
    let gone = || tr("Cette application n'est plus ouverte.", "This app is no longer open.").to_string();
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
    /// Tooltip of its first icon
    tooltip: Option<String>,
}

/// Pastel bar on: the icons Pastille received, then, as before, the programs that
/// run without having given it one: no icon right now (Spotify), or that don't answer
/// "TaskbarCreated" (Riot, Java: checked) and only give it again when they change it
fn list() -> Vec<TrayApp> {
    let mut shown = HashSet::new();
    let mut apps = crate::tray_host::icons().map(|icons| live(icons, &mut shown)).unwrap_or_default();
    apps.extend(programs().into_iter().filter(|p| !shown.contains(&p.path.to_lowercase())).map(|p| TrayApp {
        name: name_of(&p),
        icon: crate::icon::of(&p.path),
        packaged: is_packaged(&p.path),
        key: p.path,
        clicks: false,
        tip: None,
    }));
    apps.sort_by_key(|app| crate::steam::normalize(&app.name));
    apps
}

/// Icons of Pastille's notification area, without Windows' ones or its own; one
/// tile per icon (an app can have two), with its program's icon. `shown`:
/// their programs, lowercase
fn live(icons: Vec<crate::tray_host::Live>, shown: &mut HashSet<String>) -> Vec<TrayApp> {
    let excluded = Excluded::new();
    icons
        .into_iter()
        .filter_map(|icon| {
            let path = exe_path(icon.pid)?;
            if excluded.contains(&path.to_lowercase()) {
                return None;
            }
            shown.insert(path.to_lowercase());
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

/// Processes of the programs that have ever had an icon, except Windows and the launcher: those
/// tray_host.rs asks for their icons again
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

/// Names already read, by path: the list is read again every 5 s
static NAMES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Program description ("Discord"), else its tooltip ("Wallpaper Engine" for
/// wallpaper32.exe, which has no description), else the file name
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

/// Running programs that have ever had an icon, without duplicates
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

/// Notification area entries: (program path, tooltip)
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

/// "{GUID}\Discord\Discord.exe": the GUID stands for a known folder (Program Files, System32…)
fn resolve(path: &str) -> Option<String> {
    let Some(rest) = path.strip_prefix('{') else { return Some(path.to_string()) };
    let (id, rest) = rest.split_once('}')?;
    let folder = crate::feed::known_folder(&GUID::try_from(id).ok()?)?;
    Some(format!("{}{rest}", folder.display()))
}

/// Windows components (Explorer, Windows Security…) and the launcher itself
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

    /// `lower`: lowercase path
    fn contains(&self, lower: &str) -> bool {
        lower.starts_with(&self.windows) || lower == self.own
    }
}

/// Running processes: (identifier, program path). Those that can't be queried
/// (protected, from another account) are ignored.
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

/// Brings back its window; otherwise (tucked away in the notification area), relaunches the
/// program, which then shows its own again (Discord, Steam…). A Store app is launched by its identifier.
fn open(path: &str) -> Result<(), String> {
    if crate::switch::focus_existing(path) {
        return Ok(());
    }
    if is_packaged(path) {
        let id = pids_of(path).into_iter().find_map(app_id).ok_or(tr("Identifiant de l'application introuvable.", "The app's identifier was not found."))?;
        return crate::shell::open_as_is(&format!(r"shell:AppsFolder\{id}"));
    }
    crate::shell::open_as_is(path)
}

/// App identifier of a Store process ("SpotifyAB.SpotifyMusic_…!Spotify")
pub(crate) fn app_id(pid: u32) -> Option<String> {
    let mut buf = [0u16; 512];
    let mut len = buf.len() as u32;
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let found = GetApplicationUserModelId(process, &mut len, Some(PWSTR(buf.as_mut_ptr())));
        let _ = CloseHandle(process);
        found.ok().ok()?;
    }
    // The length counts the final null character
    (len > 1).then(|| String::from_utf16_lossy(&buf[..len as usize - 1]))
}

/// Explorer opened on the program's folder, the file selected
pub(crate) fn show_in_folder(path: &str) -> Result<(), String> {
    unsafe {
        // The shell expects a single-threaded apartment, as for ShellExecuteEx (shell.rs)
        let com = CoInitializeEx(None, COINIT(COINIT_APARTMENTTHREADED.0 | COINIT_DISABLE_OLE1DDE.0));
        let item = ILCreateFromPathW(&HSTRING::from(path));
        let result = if item.is_null() {
            Err(tr(format!("{path} : introuvable."), format!("{path}: not found.")))
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

/// Forced stop of all the program's processes: a notification area app
/// doesn't close with its window, it goes back there. It has no time to save (the
/// front asks for confirmation). Stops them all, then waits for them to end, 2 s at most in all.
fn quit(path: &str, name: &str) -> Result<(), String> {
    let pids = pids_of(path);
    if pids.is_empty() {
        return Err(tr(format!("{name} n'est plus ouvert."), format!("{name} is no longer open.")));
    }
    eprintln!("[pastille] force quit {path}");
    let lower = path.to_lowercase();
    let access = PROCESS_TERMINATE | PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION;
    let mut stopped = Vec::new();
    for pid in pids {
        // Already ended (child of a process stopped just before) or refused: seen at the end
        let Ok(process) = (unsafe { OpenProcess(access, false, pid) }) else { continue };
        // Program checked again on this handle: an identifier reassigned meanwhile to another
        // program is not touched
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
        Err(tr(
            format!("{name} n'a pas pu être arrêté : il tourne peut-être en administrateur."),
            format!("{name} couldn't be stopped: it may be running as administrator."),
        ))
    }
}

// Never open or stop one of Funsaiki's apps in a test.
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
        assert!(resolve(r"{not-a-guid}\x.exe").is_none());
    }

    #[test]
    fn windows_and_launcher_are_excluded() {
        let windir = std::env::var("SystemRoot").unwrap();
        let excluded = Excluded::new();
        assert!(excluded.contains(&format!(r"{windir}\explorer.exe").to_lowercase()));
        assert!(excluded.contains(&std::env::current_exe().unwrap().to_string_lossy().to_lowercase()));
        assert!(!excluded.contains(r"c:\users\johnny\appdata\local\discord\app-1.0\discord.exe"));
    }

    #[test]
    fn steam_opens_on_click() {
        assert!(menu_on_click(r"C:\Program Files (x86)\Steam\steam.exe"));
        assert!(menu_on_click(r"D:\Steam\STEAM.EXE"));
        assert!(!menu_on_click(r"C:\Program Files (x86)\Steam\bin\cef\cef.win64\steamwebhelper.exe"));
        assert!(!menu_on_click(r"C:\Program Files\EarTrumpet\EarTrumpet.exe"));
    }

    /// On a program launched by the test: a copy of PING.EXE with a unique name, twice
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
            assert!(child.try_wait().unwrap().is_some(), "process stopped");
        }
        assert_eq!(quit(&path, "Test"), Err("Test n'est plus ouvert.".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Diagnostic, read only: cargo test real_tray_apps -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_tray_apps() {
        crate::com::with_com(|| {
            let start = std::time::Instant::now();
            for app in list() {
                eprintln!("{}  ({})  icon: {}  Store: {}", app.name, app.key, app.icon.is_some(), app.packaged);
            }
            eprintln!("time: {:?}", start.elapsed());
            let again = std::time::Instant::now();
            list();
            eprintln!("second read (icons in memory): {:?}", again.elapsed());
        });
    }
}
