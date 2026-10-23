//! Showing and hiding the launcher. Rust decides when; the front replays the animation
//! on show and empties the stage before hiding (see the frozen image pitfall in CLAUDE.md).

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Shell::{DefSubclassProc, SetWindowSubclass},
        WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible, SC_KEYMENU, WM_SYSCOMMAND},
    },
};

use crate::config;

/// Launcher dialog open (choosing the charm image): it leaves it the focus without
/// hiding, and the shortcut waits until it is closed
static DIALOG: AtomicBool = AtomicBool::new(false);

pub fn set_dialog(open: bool) {
    DIALOG.store(open, Ordering::SeqCst);
}

/// Right-click on a notification area icon (process of its app, when): the app's own menu
/// takes the focus and opens over the phone, which stays, like the pill (Funsaiki's request,
/// 2026-10-23; before, it went down with the focus)
static APP_MENU: Mutex<Option<(u32, Instant)>> = Mutex::new(None);
/// Time for the app to open its menu
const APP_MENU_DELAY: Duration = Duration::from_secs(2);

pub fn expect_app_menu(pid: u32) {
    *APP_MENU.lock().unwrap() = Some((pid, Instant::now()));
}

/// Last hide request
static LAST_HIDE: Mutex<Option<Instant>> = Mutex::new(None);
/// The shortcut sometimes arrives right after a hide on focus loss (e.g. the Start menu
/// took the focus): it meant to close the launcher, not reopen it.
const REOPEN_GUARD: Duration = Duration::from_millis(400);

