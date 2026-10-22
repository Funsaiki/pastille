//! Updates diagnostic (update.rs): is the latest release (latest.json, installer,
//! signature) valid for the public key in tauri.conf.json? Downloads without installing
//! anything, without a window or any link to the installed Pastille:
//! cargo test --test mise_a_jour -- --ignored --nocapture
//! (Integration test: only this kind of target gets the manifest from build.rs.)

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
    let update = tauri::async_runtime::block_on(updater.check()).unwrap().expect("no release");
    eprintln!("{}: {}", update.version, update.download_url);
    let bytes = tauri::async_runtime::block_on(update.download(|_, _| {}, || {})).unwrap();
    eprintln!("{} bytes, signature verified", bytes.len());
}
