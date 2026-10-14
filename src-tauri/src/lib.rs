mod config;
mod launcher;
mod shell;
mod steam;
mod tray;

use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();
    // Instance unique (doit être le premier plugin), en production seulement : en développement,
    // l'app installée empêcherait `npm run tauri dev` de démarrer (même identifiant).
    // Relancer l'app déjà ouverte affiche le lanceur.
    #[cfg(not(debug_assertions))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| launcher::show(app)));
    }
    builder
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // Appelé à l'appui et au relâchement : ne basculer qu'une fois
                    if event.state() == ShortcutState::Pressed {
                        launcher::toggle(app);
                    }
                })
                .build(),
        )
        // N'écrit rien tant que Funsaiki ne coche pas « Démarrer avec Windows »
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .setup(|app| {
            // Lit apps.json et enregistre le raccourci ; s'il est déjà pris, l'app démarre
            // quand même et le lanceur le signale (on l'ouvre par la zone de notification)
            config::init(app.handle());
            let tooltip = config::tooltip(&app.state::<config::SharedConfig>().lock().unwrap());
            tray::create(app.handle(), &tooltip)?;
            launcher::disable_key_menu(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![config::get_config, shell::launch])
        .on_window_event(|window, event| match event {
            // Alt+F4 : masquer au lieu de quitter (on quitte depuis la zone de notification)
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                launcher::hide(window.app_handle());
            }
            WindowEvent::Focused(false) => launcher::on_blur(window.app_handle()),
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
