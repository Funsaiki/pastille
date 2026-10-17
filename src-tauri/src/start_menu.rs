//! Applications du menu Démarrer (demande de Funsaiki) : celles de « Toutes les applications »,
//! lues dans le dossier virtuel shell:AppsFolder (raccourcis du menu Démarrer de Funsaiki et de
//! tous les utilisateurs, apps du Store), pour les groupes `auto: "start"` (config.rs). Relues
//! avec les jeux : une application installée plus tard apparaît d'elle-même. Lancées par
//! `shell:AppsFolder\<nom>`, comme depuis le menu Démarrer (arguments du raccourci compris).
//!
//! Écartés : désinstalleurs, pages web et documents (README, manuels) que des installateurs
//! ajoutent au menu, et le lanceur lui-même. Funsaiki masque les autres depuis le clic droit
//! (app_menu.rs) : demarrer.json, à côté de apps.json.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use windows::core::{w, Interface, GUID, HSTRING, PWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    BHID_EnumItems, IEnumShellItems, IShellItem, IShellItem2, SHCreateItemFromParsingName, SIGDN, SIGDN_NORMALDISPLAY,
    SIGDN_PARENTRELATIVEPARSING,
};

use crate::icon::{self, Icon};
use crate::steam::normalize;
use crate::{com, config};

/// Préfixe des cibles : « shell:AppsFolder\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App »
pub const PREFIX: &str = r"shell:AppsFolder\";

#[derive(Clone, Debug, PartialEq)]
pub struct StartApp {
    pub name: String,
    /// `PREFIX` + nom de l'élément dans shell:AppsFolder
    pub target: String,
}

/// Par nom, sans accents ni majuscules. À appeler avec COM initialisé.
pub fn installed_apps() -> Vec<StartApp> {
    *URL_ICONS.lock().unwrap() = Some(url_icons());
    let mut apps: Vec<StartApp> = entries()
        .into_iter()
        .filter(|(name, parsing)| is_app(name, parsing))
        .map(|(name, parsing)| StartApp { name, target: format!("{PREFIX}{parsing}") })
        .collect();
    apps.sort_by_cached_key(|a| normalize(&a.name));
    apps.dedup_by(|a, b| a.target == b.target);
    apps
}

/// (nom affiché, nom de l'élément) de tout le dossier, sans filtre
fn entries() -> Vec<(String, String)> {
    let read = || -> windows::core::Result<Vec<(String, String)>> {
        let folder: IShellItem = unsafe { SHCreateItemFromParsingName(w!("shell:AppsFolder"), None)? };
        let items: IEnumShellItems = unsafe { folder.BindToHandler(None, &BHID_EnumItems)? };
        let mut found = Vec::new();
        loop {
            let mut item = [None];
            let mut fetched = 0;
            if unsafe { items.Next(&mut item, Some(&mut fetched)) }.is_err() || fetched == 0 {
                break;
            }
            let Some(item) = item[0].take() else { break };
            if let (Some(name), Some(parsing)) = (display_name(&item, SIGDN_NORMALDISPLAY), display_name(&item, SIGDN_PARENTRELATIVEPARSING)) {
                found.push((name, parsing));
            }
        }
        Ok(found)
    };
    read().unwrap_or_else(|e| {
        eprintln!("[lanceur] menu Démarrer illisible : {}", e.message().trim_end());
        Vec::new()
    })
}

fn display_name(item: &IShellItem, kind: SIGDN) -> Option<String> {
    unsafe {
        let text: PWSTR = item.GetDisplayName(kind).ok()?;
        let value = text.to_string().ok();
        CoTaskMemFree(Some(text.0 as *const _));
        value.filter(|v| !v.is_empty())
    }
}

/// Ce que l'on peut lancer comme une application
fn is_app(name: &str, parsing: &str) -> bool {
    let name = normalize(name);
    let lower = parsing.to_lowercase();
    if name.starts_with("uninstall") || name.starts_with("desinstaller") || lower == "dev.lanceur.pastel" {
        return false;
    }
    // Pages web et documentation en ligne ou locale
    if ["http:", "https:", "file:", "mailto:"].iter().any(|p| lower.starts_with(p)) {
        return false;
    }
    // Chemin de fichier (« C:\… » ou « {dossier connu}\… ») : programmes seulement, pas les
    // README, manuels (.chm, .pdf, .html) ni raccourcis web (.url) du dossier d'une application
    if lower.contains('\\') {
        let extension = Path::new(&lower).extension().and_then(|e| e.to_str()).unwrap_or("");
        return matches!(extension, "exe" | "msc" | "cpl" | "bat" | "cmd");
    }
    // Identifiant d'application (Store, programmes déclarés) ou adresse d'un jeu (steam://…)
    true
}

