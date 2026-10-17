//! Lancement des cibles par ShellExecuteExW, comme un double-clic dans l'Explorateur :
//! .exe (avec arguments), .lnk, dossiers, fichiers, URL, liens d'application, .bat.
//! Contrairement à std::process::Command, l'invite UAC s'affiche pour les programmes
//! qui demandent l'élévation.

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

use crate::{config, start_menu};

/// Le front envoie l'id d'un élément (a… : application, f… : dossier) ; Rust retrouve sa cible
/// dans la config chargée.
/// `fresh` (Ctrl+clic) : nouvelle fenêtre même si l'application est déjà ouverte.
#[tauri::command]
pub async fn launch(app: AppHandle, id: String, fresh: bool) -> Result<(), String> {
    let (target, args) = match setting_target(&id) {
        Some(target) => (target, Vec::new()),
        None => config::target_of(&app, &id)
            .ok_or_else(|| format!("Élément {id} introuvable, la config a peut-être changé."))?,
    };
    let key = crate::usage::key(&target, &args);
    // Playlist (groupe auto « spotify ») : Spotify la joue directement (spotify.rs)
    if crate::spotify::is_playable(&target) {
        let play = app.clone();
        crate::blocking(move || crate::spotify::play(&play, &target)).await??;
        crate::usage::record(&app, key);
        return Ok(());
    }
    // Hors du fil principal : l'invite UAC, par exemple, attend la réponse de l'utilisateur
    crate::blocking(move || {
        // Déjà ouverte (Code, Brave…) : sa fenêtre revient au premier plan (switch.rs). Une
        // application du menu Démarrer : le programme de son raccourci, s'il n'a pas d'arguments.
        if !fresh && args.is_empty() {
            let program = if target.starts_with(start_menu::PREFIX) {
                crate::com::with_com(|| start_menu::program(&target))
            } else {
                Some(expand_env(&target))
            };
            if program.is_some_and(|program| crate::switch::focus_existing(&program)) {
                eprintln!("[lanceur] déjà ouvert, fenêtre ramenée : {target}");
                return Ok(());
            }
        }
        open(&target, &args)
    })
    .await??;
    crate::usage::record(&app, key);
    Ok(())
}

/// Pages des Paramètres proposées par la recherche (demande de Funsaiki) : la liste du front
/// (src/settings.json), identifiants « s<position> » ; le front n'envoie jamais de cible.
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

/// Boutons du haut de la vue réglages : le front envoie un nom, pas une cible.
#[tauri::command]
pub async fn open_tool(tool: String) -> Result<(), String> {
    let target = match tool.as_str() {
        "settings" => "ms-settings:",
        "taskmgr" => "taskmgr.exe",
        // Notifications
        "steam-downloads" => "steam://open/downloads",
        "windows-update" => "ms-settings:windowsupdate",
        "storage" => "ms-settings:storagesense",
        // Barre pastel : calendrier
        "notifications" => "ms-actioncenter:",
        _ => return Err(format!("Outil inconnu : {tool}.")),
    };
    crate::blocking(move || open(target, &[])).await?
}

/// Ouvre `target` avec `args`, après développement des variables d'environnement (%USERPROFILE%…).
pub fn open(target: &str, args: &[String]) -> Result<(), String> {
    run(target, args, None)
}

/// Comme `open`, en administrateur (clic droit dans la vue applications) : l'invite UAC de Windows
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
        // Windows ouvre les .ps1 dans le Bloc-notes : on les exécute avec PowerShell
        if path.to_ascii_lowercase().ends_with(".ps1") {
            args.splice(0..0, ["-NoProfile".into(), "-File".into(), path]);
            ("powershell.exe".into(), dir)
        } else {
            (path, dir)
        }
    };
    let params = args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ");
    eprintln!("[lanceur] {} {file} {params}", if verb.is_some() { "ouvrir en administrateur" } else { "ouvrir" });
    match shell_execute(&file, &params, dir.as_deref(), verb) {
        // Invite UAC refusée par Funsaiki : rien à signaler
        Err(e) if e.code() == ERROR_CANCELLED.to_hresult() => Ok(()),
        result => result.map_err(|e| format!("{target} : {}", e.message().trim_end())),
    }
}

