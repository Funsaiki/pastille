//! Icône dans la zone de notification : indispensable, l'app n'apparaît pas dans la barre des tâches.

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle,
};
use tauri_plugin_autostart::ManagerExt;

use crate::{bar, config, launcher, shell, spotify};

pub fn create(app: &AppHandle, tooltip: &str) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Afficher", true, None::<&str>)?;
    let edit = MenuItem::with_id(app, "edit", "Modifier apps.json", true, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "Recharger la config", true, None::<&str>)?;
    // Volume interne de Spotify par son API web (spotify.rs) : connexion dans le navigateur
    let (label, enabled) = spotify::menu_label(app);
    let spotify_item = MenuItem::with_id(app, "spotify", label, enabled, None::<&str>)?;
    spotify::set_menu_item(app, spotify_item.clone());
    // Barre pastel à la place de celle de Windows (bar.rs) : seul Funsaiki la coche
    let bar_item = CheckMenuItem::with_id(app, "bar", "Barre pastel", true, bar::is_active(app), None::<&str>)?;
    // Écrit dans le registre (HKCU\…\Run) : seul Funsaiki la coche, depuis la version installée.
    // En développement, l'entrée pointerait vers l'exe de debug : option grisée.
    let installed = !cfg!(debug_assertions);
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        if installed { "Démarrer avec Windows" } else { "Démarrer avec Windows (version installée)" },
        installed,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &edit, &reload, &spotify_item, &bar_item, &autostart, &separator, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip(tooltip)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => launcher::show(app),
            "edit" => edit_config(app),
            "reload" => {
                config::reload(app);
                spotify::reload(app);
            }
            "spotify" => spotify::toggle(app),
            "bar" => {
                let _ = bar_item.set_checked(bar::toggle(app));
            }
            "autostart" => toggle_autostart(app, &autostart),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                launcher::show(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn set_tooltip(app: &AppHandle, tooltip: &str) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

fn toggle_autostart(app: &AppHandle, item: &CheckMenuItem<tauri::Wry>) {
    let manager = app.autolaunch();
    let enabled = manager.is_enabled().unwrap_or(false);
    let result = if enabled { manager.disable() } else { manager.enable() };
    if let Err(e) = result {
        eprintln!("[lanceur] démarrage avec Windows : {e}");
    }
    // La case reflète l'état réel, même si l'opération a échoué
    let _ = item.set_checked(manager.is_enabled().unwrap_or(enabled));
}

/// Ouvre apps.json dans l'éditeur associé aux fichiers .json, sinon dans le Bloc-notes.
fn edit_config(app: &AppHandle) {
    let result = config::path(app).and_then(|path| {
        let path = path.to_string_lossy().into_owned();
        shell::open(&path, &[]).or_else(|_| shell::open("notepad.exe", &[path]))
    });
    if let Err(e) = result {
        eprintln!("[lanceur] ouverture de apps.json impossible : {e}");
    }
}