/// Icônes des applications `ids` (tuiles affichées, Récents), dans le même ordre : le front
/// n'envoie que des identifiants. ~15 ms par icône la première fois, gardées ensuite (icon.rs).
#[tauri::command]
pub async fn start_icons(app: AppHandle, ids: Vec<String>) -> Vec<Option<Icon>> {
    let targets: Vec<Option<String>> = ids
        .iter()
        .map(|id| config::target_of(&app, id).map(|(target, _)| target).filter(|t| t.starts_with(PREFIX)))
        .collect();
    com::com_task(move || Ok(targets.iter().map(|t| t.as_deref().and_then(icon::of_start_app)).collect()))
        .await
        .unwrap_or_default()
}

/// L'élément du menu Démarrer d'une cible (`PREFIX` + nom). Avec COM.
pub fn item(target: &str) -> Option<IShellItem> {
    target.strip_prefix(PREFIX)?;
    unsafe { SHCreateItemFromParsingName(&HSTRING::from(target), None) }.ok()
}

/// L'élément dont l'icône est celle de la cible : le fichier d'icône d'un raccourci web,
/// sinon l'élément du menu Démarrer. Avec COM.
pub fn icon_item(target: &str) -> Option<IShellItem> {
    let url = target.strip_prefix(PREFIX)?;
    let file = URL_ICONS.lock().unwrap().as_ref().and_then(|icons| icons.get(url).cloned());
    file.and_then(|file| unsafe { SHCreateItemFromParsingName(&HSTRING::from(file), None) }.ok())
        .or_else(|| item(target))
}

/// Ce que lance une application du menu Démarrer (clic droit, app_menu.rs)
pub struct Shortcut {
    /// Cible du raccourci : un programme (« C:\…\Code.exe »), ou l'adresse d'un jeu Steam
    pub target: Option<String>,
    /// Le raccourci passe des arguments (League of Legends : --launch-product=…)
    pub args: bool,
    /// App du Store de bureau (Terminal, Spotify) : elle s'exécute en administrateur, au
    /// contraire d'une app du Store comme la Calculatrice
    pub packaged_desktop: bool,
}

/// Propriétés des éléments : System.Link.TargetParsingPath, System.Link.Arguments et
/// System.AppUserModel.HostEnvironment (0 programme, 1 app du Store, 2 app du Store de bureau)
const LINK_TARGET: PROPERTYKEY = PROPERTYKEY { fmtid: GUID::from_u128(0xb9b4b3fc_2b51_4a42_b5d8_324146afcf25), pid: 2 };
const LINK_ARGUMENTS: PROPERTYKEY = PROPERTYKEY { fmtid: GUID::from_u128(0x436f2667_14e2_4feb_b30a_146c53b5b674), pid: 100 };
const HOST_ENVIRONMENT: PROPERTYKEY = PROPERTYKEY { fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3), pid: 14 };

/// Avec COM
pub fn shortcut(target: &str) -> Option<Shortcut> {
    let item: IShellItem2 = item(target)?.cast().ok()?;
    let text = |key: &PROPERTYKEY| unsafe {
        let text = item.GetString(key).ok()?;
        let value = text.to_string().ok();
        CoTaskMemFree(Some(text.0 as *const _));
        value.filter(|v| !v.is_empty())
    };
    Some(Shortcut {
        target: text(&LINK_TARGET),
        args: text(&LINK_ARGUMENTS).is_some(),
        packaged_desktop: unsafe { item.GetUInt32(&HOST_ENVIRONMENT) }.is_ok_and(|host| host == 2),
    })
}

/* ── Applications masquées (demande de Funsaiki) : demarrer.json, à côté de apps.json (une version
   plus ancienne refuserait un champ de plus dans apps.json) ; leur nom dans shell:AppsFolder ── */

#[derive(Default, Serialize, Deserialize)]
struct Settings {
    #[serde(default)]
    hidden: Vec<String>,
}

fn settings_file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("demarrer.json"))
}

/// Cibles masquées (`PREFIX` + nom). Fichier absent ou illisible : aucune.
pub fn hidden(app: &AppHandle) -> Vec<String> {
    settings_file(app)
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<Settings>(text.trim_start_matches('\u{feff}')).ok())
        .map(|settings| settings.hidden.iter().map(|name| format!("{PREFIX}{name}")).collect())
        .unwrap_or_default()
}

