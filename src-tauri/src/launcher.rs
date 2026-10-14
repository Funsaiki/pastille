//! Affichage et masquage du lanceur. Rust décide quand ; le front rejoue l'animation
//! à l'affichage et vide la scène avant de masquer (voir « Image figée » dans CLAUDE.md).

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Shell::{DefSubclassProc, SetWindowSubclass},
        WindowsAndMessaging::{SC_KEYMENU, WM_SYSCOMMAND},
    },
};

use crate::config;

/// Dernière demande de masquage
static LAST_HIDE: Mutex<Option<Instant>> = Mutex::new(None);
/// Le raccourci arrive parfois juste après un masquage par perte de focus (ex. le menu Démarrer
/// a pris le focus) : il voulait fermer le lanceur, pas le rouvrir.
const REOPEN_GUARD: Duration = Duration::from_millis(400);

/// Journal des décisions d'affichage, en debug seulement.
fn debug_log(message: &str) {
    if cfg!(debug_assertions) {
        eprintln!("[lanceur] {message}");
    }
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

/// Alt relâché alors que le lanceur a le focus (fin d'Alt+Espace) : Windows passe la fenêtre
/// en mode menu clavier, qui retient la souris : plus aucun survol jusqu'au prochain clic.
/// La fenêtre n'a pas de menu : ce mode est ignoré. À appeler depuis le fil principal.
pub fn disable_key_menu(app: &AppHandle) {
    let Some(hwnd) = window(app).and_then(|w| w.hwnd().ok()) else { return };
    if !unsafe { SetWindowSubclass(hwnd, Some(ignore_key_menu), 1, 0) }.as_bool() {
        eprintln!("[lanceur] mode menu clavier : interception impossible");
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
        debug_log("mode menu clavier (Alt) ignoré");
        return LRESULT(0);
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

fn is_visible(w: &WebviewWindow) -> bool {
    w.is_visible().unwrap_or(false)
}

/// La fenêtre, transparente, couvre tout l'écran où se trouve la souris (barre des tâches
/// comprise) : le téléphone monte du bas de l'écran et le front met la scène à l'échelle.
fn cover_cursor_monitor(w: &WebviewWindow) -> tauri::Result<()> {
    let cursor = w.cursor_position()?;
    let Some(monitor) = w.monitor_from_point(cursor.x, cursor.y)? else { return Ok(()) };
    // Position d'abord : en changeant d'écran, Windows adapte la taille à l'échelle du nouvel écran
    w.set_position(*monitor.position())?;
    w.set_size(*monitor.size())?;
    Ok(())
}

pub fn show(app: &AppHandle) {
    let Some(w) = window(app) else { return };
    // apps.json modifié depuis la dernière ouverture : le front reçoit la nouvelle config avant « shown »
    config::reload_if_changed(app);
    config::refresh_games_if_stale(app);
    // Déjà affiché (menu « Afficher ») : ne pas bouger
    if !is_visible(&w) {
        if let Err(e) = cover_cursor_monitor(&w) {
            eprintln!("[lanceur] placement sur l'écran de la souris impossible : {e}");
        }
    }
    let _ = w.show();
    let _ = w.set_focus();
    let _ = app.emit("launcher:shown", ());
}

/// Le front vide la scène, puis masque la fenêtre lui-même.
pub fn hide(app: &AppHandle) {
    *LAST_HIDE.lock().unwrap() = Some(Instant::now());
    let _ = app.emit("launcher:hide", ());
}

pub fn toggle(app: &AppHandle) {
    let Some(w) = window(app) else { return };
    let just_hidden = LAST_HIDE.lock().unwrap().is_some_and(|t| t.elapsed() < REOPEN_GUARD);
    if is_visible(&w) {
        debug_log("raccourci : masquer");
        hide(app);
    } else if just_hidden {
        debug_log("raccourci ignoré : le lanceur vient d'être masqué");
    } else {
        debug_log("raccourci : afficher");
        show(app);
    }
}

pub fn on_blur(app: &AppHandle) {
    let Some(w) = window(app) else { return };
    if !is_visible(&w) {
        return;
    }
    // En debug, ouvrir les outils de développement fait perdre le focus : rester affiché
    #[cfg(debug_assertions)]
    {
        if w.is_devtools_open() {
            return;
        }
    }
    debug_log("perte de focus : masquer");
    hide(app);
}
