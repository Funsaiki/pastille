//! Lancement des cibles par ShellExecuteExW, comme un double-clic dans l'Explorateur :
//! .exe (avec arguments), .lnk, dossiers, fichiers, URL, liens d'application, .bat.
//! Contrairement à std::process::Command, l'invite UAC s'affiche pour les programmes
//! qui demandent l'élévation.

use std::path::Path;

use tauri::AppHandle;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::config;

/// Le front envoie l'id d'un élément (a… : application, f… : dossier) ; Rust retrouve sa cible
/// dans la config chargée.
#[tauri::command]
pub async fn launch(app: AppHandle, id: String) -> Result<(), String> {
    let (target, args) = config::target_of(&app, &id)
        .ok_or_else(|| format!("Élément {id} introuvable, la config a peut-être changé."))?;
    // Hors du thread principal : l'invite UAC, par exemple, attend la réponse de l'utilisateur
    tauri::async_runtime::spawn_blocking(move || open(&target, &args))
        .await
        .map_err(|e| e.to_string())?
}

/// Boutons du haut de la vue réglages : le front envoie un nom, pas une cible.
#[tauri::command]
pub async fn open_tool(tool: String) -> Result<(), String> {
    let target = match tool.as_str() {
        "settings" => "ms-settings:",
        "taskmgr" => "taskmgr.exe",
        _ => return Err(format!("Outil inconnu : {tool}.")),
    };
    tauri::async_runtime::spawn_blocking(move || open(target, &[]))
        .await
        .map_err(|e| e.to_string())?
}

/// Ouvre `target` avec `args`, après développement des variables d'environnement (%USERPROFILE%…).
pub fn open(target: &str, args: &[String]) -> Result<(), String> {
    let target = expand_env(target);
    let mut args: Vec<String> = args.iter().map(|a| expand_env(a)).collect();
    let (file, dir) = if is_uri(&target) {
        (target.clone(), None)
    } else {
        let path = target.replace('/', "\\");
        let dir = working_dir(&path);
        // Windows ouvre les .ps1 dans le Bloc-notes : on les exécute avec PowerShell
        if path.to_ascii_lowercase().ends_with(".ps1") {
            args.splice(0..0, ["-NoProfile".into(), "-File".into(), path]);
            ("powershell.exe".into(), dir)
        } else {
            (path, dir)
        }
    };
    let params = args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ");
    eprintln!("[lanceur] ouvrir {file} {params}");
    shell_execute(&file, &params, dir.as_deref())
        .map_err(|e| format!("{target} : {}", e.message().trim_end()))
}

/// « steam://… », « spotify: », « ms-settings: »… Un préfixe d'une seule lettre est un lecteur (C:).
fn is_uri(s: &str) -> bool {
    match s.find(':') {
        Some(i) if i > 1 => s[..i]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')),
        _ => false,
    }
}

/// Dossier de travail = celui du fichier lancé : beaucoup de jeux et d'outils en ont besoin.
fn working_dir(path: &str) -> Option<String> {
    let path = Path::new(path);
    if !path.is_file() {
        return None;
    }
    path.parent().map(|dir| dir.to_string_lossy().into_owned())
}

/// Met un argument entre guillemets selon les règles de la ligne de commande Windows
/// (celles de CommandLineToArgvW).
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
        // Les barres obliques inverses ne sont spéciales que devant un guillemet
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

fn shell_execute(file: &str, params: &str, dir: Option<&str>) -> windows::core::Result<()> {
    let file = HSTRING::from(file);
    let params = HSTRING::from(params);
    let dir = dir.map(HSTRING::from);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        // Pas de boîte d'erreur de Windows : le lanceur affiche lui-même le problème
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: if params.is_empty() { PCWSTR::null() } else { PCWSTR(params.as_ptr()) },
        lpDirectory: dir.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // ShellExecuteEx peut passer par des extensions du shell, qui demandent COM
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

    #[test]
    fn quoting() {
        assert_eq!(quote("simple"), "simple");
        assert_eq!(quote("C:\\dev\\mon projet"), "\"C:\\dev\\mon projet\"");
        assert_eq!(quote("C:\\dossier avec espace\\"), "\"C:\\dossier avec espace\\\\\"");
        assert_eq!(quote("dit \"bonjour\""), "\"dit \\\"bonjour\\\"\"");
        assert_eq!(quote(""), "\"\"");
    }

    #[test]
    fn env_is_expanded() {
        let home = std::env::var("USERPROFILE").unwrap();
        assert_eq!(expand_env("%USERPROFILE%/Projets"), format!("{home}/Projets"));
        assert_eq!(expand_env("%VARIABLE_QUI_N_EXISTE_PAS%"), "%VARIABLE_QUI_N_EXISTE_PAS%");
        assert_eq!(expand_env("sans variable"), "sans variable");
    }
}
