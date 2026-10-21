//! Mises à jour automatiques (demandées par Funsaiki le 21/10/2026, une fois le dépôt public) :
//! `latest.json` de la dernière release GitHub, publié par le workflow avec l'installateur signé
//! (clé de Funsaiki, secret `TAURI_SIGNING_PRIVATE_KEY` ; la clé publique est dans
//! tauri.conf.json). Version installée seulement : vérifié une minute après le démarrage (le
//! réseau n'est pas toujours prêt avec Windows), puis toutes les 6 h. Une version plus récente
//! s'affiche dans Notifs ; un clic la télécharge, vérifie sa signature, lance l'installateur
//! (barre de progression seule) et quitte : l'installateur relance Pastille.
//!
//! En développement, rien, sauf avec `PASTILLE_MAJ` (`1` : la vraie release ; sinon l'adresse
//! d'un latest.json) : toute version est proposée, et un clic la télécharge et vérifie sa
//! signature sans l'installer.

use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Mise à jour trouvée, en attente d'un clic
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

const FIRST_CHECK: Duration = Duration::from_secs(60);
const EVERY: Duration = Duration::from_secs(6 * 3600);
/// Vérification et téléchargement (installateur de ~3 Mo)
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
            // Hors ligne : la mise à jour déjà trouvée reste proposée
            match tauri::async_runtime::block_on(check(&app)) {
                Ok(found) => {
                    if let Some(update) = &found {
                        eprintln!("[lanceur] mise à jour : {} disponible", update.version);
                    }
                    *app.state::<Pending>().0.lock().unwrap() = found;
                }
                Err(e) => eprintln!("[lanceur] mise à jour : vérification impossible : {e}"),
            }
            std::thread::sleep(EVERY);
        }
    });
}

/// `PASTILLE_MAJ`, en développement seulement
fn dev_test() -> Option<String> {
    if cfg!(debug_assertions) { std::env::var("PASTILLE_MAJ").ok() } else { None }
}

async fn check(app: &AppHandle) -> Result<Option<Update>, String> {
    let handle = app.clone();
    let mut builder = app
        .updater_builder()
        .timeout(TIMEOUT)
        // Avant l'installateur, Pastille quitte par `std::process::exit` : la barre de Windows
        // revient comme à un Quitter (le processus de garde l'aurait rendue sinon)
        .on_before_exit(move || {
            crate::bar::shutdown();
            handle.cleanup_before_exit();
        });
    if let Some(test) = dev_test() {
        if test != "1" {
            let url = test.parse().map_err(|e| format!("PASTILLE_MAJ : {e}"))?;
            builder = builder.endpoints(vec![url]).map_err(|e| e.to_string())?;
        }
        builder = builder.version_comparator(|_, _| true);
    }
    builder.build().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())
}

/// Version proposée, lue avec les notifications (feed.rs)
pub fn available(app: &AppHandle) -> Option<String> {
    app.state::<Pending>().0.lock().unwrap().as_ref().map(|u| u.version.clone())
}

/// Clic sur la notification : téléchargement, signature vérifiée, installateur lancé, puis
/// Pastille quitte (ne revient pas). En développement, s'arrête après la vérification.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let update = app.state::<Pending>().0.lock().unwrap().clone().ok_or("Plus de mise à jour en attente.")?;
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| format!("Téléchargement de la {} impossible : {e}", update.version))?;
    if cfg!(debug_assertions) {
        eprintln!(
            "[lanceur] mise à jour {} téléchargée et signature vérifiée ({} octets) ; pas installée en développement",
            update.version,
            bytes.len()
        );
        return Ok(());
    }
    update.install(bytes).map_err(|e| format!("Installation de la {} impossible : {e}", update.version))
}