/// Log of show/hide decisions, in debug only.
fn debug_log(message: &str) {
    if cfg!(debug_assertions) {
        eprintln!("[pastille] {message}");
    }
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

/// Alt released while the launcher has the focus (end of Alt+Space): Windows puts the window
/// in keyboard menu mode, which holds the mouse: no more hover until the next click.
/// The window has no menu: this mode is ignored. To be called from the main thread.
pub fn disable_key_menu(app: &AppHandle) {
    let Some(hwnd) = window(app).and_then(|w| w.hwnd().ok()) else { return };
    if !unsafe { SetWindowSubclass(hwnd, Some(ignore_key_menu), 1, 0) }.as_bool() {
        eprintln!("[pastille] keyboard menu mode: couldn't intercept");
    }
}

unsafe extern "system" fn ignore_key_menu(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if message == WM_SYSCOMMAND && (wparam.0 as u32 & 0xFFF0) == SC_KEYMENU {
        debug_log("keyboard menu mode (Alt) ignored");
        return LRESULT(0);
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

fn is_visible(w: &WebviewWindow) -> bool {
    w.is_visible().unwrap_or(false)
}

/// The transparent window covers the whole screen the mouse is on (taskbar
/// included): the phone rises from the bottom of the screen and the front scales the stage.
fn cover_cursor_monitor(w: &WebviewWindow) -> tauri::Result<()> {
    let cursor = w.cursor_position()?;
    let Some(monitor) = w.monitor_from_point(cursor.x, cursor.y)? else { return Ok(()) };
    // Position first: when changing screens, Windows adapts the size to the new screen's scale
    w.set_position(*monitor.position())?;
    w.set_size(*monitor.size())?;
    Ok(())
}

/// When hidden, WebView2 can give memory back (requested by Funsaiki); normal level before each
/// show. The phone, and the pastel bar windows when the pill is tucked away.
pub fn set_memory_low(w: &WebviewWindow, low: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    use windows::core::Interface;
    let _ = w.with_webview(move |webview| unsafe {
        let level = if low { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW } else { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL };
        // WebView2 too old (before version 1.0.2210): nothing to do
        if let Ok(core) = webview.controller().CoreWebView2().and_then(|c| c.cast::<ICoreWebView2_19>()) {
            let _ = core.SetMemoryUsageTargetLevel(level);
        }
    });
}

/// The front hides the window with this command, once the stage is emptied (ui.ts)
#[tauri::command]
pub fn hide_window(app: AppHandle) {
    if let Some(w) = window(&app) {
        let _ = w.hide();
        set_memory_low(&w, true);
    }
}

pub fn show(app: &AppHandle) {
    show_page(app, None);
}

/// `page`: what the front opens once the phone has risen ("prefs": the Settings page,
/// from the tray icon menu)
pub fn show_page(app: &AppHandle, page: Option<&str>) {
    let Some(w) = window(app) else { return };
    set_memory_low(&w, false);
    // apps.json changed since the last opening: the front gets the new config before "shown"
    config::reload_if_changed(app);
    config::refresh_games_if_stale(app);
    // Already shown ("Show" menu): don't move
    if !is_visible(&w) {
        if let Err(e) = cover_cursor_monitor(&w) {
            eprintln!("[pastille] couldn't place on the mouse's screen: {e}");
        }
    }
    let _ = w.show();
    let _ = w.set_focus();
    let _ = app.emit("launcher:shown", page);
}

/// First launch (after installation): the phone shows itself, with the
/// welcome. Called by the page once ready: shown during startup, it lost the
/// focus while WebView2 was being created. Once per startup.
#[tauri::command]
pub fn first_show(app: AppHandle) {
    static DONE: AtomicBool = AtomicBool::new(false);
    if config::first_run(&app) && !DONE.swap(true, Ordering::SeqCst) {
        show(&app);
    }
}

/// The front empties the stage, then hides the window itself.
pub fn hide(app: &AppHandle) {
    *LAST_HIDE.lock().unwrap() = Some(Instant::now());
    let _ = app.emit("launcher:hide", ());
}

pub fn toggle(app: &AppHandle) {
    let Some(w) = window(app) else { return };
    if DIALOG.load(Ordering::SeqCst) {
        debug_log("shortcut ignored: dialog open");
        return;
    }
    let just_hidden = LAST_HIDE.lock().unwrap().is_some_and(|t| t.elapsed() < REOPEN_GUARD);
    if is_visible(&w) {
        debug_log("shortcut: hide");
        hide(app);
    } else if just_hidden {
        debug_log("shortcut ignored: the launcher was just hidden");
    } else {
        debug_log("shortcut: show");
        show(app);
    }
}

pub fn on_blur(app: &AppHandle) {
    let Some(w) = window(app) else { return };
    if !is_visible(&w) || DIALOG.load(Ordering::SeqCst) {
        return;
    }
    // In debug, opening the developer tools loses the focus: stay shown
    #[cfg(debug_assertions)]
    {
        if w.is_devtools_open() {
            return;
        }
    }
    let menu = APP_MENU.lock().unwrap().take().filter(|(_, at)| at.elapsed() < APP_MENU_DELAY);
    if let (Some((pid, _)), Ok(hwnd)) = (menu, w.hwnd()) {
        debug_log("focus lost to an app's menu: stay shown");
        watch_app_menu(app, hwnd.0 as isize, pid);
        return;
    }
    debug_log("focus lost: hide");
    hide(app);
}

/// While the app's menu is open, the phone stays. It goes down when one of the app's
/// windows comes forward (an entry of its menu) or when another program takes the focus;
/// back on the phone (a click on it once the menu is closed), the usual focus loss.
fn watch_app_menu(app: &AppHandle, own: isize, pid: u32) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(100));
        let own = HWND(own as *mut _);
        if !unsafe { IsWindowVisible(own) }.as_bool() {
            return;
        }
        let foreground = unsafe { GetForegroundWindow() };
        if foreground == own {
            return;
        }
        // No foreground window for a moment (focus changing hands)
        if foreground.is_invalid() {
            continue;
        }
        let mut owner = 0;
        unsafe { GetWindowThreadProcessId(foreground, Some(&mut owner)) };
        if owner != pid || crate::switch::is_app_window(foreground) {
            debug_log("app's menu closed on a window: hide");
            hide(&app);
            return;
        }
    });
}
