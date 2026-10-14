//! Jeux Epic installés, pour les groupes marqués `auto: "epic"` : manifestes du lanceur Epic
//! (%PROGRAMDATA%\Epic\EpicGamesLauncher\Data\Manifests\*.item, en JSON, lecture seule),
//! lancés par l'adresse du lanceur Epic, comme ses raccourcis de bureau.

use std::{fs, path::PathBuf};

use serde::Deserialize;

use crate::steam::{clean_name, normalize};

#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub name: String,
    pub target: String,
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(rename = "DisplayName")]
    display_name: String,
    #[serde(rename = "CatalogNamespace")]
    namespace: String,
    #[serde(rename = "CatalogItemId")]
    item_id: String,
    #[serde(rename = "AppName")]
    app_name: String,
    #[serde(rename = "MainGameAppName", default)]
    main_game: String,
    #[serde(rename = "bIsIncompleteInstall", default)]
    incomplete: bool,
    #[serde(rename = "AppCategories", default)]
    categories: Vec<String>,
}

/// Jeux installés, triés par nom. Vide si le lanceur Epic n'est pas installé.
pub fn installed_games() -> Vec<Game> {
    let data = std::env::var_os("ProgramData").map_or_else(|| PathBuf::from(r"C:\ProgramData"), PathBuf::from);
    let Ok(entries) = fs::read_dir(data.join(r"Epic\EpicGamesLauncher\Data\Manifests")) else { return Vec::new() };
    let mut games: Vec<Game> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("item")))
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .filter_map(|text| parse(&text))
        .collect();
    games.sort_by_key(|g| normalize(&g.name));
    // Un jeu peut avoir deux manifestes (deux chez Funsaiki pour F1 Manager 2024) : même nom, voisins
    games.dedup_by(|a, b| a.target == b.target);
    games
}

fn parse(text: &str) -> Option<Game> {
    let m: Manifest = serde_json::from_str(text.strip_prefix('\u{feff}').unwrap_or(text)).ok()?;
    let dlc = !m.main_game.is_empty() && m.main_game != m.app_name;
    // Installation inachevée, DLC (rattaché à un autre jeu), ou pas un jeu (moteur, outil)
    if m.incomplete || dlc || !m.categories.iter().any(|c| c == "games") {
        return None;
    }
    Some(Game {
        name: clean_name(&m.display_name),
        target: format!(
            "com.epicgames.launcher://apps/{}%3A{}%3A{}?action=launch&silent=true",
            m.namespace, m.item_id, m.app_name
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{ "DisplayName": "F1® Manager 2024", "AppName": "03c9", "MainGameAppName": "03c9",
        "CatalogNamespace": "53ea", "CatalogItemId": "93ba", "bIsIncompleteInstall": false,
        "AppCategories": ["games", "applications"], "LaunchExecutable": "F1Manager24.exe" }"#;

    #[test]
    fn manifest_is_parsed() {
        assert_eq!(
            parse(MANIFEST),
            Some(Game {
                name: "F1 Manager 2024".into(),
                target: "com.epicgames.launcher://apps/53ea%3A93ba%3A03c9?action=launch&silent=true".into(),
            })
        );
        assert!(parse(&MANIFEST.replace(r#""MainGameAppName": "03c9""#, r#""MainGameAppName": "ffff""#)).is_none(), "DLC");
        assert!(parse(&MANIFEST.replace("false", "true")).is_none(), "inachevé");
        assert!(parse(&MANIFEST.replace(r#""games", "#, "")).is_none(), "pas un jeu");
    }

    /// Diagnostic : cargo test real_epic -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_epic() {
        for g in installed_games() {
            eprintln!("{}  {}", g.name, g.target);
        }
    }
}
