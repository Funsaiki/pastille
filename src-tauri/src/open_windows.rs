//! Open windows offered by the search (requested by Funsaiki): typing "disc" also offers
//! Discord's window, like an Alt+Tab with search. The taskbar windows
//! (switch.rs), except the launcher's.

use serde::Serialize;
use windows::core::GUID;
use windows::Win32::Foundation::{HWND, PROPERTYKEY};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};

use crate::i18n::tr;
use crate::switch;

#[derive(Serialize)]
pub struct OpenWindow {
    /// Window handle, sent back to bring it up (Rust checks it is still there)
    id: String,
    title: String,
    /// Program ("Discord", "Brave Browser")
    app: String,
    icon: Option<crate::icon::Icon>,
}

/// The most recently active first
#[tauri::command]
pub async fn open_windows() -> Vec<OpenWindow> {
    crate::com::com_task(|| Ok(list())).await.unwrap_or_default()
}

/// Brings the window to the foreground (restored if it was minimized)
#[tauri::command]
pub async fn focus_window(id: String) -> Result<(), String> {
    crate::blocking(move || {
        let hwnd = app_window(&id)?;
        if switch::bring(hwnd) { Ok(()) } else { Err(tr("Windows a refusé de passer à cette fenêtre.", "Windows refused to switch to this window.").into()) }
    })
    .await?
}

/// Window sent by the front (search, pastel bar): still a taskbar window,
/// and not the launcher. Nothing else is touched.
pub(crate) fn app_window(id: &str) -> Result<HWND, String> {
    let handle: isize = id.parse().map_err(|_| tr(format!("Fenêtre {id} inconnue."), format!("Unknown window {id}.")))?;
    let hwnd = HWND(handle as *mut _);
    if !switch::is_app_window(hwnd) || switch::process_of(hwnd).1 == std::process::id() {
        return Err(tr("Cette fenêtre n'est plus ouverte.", "This window is no longer open.").to_string());
    }
    Ok(hwnd)
}

/// The window's `PKEY_AppUserModel_*` properties
const APP_MODEL: GUID = GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3);
const APP_ID: u32 = 5;
const RELAUNCH_COMMAND: u32 = 2;
const RELAUNCH_NAME: u32 = 4;

/// App identifier given to the window (`PKEY_AppUserModel_ID`): the one of the Store apps
/// hosted by ApplicationFrameHost (Settings, Calculator). With COM.
pub(crate) fn window_app_id(hwnd: HWND) -> Option<String> {
    app_model(hwnd, APP_ID)
}

/// How the Windows taskbar relaunches the window, when its app says so
/// (`PKEY_AppUserModel_RelaunchCommand`): Steam's windows belong to steamwebhelper.exe,
/// which doesn't start alone; they give "steam.exe" and the name "Steam". With COM.
pub(crate) struct Relaunch {
    pub program: String,
    /// The command's parameters, as written
    pub params: String,
    pub name: Option<String>,
}

pub(crate) fn window_relaunch(hwnd: HWND) -> Option<Relaunch> {
    let command = app_model(hwnd, RELAUNCH_COMMAND)?;
    let (program, params) = split_command(&command, |path| std::path::Path::new(path).is_file())?;
    // "@file.dll,-12": a resource, the program's own name will do
    let name = app_model(hwnd, RELAUNCH_NAME).filter(|name| !name.starts_with('@'));
    Some(Relaunch { program, params, name })
}

/// Program and parameters of a command line: the program in quotes, otherwise like Windows
/// (CreateProcess) the shortest start up to a space that is an existing file: Steam writes
/// C:\Program Files (x86)\Steam\steam.exe without quotes. Nothing found: the whole line.
fn split_command(command: &str, exists: impl Fn(&str) -> bool) -> Option<(String, String)> {
    let command = command.trim();
    let (program, rest) = match command.strip_prefix('"') {
        Some(quoted) => quoted.split_once('"')?,
        None => command
            .match_indices(' ')
            .map(|(i, _)| (&command[..i], &command[i..]))
            .find(|(program, _)| exists(program))
            .unwrap_or((command, "")),
    };
    (!program.is_empty()).then(|| (program.to_string(), rest.trim().to_string()))
}

fn app_model(hwnd: HWND, pid: u32) -> Option<String> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd).ok()?;
        let mut value = store.GetValue(&PROPERTYKEY { fmtid: APP_MODEL, pid }).ok()?;
        let text = PropVariantToStringAlloc(&value).ok();
        let _ = PropVariantClear(&mut value);
        let text = text?;
        let id = text.to_string().ok();
        CoTaskMemFree(Some(text.0 as *const _));
        id.filter(|id| !id.is_empty())
    }
}

fn list() -> Vec<OpenWindow> {
    let own = std::process::id();
    switch::app_windows()
        .into_iter()
        .filter_map(|hwnd| {
            let (path, pid) = switch::process_of(hwnd);
            let path = path?;
            if pid == own {
                return None;
            }
            let app = crate::mixer::program_name(&path);
            Some(OpenWindow { id: (hwnd.0 as isize).to_string(), title: title(hwnd), app, icon: crate::icon::of(&path) })
        })
        .collect()
}

pub(crate) fn title(hwnd: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    let mut buf = vec![0u16; len as usize + 1];
    let copied = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..copied.max(0) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relaunch_commands_split() {
        let files = [r"C:\Program Files (x86)\Steam\steam.exe", r"C:\app.exe"];
        let split = |c| split_command(c, |path| files.contains(&path));
        let pair = |p: &str, r: &str| Some((p.to_string(), r.to_string()));
        // Steam's: without quotes, spaces in the path
        assert_eq!(split(r"C:\Program Files (x86)\Steam\steam.exe"), pair(r"C:\Program Files (x86)\Steam\steam.exe", ""));
        assert_eq!(split(r"C:\Program Files (x86)\Steam\steam.exe -silent"), pair(r"C:\Program Files (x86)\Steam\steam.exe", "-silent"));
        assert_eq!(
            split(r#""C:\Program Files\App\app.exe" --profile-directory="Profile 1" -x"#),
            pair(r"C:\Program Files\App\app.exe", r#"--profile-directory="Profile 1" -x"#)
        );
        assert_eq!(split(r"C:\app.exe  -a"), pair(r"C:\app.exe", "-a"));
        // Not found: the whole line, which will fail with a message
        assert_eq!(split(r"C:\Missing Folder\x.exe"), pair(r"C:\Missing Folder\x.exe", ""));
        assert_eq!(split(r#""C:\unfinished.exe"#), None);
        assert_eq!(split("  "), None);
    }

    /// Diagnostic, read-only: cargo test real_open_windows -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_open_windows() {
        crate::com::with_com(|| {
            let start = std::time::Instant::now();
            for w in list() {
                eprintln!("{:<22} {}  (icon: {})", w.app, w.title, w.icon.is_some());
            }
            eprintln!("time: {:?}", start.elapsed());
        });
    }
}
