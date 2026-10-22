//! Pastille's language (requested by Funsaiki on 2026-10-21): French or English, chosen in
//! the Settings page (prefs.rs, settings.json), otherwise Windows's. The Rust texts the
//! user sees (errors shown in the notifications, tray icon menu, Spotify connection
//! pages) are written in both: `tr("français", "English")`, and two
//! `format!` for a text with values. The development logs are in English only.

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
thread_local! {
    /// Tests: English on one thread only (the global language is shared by every test)
    static TEST_ENGLISH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn english() -> bool {
    #[cfg(test)]
    if TEST_ENGLISH.get() {
        return true;
    }
    ENGLISH.load(Ordering::Relaxed)
}

/// Tests: `f` in English, on this thread only
#[cfg(test)]
pub fn in_english<T>(f: impl FnOnce() -> T) -> T {
    TEST_ENGLISH.set(true);
    let result = f();
    TEST_ENGLISH.set(false);
    result
}

pub fn set_english(on: bool) {
    ENGLISH.store(on, Ordering::Relaxed);
}

/// The text in the current language
pub fn tr<T>(fr: T, en: T) -> T {
    if english() { en } else { fr }
}
