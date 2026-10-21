//! Diagnostic des mises à jour (update.rs) : la dernière release (latest.json, installateur,
//! signature) est-elle valide pour la clé publique de tauri.conf.json ? Télécharge sans rien
//! installer, sans fenêtre ni lien avec la Pastille installée :
//! cargo test --test mise_a_jour -- --ignored --nocapture
//! (Test d'intégration : seul ce genre de cible reçoit le manifeste de build.rs.)

use std::time::Duration;

use tauri_plugin_updater::UpdaterExt;

#[test]
#[ignore]
fn real_release_is_signed() {
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    let app = tauri::Builder::default()
        .any_thread()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .unwrap();
    let updater = app
        .updater_builder()
        .timeout(Duration::from_secs(120))
        .version_comparator(|_, _| true)
        .build()
        .unwrap();
    let update = tauri::async_runtime::block_on(updater.check()).unwrap().expect("pas de release");
    eprintln!("{} : {}", update.version, update.download_url);
    let bytes = tauri::async_runtime::block_on(update.download(|_, _| {}, || {})).unwrap();
    eprintln!("{} octets, signature vérifiée", bytes.len());
}
