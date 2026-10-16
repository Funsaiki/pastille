mod audio;
mod bar;
mod com;
mod config;
mod epic;
mod feed;
mod icon;
mod launcher;
mod media;
mod mixer;
mod open_windows;
mod power;
mod shell;
mod shell_hook;
mod spotify;
mod steam;
mod switch;
mod system;
mod taskbar;
mod tray;
mod tray_apps;
mod usage;

use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

/// Hors du fil principal : appels qui peuvent attendre (disque, COM, invite UAC…)
pub(crate) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

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
            com::keep_mta();
            app.manage(usage::load(app.handle()));
            app.manage(spotify::load(app.handle()));
            config::init(app.handle());
            app.manage(system::Meter::default());
            let tooltip = config::tooltip(&app.state::<config::SharedConfig>().lock().unwrap());
            tray::create(app.handle(), &tooltip)?;
            launcher::disable_key_menu(app.handle());
            if let Some(w) = app.get_webview_window("main") {
                launcher::set_memory_low(&w, true);   // la fenêtre démarre masquée
            }
            bar::init(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::get_config,
            launcher::hide_window,
            shell::launch,
            shell::open_tool,
            shell::web_search,
            power::power,
            system::profile,
            system::stats,
            audio::audio,
            audio::set_volume,
            audio::set_mic_muted,
            media::media,
            media::media_control,
            media::media_cover,
            mixer::mixer,
            mixer::set_app_volume,
            spotify::spotify_state,
            spotify::set_spotify_shuffle,
            spotify::set_spotify_repeat,
            open_windows::open_windows,
            open_windows::focus_window,
            spotify::set_spotify_volume,
            feed::feed,
            feed::open_download,
            tray_apps::tray_apps,
            tray_apps::tray_app_action,
            bar::bar_windows,
            bar::bar_rects,
            bar::bar_hidden,
            bar::minimize_window,
            bar::close_window,
            bar::new_window,
            bar::show_launcher,
            bar::bar_menu
        ])
        // Fenêtre du lanceur seulement (pas celles de la barre pastel)
        .on_window_event(|window, event| match event {
            _ if window.label() != "main" => {}
            // Alt+F4 : masquer au lieu de quitter (on quitte depuis la zone de notification)
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                launcher::hide(window.app_handle());
            }
            WindowEvent::Focused(false) => launcher::on_blur(window.app_handle()),
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app, event| {
            // La barre des tâches de Windows revient quand le lanceur quitte
            if let RunEvent::Exit = event {
                bar::shutdown();
            }
        });
}