/// Recherche sur le web quand rien ne correspond dans le lanceur (demande de Funsaiki) : le front
/// envoie le texte, Rust construit l'adresse ; ouverte dans le navigateur par défaut.
#[tauri::command]
pub async fn web_search(query: String) -> Result<(), String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("Rien à chercher.".into());
    }
    let url = format!("https://www.google.com/search?q={}", encode_query(query));
    crate::blocking(move || open_as_is(&url)).await?
}

/// Paramètre d'adresse : espaces en « + », tout sauf lettres, chiffres et - _ . ~ en %XX (UTF-8)
pub(crate) fn encode_query(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Ouvre un fichier ou une adresse tels quels, comme un double-clic dans l'Explorateur
/// (fichiers téléchargés, recherche web) : sans développer les variables d'environnement
/// (« %20 » reste « %20 »), et un .ps1 s'ouvre comme dans l'Explorateur au lieu d'être exécuté
/// par PowerShell (les .exe, .bat… se lancent, comme d'un double-clic)
pub fn open_as_is(path: &str) -> Result<(), String> {
    eprintln!("[lanceur] ouvrir {path}");
    shell_execute(path, "", working_dir(path).as_deref(), None).map_err(|e| format!("{path} : {}", e.message().trim_end()))
}

/// « steam://… », « spotify: », « ms-settings: »… Un préfixe d'une seule lettre est un lecteur (C:).
pub(crate) fn is_uri(s: &str) -> bool {
    match s.find(':') {
        Some(i) if i > 1 => s[..i]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')),
        _ => false,
    }
}

/// Programme qu'ouvre Windows par défaut pour un protocole (« https » : le navigateur) ou une
/// extension (« .pdf »), d'après les choix des Paramètres : tuiles `default` de apps.json
pub fn default_program(kind: &str) -> Option<String> {
    let kind = kind.trim_end_matches(':');
    let flags = if kind.starts_with('.') { ASSOCF_NONE } else { ASSOCF_IS_PROTOCOL };
    let kind = HSTRING::from(kind);
    let mut len = 0u32;
    unsafe {
        // Une première fois pour la longueur (S_FALSE), puis le chemin
        AssocQueryStringW(flags, ASSOCSTR_EXECUTABLE, &kind, PCWSTR::null(), None, &mut len).ok().ok()?;
        let mut buf = vec![0u16; len as usize];
        AssocQueryStringW(flags, ASSOCSTR_EXECUTABLE, &kind, PCWSTR::null(), Some(windows::core::PWSTR(buf.as_mut_ptr())), &mut len).ok().ok()?;
        let path = String::from_utf16_lossy(&buf[..(len as usize).saturating_sub(1)]);
        Path::new(&path).is_file().then_some(path)
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

/// `verb` : « runas » pour l'administrateur ; sinon l'action par défaut (ouvrir)
fn shell_execute(file: &str, params: &str, dir: Option<&str>, verb: Option<&str>) -> windows::core::Result<()> {
    let file = HSTRING::from(file);
    let params = HSTRING::from(params);
    let dir = dir.map(HSTRING::from);
    let verb = verb.map(HSTRING::from);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        // Pas de boîte d'erreur de Windows : le lanceur affiche lui-même le problème
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: verb.as_ref().map_or(PCWSTR::null(), |v| PCWSTR(v.as_ptr())),
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

    /// Lecture seule : rien n'est ouvert
    #[test]
    fn default_programs() {
        let browser = default_program("https").expect("un navigateur par défaut");
        assert!(browser.to_lowercase().ends_with(".exe") && Path::new(&browser).is_file(), "{browser}");
        assert_eq!(default_program("https:"), Some(browser));
        assert!(default_program("protocole-qui-n-existe-pas").is_none());
    }

    #[test]
    fn settings_pages() {
        assert_eq!(setting_target("s0").as_deref(), Some("ms-settings:network-wifi"));
        assert!(setting_target("s999").is_none());
        assert!(setting_target("a0").is_none(), "une app de apps.json n'est pas une page");
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
