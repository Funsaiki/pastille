//! Launching targets with ShellExecuteExW, like a double click in Explorer:
//! .exe (with arguments), .lnk, folders, files, URLs, app links, .bat.
//! Unlike std::process::Command, the UAC prompt shows for programs
//! that ask for elevation.

use std::path::Path;
use std::sync::LazyLock;

use serde::Deserialize;
use tauri::AppHandle;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_CANCELLED;
use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::UI::Shell::{
    AssocQueryStringW, ShellExecuteExW, ASSOCF_IS_PROTOCOL, ASSOCF_NONE, ASSOCSTR_EXECUTABLE, SEE_MASK_FLAG_NO_UI,
    SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::i18n::tr;
use crate::{config, start_menu};

/// The front sends an item's id (a…: app, f…: folder); Rust finds its target
/// in the loaded config.
/// `fresh` (Ctrl+click): new window even if the app is already open.
#[tauri::command]
pub async fn launch(app: AppHandle, id: String, fresh: bool) -> Result<(), String> {
    let (target, args) = match setting_target(&id) {
        Some(target) => (target, Vec::new()),
        None => config::target_of(&app, &id)
            .ok_or_else(|| tr(format!("Élément {id} introuvable, la config a peut-être changé."), format!("Item {id} not found, the config may have changed.")))?,
    };
    let key = crate::usage::key(&target, &args);
    // Playlist (auto group "spotify"): Spotify plays it directly (spotify.rs)
    if crate::spotify::is_playable(&target) {
        let play = app.clone();
        crate::blocking(move || crate::spotify::play(&play, &target)).await??;
        crate::usage::record(&app, key);
        return Ok(());
    }
    // Off the main thread: the UAC prompt, for example, waits for the user's answer
    crate::blocking(move || {
        // Already open (Code, Brave…): its window comes back to the foreground (switch.rs). A
        // Start menu app: the program of its shortcut, if it has no arguments.
        if !fresh && args.is_empty() {
            let program = if target.starts_with(start_menu::PREFIX) {
                crate::com::with_com(|| start_menu::program(&target))
            } else {
                Some(expand_env(&target))
            };
            if program.is_some_and(|program| crate::switch::focus_existing(&program)) {
                eprintln!("[pastille] already open, window brought back: {target}");
                return Ok(());
            }
        }
        open(&target, &args)
    })
    .await??;
    crate::usage::record(&app, key);
    Ok(())
}

/// Windows Settings pages offered by the search (requested by Funsaiki): the front's list
/// (src/settings.json), identifiers "s<position>"; the front never sends a target.
fn setting_target(id: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Setting {
        target: String,
    }
    static SETTINGS: LazyLock<Vec<Setting>> =
        LazyLock::new(|| serde_json::from_str(include_str!("../../src/settings.json")).expect("src/settings.json invalide"));
    let index: usize = id.strip_prefix('s')?.parse().ok()?;
    SETTINGS.get(index).map(|s| s.target.clone())
}

/// Buttons at the top of the quick settings view: the front sends a name, not a target.
#[tauri::command]
pub async fn open_tool(tool: String) -> Result<(), String> {
    let target = match tool.as_str() {
        "settings" => "ms-settings:",
        "taskmgr" => "taskmgr.exe",
        // Status bar: the network list of the Windows taskbar; for Bluetooth, Windows
        // has no reliable link to its quick panel, so it's its Settings
        "wifi" => "ms-availablenetworks:",
        "bluetooth" => "ms-settings:bluetooth",
        // Notifications
        "steam-downloads" => "steam://open/downloads",
        "windows-update" => "ms-settings:windowsupdate",
        "storage" => "ms-settings:storagesense",
        // Pastel bar: calendar
        "notifications" => "ms-actioncenter:",
        _ => return Err(tr(format!("Outil inconnu : {tool}."), format!("Unknown tool: {tool}."))),
    };
    crate::blocking(move || open(target, &[])).await?
}

/// Opens `target` with `args`, after expanding environment variables (%USERPROFILE%…).
pub fn open(target: &str, args: &[String]) -> Result<(), String> {
    run(target, args, None)
}

/// Like `open`, as administrator (right click in the apps view): the Windows UAC prompt
pub fn open_as_admin(target: &str, args: &[String]) -> Result<(), String> {
    run(target, args, Some("runas"))
}

fn run(target: &str, args: &[String], verb: Option<&str>) -> Result<(), String> {
    let target = expand_env(target);
    let mut args: Vec<String> = args.iter().map(|a| expand_env(a)).collect();
    let (file, dir) = if is_uri(&target) {
        (target.clone(), None)
    } else {
        let path = target.replace('/', "\\");
        let dir = working_dir(&path);
        // Windows opens .ps1 files in Notepad: we run them with PowerShell
        if path.to_ascii_lowercase().ends_with(".ps1") {
            args.splice(0..0, ["-NoProfile".into(), "-File".into(), path]);
            ("powershell.exe".into(), dir)
        } else {
            (path, dir)
        }
    };
    let params = args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ");
    eprintln!("[pastille] {} {file} {params}", if verb.is_some() { "open as administrator" } else { "open" });
    match shell_execute(&file, &params, dir.as_deref(), verb) {
        // UAC prompt declined by Funsaiki: nothing to report
        Err(e) if e.code() == ERROR_CANCELLED.to_hresult() => Ok(()),
        result => result.map_err(|e| format!("{target} : {}", e.message().trim_end())),
    }
}

/// Web search when nothing matches in the launcher (requested by Funsaiki): the front
/// sends the text, Rust builds the address; opened in the default browser.
#[tauri::command]
pub async fn web_search(query: String) -> Result<(), String> {
    let query = query.trim();
    if query.is_empty() {
        return Err(tr("Rien à chercher.", "Nothing to search for.").into());
    }
    let url = format!("https://www.google.com/search?q={}", encode_query(query));
    crate::blocking(move || open_as_is(&url)).await?
}

/// URL parameter: spaces as "+", everything but letters, digits and - _ . ~ as %XX (UTF-8)
pub(crate) fn encode_query(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Opens a file or an address as is, like a double click in Explorer
/// (downloaded files, web search): without expanding environment variables
/// ("%20" stays "%20"), and a .ps1 opens as in Explorer instead of being run
/// by PowerShell (.exe, .bat… are launched, as with a double click)
pub fn open_as_is(path: &str) -> Result<(), String> {
    open_command(path, "")
}

/// Like `open_as_is`, with parameters as written: the relaunch command an app gives its
/// windows (open_windows::window_relaunch)
pub fn open_command(path: &str, params: &str) -> Result<(), String> {
    eprintln!("[pastille] open {path} {params}");
    shell_execute(path, params, working_dir(path).as_deref(), None).map_err(|e| format!("{path} : {}", e.message().trim_end()))
}

/// "steam://…", "spotify:", "ms-settings:"… A one-letter prefix is a drive (C:).
pub(crate) fn is_uri(s: &str) -> bool {
    match s.find(':') {
        Some(i) if i > 1 => s[..i]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')),
        _ => false,
    }
}

/// Program Windows opens by default for a protocol ("https": the browser) or an
/// extension (".pdf"), per the choices in Settings: `default` tiles of apps.json
pub fn default_program(kind: &str) -> Option<String> {
    let kind = kind.trim_end_matches(':');
    let flags = if kind.starts_with('.') { ASSOCF_NONE } else { ASSOCF_IS_PROTOCOL };
    let kind = HSTRING::from(kind);
    let mut len = 0u32;
    unsafe {
        // Once for the length (S_FALSE), then the path
        AssocQueryStringW(flags, ASSOCSTR_EXECUTABLE, &kind, PCWSTR::null(), None, &mut len).ok().ok()?;
        let mut buf = vec![0u16; len as usize];
        AssocQueryStringW(flags, ASSOCSTR_EXECUTABLE, &kind, PCWSTR::null(), Some(windows::core::PWSTR(buf.as_mut_ptr())), &mut len).ok().ok()?;
        let path = String::from_utf16_lossy(&buf[..(len as usize).saturating_sub(1)]);
        Path::new(&path).is_file().then_some(path)
    }
}

/// Working directory = the launched file's: many games and tools need it.
fn working_dir(path: &str) -> Option<String> {
    let path = Path::new(path);
    if !path.is_file() {
        return None;
    }
    path.parent().map(|dir| dir.to_string_lossy().into_owned())
}

/// Quotes an argument according to the Windows command line rules
/// (those of CommandLineToArgvW).
fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0;
    for c in arg.chars() {
        if c == '\\' {
            backslashes += 1;
            continue;
        }
        // Backslashes are only special before a quote
        let n = if c == '"' { backslashes * 2 + 1 } else { backslashes };
        out.extend(std::iter::repeat_n('\\', n));
        out.push(c);
        backslashes = 0;
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out
}

pub fn expand_env(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let src = HSTRING::from(s);
    unsafe {
        let len = ExpandEnvironmentStringsW(&src, None) as usize;
        let mut buf = vec![0u16; len];
        let written = ExpandEnvironmentStringsW(&src, Some(&mut buf)) as usize;
        if len == 0 || written == 0 || written > len {
            return s.to_string();
        }
        String::from_utf16_lossy(&buf[..written - 1])
    }
}

/// `verb`: "runas" for administrator; otherwise the default action (open)
fn shell_execute(file: &str, params: &str, dir: Option<&str>, verb: Option<&str>) -> windows::core::Result<()> {
    let file = HSTRING::from(file);
    let params = HSTRING::from(params);
    let dir = dir.map(HSTRING::from);
    let verb = verb.map(HSTRING::from);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        // No Windows error box: the launcher shows the problem itself
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: verb.as_ref().map_or(PCWSTR::null(), |v| PCWSTR(v.as_ptr())),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: if params.is_empty() { PCWSTR::null() } else { PCWSTR(params.as_ptr()) },
        lpDirectory: dir.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // ShellExecuteEx may go through shell extensions, which need COM
    let com = unsafe { CoInitializeEx(None, COINIT(COINIT_APARTMENTTHREADED.0 | COINIT_DISABLE_OLE1DDE.0)) };
    let result = unsafe { ShellExecuteExW(&mut info) };
    if com.is_ok() {
        unsafe { CoUninitialize() };
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_or_path() {
        for uri in ["steam://open/main", "spotify:", "ms-settings:", "com.epicgames.launcher://", "https://a.b"] {
            assert!(is_uri(uri), "{uri}");
        }
        for path in ["C:/Program Files/x.exe", "c:\\x", "wt.exe", "%USERPROFILE%/Projets"] {
            assert!(!is_uri(path), "{path}");
        }
    }

    /// Read only: nothing is opened
    #[test]
    fn default_programs() {
        let browser = default_program("https").expect("a default browser");
        assert!(browser.to_lowercase().ends_with(".exe") && Path::new(&browser).is_file(), "{browser}");
        assert_eq!(default_program("https:"), Some(browser));
        assert!(default_program("protocol-that-does-not-exist").is_none());
    }

    #[test]
    fn settings_pages() {
        assert_eq!(setting_target("s0").as_deref(), Some("ms-settings:network-wifi"));
        assert!(setting_target("s999").is_none());
        assert!(setting_target("a0").is_none(), "an app from apps.json isn't a page");
    }

    #[test]
    fn search_text_is_encoded() {
        assert_eq!(encode_query("street fighter 6"), "street+fighter+6");
        assert_eq!(encode_query("été & co?"), "%C3%A9t%C3%A9+%26+co%3F");
    }

    #[test]
    fn quoting() {
        assert_eq!(quote("simple"), "simple");
        assert_eq!(quote("C:\\dev\\mon projet"), "\"C:\\dev\\mon projet\"");
        assert_eq!(quote("C:\\folder with space\\"), "\"C:\\folder with space\\\\\"");
        assert_eq!(quote("dit \"bonjour\""), "\"dit \\\"bonjour\\\"\"");
        assert_eq!(quote(""), "\"\"");
    }

    #[test]
    fn env_is_expanded() {
        let home = std::env::var("USERPROFILE").unwrap();
        assert_eq!(expand_env("%USERPROFILE%/Projets"), format!("{home}/Projets"));
        assert_eq!(expand_env("%VARIABLE_QUI_N_EXISTE_PAS%"), "%VARIABLE_QUI_N_EXISTE_PAS%");
        assert_eq!(expand_env("no variable"), "no variable");
    }
}
