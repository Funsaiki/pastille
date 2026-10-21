//! Langue de Pastille (demandée par Funsaiki le 21/10/2026) : français ou anglais, choisie dans
//! la page Paramètres (prefs.rs, reglages.json), sinon celle de Windows. Les textes de Rust que
//! voit l'utilisateur (erreurs montrées dans les notifications, menu de l'icône, pages de la
//! connexion à Spotify) s'écrivent dans les deux : `tr("français", "English")`, et deux
//! `format!` pour un texte avec des valeurs. Les journaux de développement restent en français.

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

pub fn set_english(on: bool) {
    ENGLISH.store(on, Ordering::Relaxed);
}

/// Le texte de la langue en cours
pub fn tr<T>(fr: T, en: T) -> T {
    if english() { en } else { fr }
}