/// Masque ou réaffiche `target`. Fichier temporaire puis renommage, comme barre.json.
pub fn set_hidden(app: &AppHandle, target: &str, hide: bool) -> Result<(), String> {
    let name = target.strip_prefix(PREFIX).ok_or("Pas une application du menu Démarrer.")?;
    let path = settings_file(app).ok_or("Dossier de config introuvable.")?;
    let mut names: Vec<String> = hidden(app).iter().filter_map(|t| t.strip_prefix(PREFIX)).map(String::from).collect();
    names.retain(|n| n != name);
    if hide {
        names.push(name.to_string());
    }
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, serde_json::to_string_pretty(&Settings { hidden: names }).unwrap_or_default())
        .and_then(|_| fs::rename(&temp, &path))
        .map_err(|e| format!("{} non enregistré : {e}", path.display()))
}

/// Raccourcis web (.url) du menu Démarrer : fichier d'icône par adresse. Le menu Démarrer
/// dessine l'icône du jeu pour ceux de Steam (steam://rungameid/…), le dossier des
/// applications seulement une page blanche. Relus avec la liste.
static URL_ICONS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn url_icons() -> HashMap<String, String> {
    let mut found = HashMap::new();
    let mut dirs: Vec<PathBuf> = ["APPDATA", "ProgramData"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|dir| PathBuf::from(dir).join(r"Microsoft\Windows\Start Menu\Programs"))
        .collect();
    while let Some(dir) = dirs.pop() {
        for path in fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()) {
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("url")) {
                let Ok(bytes) = fs::read(&path) else { continue };
                let text = String::from_utf8_lossy(&bytes);
                let value = |key: &str| text.lines().find_map(|l| l.trim().strip_prefix(key)).map(|v| v.trim().to_string());
                if let (Some(url), Some(icon)) = (value("URL="), value("IconFile="))
                    && Path::new(&icon).is_file()
                {
                    found.insert(url, icon);
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_and_uninstallers_are_skipped() {
        assert!(is_app("Calculatrice", "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"));
        assert!(is_app("Counter-Strike 2", "steam://rungameid/730"));
        assert!(is_app("osu!", r"C:\Users\x\AppData\Local\osu!\osu!.exe"));
        assert!(is_app("Steam", r"{7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E}\Steam\steam.exe"));
        assert!(is_app("League of Legends", "Microsoft.AutoGenerated.{7288B581-7522-5059-2069-0240330C1BF2}"));
        assert!(!is_app("Uninstall Node.js", "Microsoft.AutoGenerated.{03CD90DE-8CB0-CBA2-E8EE-E4AD91AE101B}"));
        assert!(!is_app("Désinstaller X", r"C:\X\unins000.exe"));
        assert!(!is_app("Node.js website", "https://nodejs.org/"));
        assert!(!is_app("PostgreSQL documentation", r"file://F:\Program Files\PostgreSQL\17/doc/index.html"));
        assert!(!is_app("README", r"{6D809377-6AF0-444B-8957-A3773F02200E}\MKVToolNix\doc\README.txt"));
        assert!(!is_app("PuTTY Web Site", r"{6D809377-6AF0-444B-8957-A3773F02200E}\PuTTY\website.url"));
        assert!(!is_app("Lanceur pastel", "dev.lanceur.pastel"));
    }

    /// Diagnostic sur le PC (lecture seule, rien n'est lancé) :
    /// cargo test real_start_menu -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_start_menu() {
        crate::com::with_com(|| {
            let started = std::time::Instant::now();
            let all = entries();
            let apps = installed_apps();
            eprintln!("{} éléments, {} gardés, lus en {:?}", all.len(), apps.len(), started.elapsed());
            for (name, parsing) in all.iter().filter(|(n, p)| !is_app(n, p)) {
                eprintln!("  écarté : {name} — {parsing}");
            }
            let started = std::time::Instant::now();
            let mut missing = Vec::new();
            for app in &apps {
                match item(&app.target).and_then(|i| display_name(&i, SIGDN_NORMALDISPLAY)) {
                    Some(name) if name == app.name => {}
                    other => missing.push(format!("{} — {} → {other:?}", app.name, app.target)),
                }
            }
            eprintln!("cibles retrouvées en {:?} ; introuvables : {missing:#?}", started.elapsed());
            let started = std::time::Instant::now();
            let icons = apps.iter().filter(|a| crate::icon::of_start_app(&a.target).is_some()).count();
            eprintln!("{icons} icônes sur {} en {:?}", apps.len(), started.elapsed());
            let started = std::time::Instant::now();
            let _ = apps.iter().filter(|a| crate::icon::of_start_app(&a.target).is_some()).count();
            eprintln!("relues (mémoire) en {:?}", started.elapsed());
            for app in &apps {
                eprintln!("  {} — {}", app.name, app.target);
            }
        });
    }
}
