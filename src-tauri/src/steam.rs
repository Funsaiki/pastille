//! Jeux Steam installés, pour les groupes marqués `auto: "steam"`, et téléchargements de
//! Steam pour les notifications. Dossier Steam lu dans le registre (lecture seule),
//! bibliothèques dans steamapps/libraryfolders.vdf, jeux dans les appmanifest_*.acf de
//! chaque bibliothèque.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};
use windows_registry::CURRENT_USER;

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
/// Bits de StateFlags d'une mise à jour en cours : en marche, démarrée, téléchargement,
/// préparation, installation des fichiers
const UPDATING: u32 = 256 | 1024 | 1_048_576 | 2_097_152 | 4_194_304;
const PAUSED: u32 = 512;
/// Une mise à jour terminée reste signalée 24 h
const RECENT_SECS: u64 = 24 * 3600;

#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub appid: u32,
    pub name: String,
}

/// Jeux installés, triés par nom. Vide si Steam est introuvable.
pub fn installed_games() -> Vec<Game> {
    let mut games: Vec<Game> = manifests().iter().filter_map(|text| parse_manifest(text)).collect();
    games.sort_by_key(|g| normalize(&g.name));
    games.dedup_by_key(|g| g.appid); // un jeu présent dans deux bibliothèques
    games
}

/// Téléchargement d'un jeu : mise à jour en cours, en pause, ou terminée depuis moins de 24 h
#[derive(Debug, PartialEq, Serialize)]
pub struct Activity {
    pub game: String,
    /// Part déjà téléchargée d'une mise à jour en cours, en %, si Steam la donne
    pub progress: Option<u8>,
    pub paused: bool,
    /// Secondes depuis la fin de la mise à jour ; None tant qu'elle n'est pas finie
    pub age: Option<u64>,
}

pub fn activity() -> Vec<Activity> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut seen = HashSet::new();
    manifests()
        .iter()
        .filter_map(|text| parse_activity(text, now))
        .filter(|(appid, _)| seen.insert(*appid))
        .map(|(_, activity)| activity)
        .collect()
}

/// Contenu des appmanifest_*.acf de toutes les bibliothèques
fn manifests() -> Vec<String> {
    let Some(steam) = steam_dir() else { return Vec::new() };
    library_dirs(&steam)
        .iter()
        .filter_map(|lib| fs::read_dir(lib.join("steamapps")).ok())
        .flat_map(|dir| dir.flatten())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("appmanifest_") && name.ends_with(".acf")
        })
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .collect()
}

/// Nom sans accents ni majuscules, pour trier comme on lit (« Été » avec les E)
fn normalize(s: &str) -> String {
    s.nfd().filter(|c| !is_combining_mark(*c)).flat_map(char::to_lowercase).collect()
}

fn steam_dir() -> Option<PathBuf> {
    let dir = CURRENT_USER
        .open(r"Software\Valve\Steam")
        .and_then(|key| key.get_string("SteamPath"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"C:\Program Files (x86)\Steam"));
    dir.is_dir().then_some(dir)
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

/// Champs communs d'un manifeste : identifiant, état, nom. None pour les outils Steam.
fn parse_common(text: &str) -> Option<(u32, u32, String)> {
    let appid: u32 = values(text, "appid").next()?.parse().ok()?;
    if TOOLS.contains(&appid) {
        return None;
    }
    let flags: u32 = values(text, "StateFlags").next()?.parse().ok()?;
    Some((appid, flags, clean_name(&values(text, "name").next()?)))
}

fn parse_manifest(text: &str) -> Option<Game> {
    let (appid, flags, name) = parse_common(text)?;
    (flags & FULLY_INSTALLED != 0).then_some(Game { appid, name })
}

fn parse_activity(text: &str, now: u64) -> Option<(u32, Activity)> {
    let (appid, flags, game) = parse_common(text)?;
    let number = |key| values(text, key).next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    if flags & (UPDATING | PAUSED) != 0 {
        let (done, total) = (number("BytesDownloaded"), number("BytesToDownload"));
        let progress = (total > 0).then(|| (done.saturating_mul(100) / total).min(100) as u8);
        return Some((appid, Activity { game, progress, paused: flags & PAUSED != 0, age: None }));
    }
    let updated = number("LastUpdated");
    let age = now.checked_sub(updated)?;
    (flags == FULLY_INSTALLED && updated > 0 && age < RECENT_SECS)
        .then_some((appid, Activity { game, progress: None, paused: false, age: Some(age) }))
}

/// « Street Fighter™ 6 », « F1® 23 » : les symboles de marque alourdissent les tuiles
fn clean_name(name: &str) -> String {
    name.replace(['™', '®', '©'], "").split_whitespace().collect::<Vec<_>>().join(" ")
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
    fn activity_is_parsed() {
        let now = 1_800_000_000;
        let with = |flags: &str, extra: &str| {
            MANIFEST.replace("\"StateFlags\"\t\t\"6\"", &format!("\"StateFlags\"\t\t\"{flags}\"\n{extra}"))
        };
        // Mise à jour en cours (1026 = à mettre à jour + démarrée), à 25 %
        let running = with("1026", "\"BytesDownloaded\"\t\t\"250\"\n\"BytesToDownload\"\t\t\"1000\"");
        assert_eq!(
            parse_activity(&running, now).map(|(_, a)| a),
            Some(Activity { game: "Street Fighter 6".into(), progress: Some(25), paused: false, age: None })
        );
        // En pause, sans taille connue
        let paused = parse_activity(&with("1542", ""), now).unwrap().1;
        assert!(paused.paused && paused.progress.is_none());
        // Terminée il y a une heure ; il y a deux jours, plus signalée
        let done = |ago: u64| with("4", &format!("\"LastUpdated\"\t\t\"{}\"", now - ago));
        assert_eq!(parse_activity(&done(3600), now).unwrap().1.age, Some(3600));
        assert!(parse_activity(&done(2 * 86400), now).is_none());
        // Mise à jour en attente, pas commencée (6) : rien à signaler
        assert!(parse_activity(&with("6", ""), now).is_none());
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
        eprintln!("activité : {:?}", activity());
    }
}
