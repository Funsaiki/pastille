//! Icône dans la zone de notification : indispensable, l'app n'apparaît pas dans la barre des tâches.

use std::sync::OnceLock;

use tauri::{
    menu::{ContextMenu, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Wry,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsWindowVisible, SetForegroundWindow};

use crate::{config, i18n::tr, launcher};

/// Afficher, Paramètres et Quitter : le reste est dans la page Paramètres du téléphone (prefs.rs,
/// demande de Funsaiki du 21/10/2026)
fn labels() -> [&'static str; 3] {
    [tr("Afficher", "Show"), tr("Paramètres", "Settings"), tr("Quitter", "Quit")]
}

/// Entrées du menu, renommées quand la langue change
static ITEMS: OnceLock<[MenuItem<Wry>; 3]> = OnceLock::new();

pub fn create(app: &AppHandle, tooltip: &str) -> tauri::Result<()> {
    let [show, prefs, quit] = labels();
    let show = MenuItem::with_id(app, "show", show, true, None::<&str>)?;
    let prefs = MenuItem::with_id(app, "prefs", prefs, true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", quit, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &prefs, &separator, &quit])?;
    let _ = MENU.set(menu.clone());
    let _ = ITEMS.set([show, prefs, quit]);

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip(tooltip)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => launcher::show(app),
            "prefs" => launcher::show_page(app, Some("prefs")),
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

/// Menu de l'icône, aussi montré par la barre pastel (la barre de Windows masquée, l'icône n'est
/// plus accessible)
static MENU: OnceLock<Menu<Wry>> = OnceLock::new();

/// À la souris, attaché à la fenêtre du lanceur (masquée) : la barre pastel peut rentrer
/// pendant qu'il est ouvert. Les choix passent par `on_menu_event`, comme depuis l'icône.
/// Fil principal : le menu attend le choix.
pub fn popup_menu(app: &AppHandle) {
    let (Some(menu), Some(w)) = (MENU.get(), app.get_webview_window("main")) else { return };
    let before = unsafe { GetForegroundWindow() };
    if let Err(e) = menu.popup(w.as_ref().window()) {
        eprintln!("[lanceur] menu du lanceur : {e}");
    }
    // Windows a mis la fenêtre du lanceur, masquée, au premier plan pour le menu : le clavier
    // revient à la fenêtre d'avant (« Afficher » remet ensuite le téléphone devant)
    if let Ok(hwnd) = w.hwnd() {
        unsafe {
            if GetForegroundWindow() == hwnd && !IsWindowVisible(hwnd).as_bool() && !before.is_invalid() && before != hwnd {
                let _ = SetForegroundWindow(before);
            }
        }
    }
}

/// Langue changée (page Paramètres) : menu et infobulle dans la nouvelle
pub fn relabel(app: &AppHandle) {
    if let Some(items) = ITEMS.get() {
        for (item, label) in items.iter().zip(labels()) {
            let _ = item.set_text(label);
        }
    }
    let tooltip = config::tooltip(&app.state::<config::SharedConfig>().lock().unwrap());
    set_tooltip(app, &tooltip);
}

pub fn set_tooltip(app: &AppHandle, tooltip: &str) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(tooltip));
    }
}
