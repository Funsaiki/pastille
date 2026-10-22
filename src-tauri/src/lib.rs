mod app_menu;
mod appearance;
mod audio;
mod bar;
mod bar_pins;
mod bar_preview;
mod com;
mod config;
mod epic;
mod feed;
mod i18n;
mod icon;
mod launcher;
mod media;
mod mixer;
mod open_windows;
mod power;
mod prefs;
mod shell;
mod shell_hook;
mod spotify;
mod start_menu;
mod steam;
mod switch;
mod system;
mod taskbar;
mod tray;
mod tray_apps;
mod tray_host;
mod update;
mod usage;

use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

/// Off the main thread: calls that may wait (disk, COM, UAC prompt…)
pub(crate) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Pastel bar watchdog process (taskbar.rs): nothing else
    if taskbar::guard() {
        return;
    }
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();
    // Single instance (must be the first plugin), in production only: in development,
    // the installed app would keep `npm run tauri dev` from starting (same identifier).
    // Relaunching the app already open shows the launcher.
    #[cfg(not(debug_assertions))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| launcher::show(app)));
    }
    builder
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // Called on press and on release: toggle only once
                    if event.state() == ShortcutState::Pressed {
                        launcher::toggle(app);
                    }
                })
                .build(),
        )
        // Writes nothing until Funsaiki turns on "Start with Windows"
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        // Updates from the GitHub releases (update.rs)
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // Reads apps.json and registers the shortcut; if it's already taken, the app starts
            // anyway and the launcher reports it (it is opened from the notification area)
            com::keep_mta();
            config::rename_old_files(app.handle());   // files named in French, before any read
            prefs::init(app.handle());   // language, before any text
            app.manage(usage::load(app.handle()));
            app.manage(spotify::load(app.handle()));
            config::init(app.handle());
            app.manage(system::Meter::default());
            let tooltip = config::tooltip(&app.state::<config::SharedConfig>().lock().unwrap());
            tray::create(app.handle(), &tooltip)?;
            launcher::disable_key_menu(app.handle());
            if let Some(w) = app.get_webview_window("main") {
                launcher::set_memory_low(&w, true);   // the window starts hidden
            }
            bar::init(app.handle());
            update::init(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::get_config,
            appearance::get_appearance,
            appearance::set_appearance,
            appearance::pick_charm_image,
            prefs::enable_autostart,
            prefs::prefs,
            prefs::get_language,
            prefs::set_language,
            prefs::set_pref_switch,
            prefs::pref_action,
            update::check_update,
            launcher::first_show,
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
            spotify::spotify_reconnect,
            spotify::set_spotify_shuffle,
            spotify::set_spotify_repeat,
            open_windows::open_windows,
            open_windows::focus_window,
            spotify::set_spotify_volume,
            feed::feed,
            feed::open_download,
            update::install_update,
            tray_apps::tray_apps,
            tray_apps::tray_app_action,
            tray_apps::tray_click,
            bar::bar_windows,
            bar::bar_rects,
            bar::bar_hidden,
            bar::minimize_window,
            bar::close_window,
            bar::new_window,
            bar::show_launcher,
            bar::bar_menu,
            bar::bar_screens,
            bar::move_window,
            bar_pins::bar_pins,
            bar_pins::bar_pin,
            bar_pins::bar_unpin,
            bar_pins::bar_open_pin,
            bar_preview::bar_preview,
            start_menu::start_icons,
            app_menu::app_menu,
            app_menu::app_action,
            bar_preview::bar_preview_place,
            bar_preview::bar_preview_close
        ])
        // Launcher window only (not the pastel bar ones)
        .on_window_event(|window, event| match event {
            _ if window.label() != "main" => {}
            // Alt+F4: hide instead of quitting (quitting is done from the notification area)
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
            // The Windows taskbar comes back when the launcher quits
            if let RunEvent::Exit = event {
                bar::shutdown();
            }
        });
}
