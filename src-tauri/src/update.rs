//! Automatic updates (requested by Funsaiki on 2026-10-21, once the repo was public):
//! `latest.json` of the latest GitHub release, published by the workflow with the signed installer
//! (Funsaiki's key, secret `TAURI_SIGNING_PRIVATE_KEY`; the public key is in
//! tauri.conf.json). Installed build only: checked one minute after startup (the
//! network isn't always ready with Windows), then every 6 h. A newer version
//! shows in Notifs; a click downloads it, checks its signature, runs the installer
//! (progress bar only) and quits: the installer restarts Pastille.
//!
//! In development, nothing, except with `PASTILLE_UPDATE` (`1`: the real release; otherwise the URL
//! of a latest.json): any version is offered, and a click downloads it and checks its
//! signature without installing it.

use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::i18n::tr;

/// Update found, waiting for a click
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

const FIRST_CHECK: Duration = Duration::from_secs(60);
const EVERY: Duration = Duration::from_secs(6 * 3600);
/// Check and download (~3 MB installer)
const TIMEOUT: Duration = Duration::from_secs(120);

pub fn init(app: &AppHandle) {
    app.manage(Pending::default());
    if cfg!(debug_assertions) && dev_test().is_none() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(if cfg!(debug_assertions) { Duration::from_secs(5) } else { FIRST_CHECK });
        loop {
            // Offline: the update already found is still offered
            match tauri::async_runtime::block_on(check_now(&app)) {
                Ok(Some(version)) => eprintln!("[pastille] update: {version} available"),
                Ok(None) => {}
                Err(e) => eprintln!("[pastille] update: couldn't check: {e}"),
            }
            std::thread::sleep(EVERY);
        }
    });
}

/// `PASTILLE_UPDATE`, in development only
fn dev_test() -> Option<String> {
    if cfg!(debug_assertions) { std::env::var("PASTILLE_UPDATE").ok() } else { None }
}

async fn check(app: &AppHandle) -> Result<Option<Update>, String> {
    let handle = app.clone();
    let mut builder = app
        .updater_builder()
        .timeout(TIMEOUT)
        // Before the installer, Pastille quits through `std::process::exit`: the Windows taskbar
        // comes back as on a Quit (the watchdog process would have restored it otherwise)
        .on_before_exit(move || {
            crate::bar::shutdown();
            handle.cleanup_before_exit();
        });
    if let Some(test) = dev_test() {
        if test != "1" {
            let url = test.parse().map_err(|e| format!("PASTILLE_UPDATE: {e}"))?;
            builder = builder.endpoints(vec![url]).map_err(|e| e.to_string())?;
        }
        builder = builder.version_comparator(|_, _| true);
    }
    builder.build().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())
}

/// Checks and keeps the update found; returns its version
async fn check_now(app: &AppHandle) -> Result<Option<String>, String> {
    let found = check(app).await?;
    let version = found.as_ref().map(|u| u.version.clone());
    *app.state::<Pending>().0.lock().unwrap() = found;
    Ok(version)
}

/// "Check" on the Settings page: right away, in development too (against the real
/// release; nothing installs without a click, and in development nothing at all)
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<String>, String> {
    check_now(&app).await.map_err(|e| tr(format!("Vérification impossible : {e}"), format!("Couldn't check: {e}")))
}

/// Version offered, read with the notifications (feed.rs) and the Settings page
pub fn available(app: &AppHandle) -> Option<String> {
    app.state::<Pending>().0.lock().unwrap().as_ref().map(|u| u.version.clone())
}

/// The same for the pastel bar's dot on Apps, read each time the pill comes out
#[tauri::command]
pub fn update_available(app: AppHandle) -> Option<String> {
    available(&app)
}

/// Click on the notification: download, signature checked, installer run, then
/// Pastille quits (doesn't return). In development, stops after the check.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let update = app.state::<Pending>().0.lock().unwrap().clone().ok_or(tr("Plus de mise à jour en attente.", "No update pending anymore."))?;
    let version = update.version.clone();
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| tr(format!("Téléchargement de la {version} impossible : {e}"), format!("Couldn't download {version}: {e}")))?;
    if cfg!(debug_assertions) {
        eprintln!(
            "[pastille] update {} downloaded and signature verified ({} bytes); not installed in development",
            update.version,
            bytes.len()
        );
        return Ok(());
    }
    update.install(bytes).map_err(|e| tr(format!("Installation de la {version} impossible : {e}"), format!("Couldn't install {version}: {e}")))
}
