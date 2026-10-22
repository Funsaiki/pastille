//! Icon in the notification area: required, the app doesn't appear in the taskbar.

use std::sync::OnceLock;

use tauri::{
    menu::{ContextMenu, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Wry,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsWindowVisible, SetForegroundWindow};

use crate::{config, i18n::tr, launcher};

/// Show, Settings and Quit: the rest is in the phone's Settings page (prefs.rs,
/// requested by Funsaiki on 2026-10-21)
fn labels() -> [&'static str; 3] {
    [tr("Afficher", "Show"), tr("Paramètres", "Settings"), tr("Quitter", "Quit")]
}

/// Menu entries, relabeled when the language changes
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

/// Tray icon menu, also shown by the pastel bar (with the Windows taskbar hidden, the icon is no
/// longer reachable)
static MENU: OnceLock<Menu<Wry>> = OnceLock::new();

/// At the mouse, attached to the launcher window (hidden): the pastel bar can tuck away
/// while it is open. The choices go through `on_menu_event`, as from the icon.
/// Main thread: the menu waits for the choice.
pub fn popup_menu(app: &AppHandle) {
    let (Some(menu), Some(w)) = (MENU.get(), app.get_webview_window("main")) else { return };
    let before = unsafe { GetForegroundWindow() };
    if let Err(e) = menu.popup(w.as_ref().window()) {
        eprintln!("[pastille] launcher menu: {e}");
    }
    // Windows put the hidden launcher window in the foreground for the menu: the keyboard
    // goes back to the previous window ("Show" then brings the phone back to the front)
    if let Ok(hwnd) = w.hwnd() {
        unsafe {
            if GetForegroundWindow() == hwnd && !IsWindowVisible(hwnd).as_bool() && !before.is_invalid() && before != hwnd {
                let _ = SetForegroundWindow(before);
            }
        }
    }
}

/// Language changed (Settings page): menu and tooltip in the new one
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
