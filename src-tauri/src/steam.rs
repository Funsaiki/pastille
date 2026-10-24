//! Installed Steam games, for the groups marked `auto: "steam"`, and Steam downloads
//! for the notifications. Steam folder read from the registry (read only),
//! libraries in steamapps/libraryfolders.vdf, games in the appmanifest_*.acf of
//! each library.

use std::{
    cmp::Reverse,
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};
use windows_registry::CURRENT_USER;

/// Steam tools that aren't games
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
/// StateFlags bit: fully installed (the other bits flag an update, etc.)
const FULLY_INSTALLED: u32 = 4;
/// StateFlags bits of an update in progress: running, started, downloading,
/// preparing, installing files
const UPDATING: u32 = 256 | 1024 | 1_048_576 | 2_097_152 | 4_194_304;
const PAUSED: u32 = 512;
/// A finished update stays reported for 24 h
const RECENT_SECS: u64 = 24 * 3600;

#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub appid: u32,
    pub name: String,
    /// Last played (seconds since 1970), 0 if never launched
    pub last_played: u64,
}

/// Installed games, the last played first (requested by Funsaiki), then by name. Empty if Steam
/// isn't found.
pub fn installed_games() -> Vec<Game> {
    let mut games: Vec<Game> = manifests().iter().filter_map(|text| parse_manifest(text)).collect();
    sort_games(&mut games);
    games
}

fn sort_games(games: &mut Vec<Game>) {
    games.sort_by_cached_key(|g| (Reverse(g.last_played), normalize(&g.name)));
    // A game in two libraries: the one played last stays
    let mut seen = HashSet::new();
    games.retain(|g| seen.insert(g.appid));
}

/// A game's download: update in progress, paused, or finished less than 24 h ago
#[derive(Debug, PartialEq, Serialize)]
pub struct Activity {
    pub game: String,
    /// Share of an update in progress already downloaded, in %, if Steam gives it
    pub progress: Option<u8>,
    pub paused: bool,
    /// Seconds since the update finished; None until it is finished
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

/// Contents of the appmanifest_*.acf of all libraries
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

/// Name without accents or capitals, to sort as one reads ("Été" with the E's)
pub fn normalize(s: &str) -> String {
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

/// Steam folder + libraries declared in libraryfolders.vdf.
fn library_dirs(steam: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![steam.to_path_buf()];
    if let Ok(text) = fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")) {
        dirs.extend(values(&text, "path").map(PathBuf::from));
    }
    // C:\Program Files (x86)\Steam often appears twice, spelled two ways
    dirs.sort_by_key(|d| d.to_string_lossy().to_lowercase().replace('/', "\\"));
    dirs.dedup_by_key(|d| d.to_string_lossy().to_lowercase().replace('/', "\\"));
    dirs
}

/// Common fields of a manifest: identifier, state, name. None for Steam tools.
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
    let last_played = values(text, "LastPlayed").next().and_then(|v| v.parse().ok()).unwrap_or(0);
    (flags & FULLY_INSTALLED != 0).then_some(Game { appid, name, last_played })
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

/// "Street Fighter™ 6", "F1® 23": trademark symbols weigh down the tiles
pub fn clean_name(name: &str) -> String {
    name.replace(['™', '®', '©'], "").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Values of the `"key"  "value"` lines of a VDF/ACF file (Valve's text format).
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
        // StateFlags 6 = installed (4) + update pending (2): kept; 2 alone: not installed
        assert_eq!(parse_manifest(MANIFEST), Some(Game { appid: 2050650, name: "Street Fighter 6".into(), last_played: 0 }));
        assert!(parse_manifest(&MANIFEST.replace("\"StateFlags\"\t\t\"6\"", "\"StateFlags\"\t\t\"2\"")).is_none());
        assert!(parse_manifest(&MANIFEST.replace("2050650", "228980")).is_none());
    }

    #[test]
    fn activity_is_parsed() {
        let now = 1_800_000_000;
        let with = |flags: &str, extra: &str| {
            MANIFEST.replace("\"StateFlags\"\t\t\"6\"", &format!("\"StateFlags\"\t\t\"{flags}\"\n{extra}"))
        };
        // Update in progress (1026 = needs update + started), at 25%
        let running = with("1026", "\"BytesDownloaded\"\t\t\"250\"\n\"BytesToDownload\"\t\t\"1000\"");
        assert_eq!(
            parse_activity(&running, now).map(|(_, a)| a),
            Some(Activity { game: "Street Fighter 6".into(), progress: Some(25), paused: false, age: None })
        );
        // Paused, size unknown
        let paused = parse_activity(&with("1542", ""), now).unwrap().1;
        assert!(paused.paused && paused.progress.is_none());
        // Finished an hour ago; two days ago, no longer reported
        let done = |ago: u64| with("4", &format!("\"LastUpdated\"\t\t\"{}\"", now - ago));
        assert_eq!(parse_activity(&done(3600), now).unwrap().1.age, Some(3600));
        assert!(parse_activity(&done(2 * 86400), now).is_none());
        // Update pending, not started (6): nothing to report
        assert!(parse_activity(&with("6", ""), now).is_none());
    }

    #[test]
    fn last_played_first_then_by_name() {
        let game = |appid, name: &str, last_played| Game { appid, name: name.into(), last_played };
        let mut games = vec![game(1, "Hades", 0), game(2, "Apex", 0), game(3, "Zelda", 50), game(4, "Celeste", 90), game(3, "Zelda", 50)];
        sort_games(&mut games);
        let names: Vec<_> = games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Celeste", "Zelda", "Apex", "Hades"]);
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

    /// Diagnostic on the real install: cargo test real_games -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_games() {
        eprintln!("{:?}", steam_dir());
        for g in installed_games() {
            eprintln!("{:>8}  {}", g.appid, g.name);
        }
        eprintln!("activity: {:?}", activity());
    }
}
