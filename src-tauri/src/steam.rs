//! Jeux Steam installés, pour les groupes marqués `auto: "steam"`.
//! Dossier Steam lu dans le registre (lecture seule), bibliothèques dans
//! steamapps/libraryfolders.vdf, jeux dans les appmanifest_*.acf de chaque bibliothèque.

use std::{
    fs,
    path::{Path, PathBuf},
};

use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};
use windows::core::w;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

/// Outils Steam qui ne sont pas des jeux
const TOOLS: &[u32] = &[
    228980,  // Steamworks Common Redistributables
    1070560, // Steam Linux Runtime
    1391110, // Steam Linux Runtime - Soldier
    1628350, // Steam Linux Runtime - Sniper
    1493710, // Proton Experimental
    2180100, // Proton Hotfix
    2348590, // Proton 8.0
    2805730, // Proton 9.0
];
/// Bit de StateFlags : entièrement installé (les autres bits signalent une mise à jour, etc.)
const FULLY_INSTALLED: u32 = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub appid: u32,
    pub name: String,
}

/// Jeux installés, triés par nom. Vide si Steam est introuvable.
pub fn installed_games() -> Vec<Game> {
    let Some(steam) = steam_dir() else { return Vec::new() };
    let mut games: Vec<Game> = library_dirs(&steam)
        .iter()
        .filter_map(|lib| fs::read_dir(lib.join("steamapps")).ok())
        .flat_map(|dir| dir.flatten())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("appmanifest_") && name.ends_with(".acf")
        })
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .filter_map(|text| parse_manifest(&text))
        .collect();
    games.sort_by_key(|g| normalize(&g.name));
    games.dedup_by_key(|g| g.appid); // un jeu présent dans deux bibliothèques
    games
}

/// Nom sans accents ni majuscules, pour trier comme on lit (« Été » avec les E)
fn normalize(s: &str) -> String {
    s.nfd().filter(|c| !is_combining_mark(*c)).flat_map(char::to_lowercase).collect()
}

fn steam_dir() -> Option<PathBuf> {
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Valve\\Steam"),
            w!("SteamPath"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    let from_registry = status
        .is_ok()
        .then(|| PathBuf::from(String::from_utf16_lossy(&buf[..(size as usize / 2).saturating_sub(1)])));
    from_registry
        .or_else(|| Some(PathBuf::from("C:\\Program Files (x86)\\Steam")))
        .filter(|dir| dir.is_dir())
}

/// Dossier Steam + bibliothèques déclarées dans libraryfolders.vdf.
fn library_dirs(steam: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![steam.to_path_buf()];
    if let Ok(text) = fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")) {
        dirs.extend(values(&text, "path").map(PathBuf::from));
    }
    // C:\Program Files (x86)\Steam apparaît souvent deux fois, sous deux écritures
    dirs.sort_by_key(|d| d.to_string_lossy().to_lowercase().replace('/', "\\"));
    dirs.dedup_by_key(|d| d.to_string_lossy().to_lowercase().replace('/', "\\"));
    dirs
}

fn parse_manifest(text: &str) -> Option<Game> {
    let appid: u32 = values(text, "appid").next()?.parse().ok()?;
    let flags: u32 = values(text, "StateFlags").next()?.parse().ok()?;
    if flags & FULLY_INSTALLED == 0 || TOOLS.contains(&appid) {
        return None;
    }
    // « Street Fighter™ 6 », « F1® 23 » : les symboles de marque alourdissent les tuiles
    let name: String = values(text, "name").next()?.replace(['™', '®', '©'], "");
    Some(Game { appid, name: name.split_whitespace().collect::<Vec<_>>().join(" ") })
}

/// Valeurs des lignes `"clé"  "valeur"` d'un fichier VDF/ACF (format texte de Valve).
fn values<'a>(text: &'a str, key: &'a str) -> impl Iterator<Item = String> + 'a {
    text.lines().filter_map(move |line| {
        let rest = line.trim().strip_prefix('"')?.strip_prefix(key)?.strip_prefix('"')?;
        let value = rest.trim().strip_prefix('"')?.strip_suffix('"')?;
        Some(value.replace("\\\\", "\\"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "\"AppState\"\n{\n\t\"appid\"\t\t\"2050650\"\n\t\"name\"\t\t\"Street Fighter™ 6\"\n\t\"StateFlags\"\t\t\"6\"\n\t\"UserConfig\"\n\t{\n\t\t\"language\"\t\t\"french\"\n\t}\n}\n";

    #[test]
    fn manifest_is_parsed() {
        // StateFlags 6 = installé (4) + mise à jour en attente (2) : gardé ; 2 seul : pas installé
        assert_eq!(parse_manifest(MANIFEST), Some(Game { appid: 2050650, name: "Street Fighter 6".into() }));
        assert!(parse_manifest(&MANIFEST.replace("\"StateFlags\"\t\t\"6\"", "\"StateFlags\"\t\t\"2\"")).is_none());
        assert!(parse_manifest(&MANIFEST.replace("2050650", "228980")).is_none());
    }

    #[test]
    fn names_sort_without_accents() {
        assert_eq!(normalize("Été Noël"), "ete noel");
    }

    #[test]
    fn library_paths_are_unescaped() {
        let vdf = "\"libraryfolders\"\n{\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t\t\"label\"\t\t\"\"\n\t}\n}\n";
        assert_eq!(values(vdf, "path").collect::<Vec<_>>(), ["D:\\SteamLibrary"]);
        assert_eq!(values(vdf, "pat").count(), 0);
    }

    /// Diagnostic sur l'installation réelle : cargo test real_games -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_games() {
        eprintln!("{:?}", steam_dir());
        for g in installed_games() {
            eprintln!("{:>8}  {}", g.appid, g.name);
        }
    }
}
