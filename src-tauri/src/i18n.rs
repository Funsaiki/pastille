//! Pastille's language (requested by Funsaiki on 2026-10-21): French or English, chosen in
//! the Settings page (prefs.rs, reglages.json), otherwise Windows's. The Rust texts the
//! user sees (errors shown in the notifications, tray icon menu, Spotify connection
//! pages) are written in both: `tr("français", "English")`, and two
//! `format!` for a text with values. The development logs stay in French.

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

pub fn set_english(on: bool) {
    ENGLISH.store(on, Ordering::Relaxed);
}

/// The text in the current language
pub fn tr<T>(fr: T, en: T) -> T {
    if english() { en } else { fr }
}
