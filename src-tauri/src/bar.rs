//! Barre pastel (demandée par Funsaiki le 16/10/2026) : à la place de la barre des tâches de
//! Windows, une pilule en bas de chaque écran (barre.html, bar.ts), en masquage automatique.
//!
//! Une fenêtre transparente par écran (« bar-0 » pour le principal, puis de gauche à droite),
//! sur toute la largeur de son bord bas et HEIGHT de haut, pour la zone de notification et les
//! menus au-dessus de la pilule. Elle ne prend jamais le focus (la fenêtre au premier plan reste
//! celle de Funsaiki) et laisse passer la souris, sauf sur ce que le front affiche (rectangles de
//! `bar_rects`). Masquée tant que la pilule est rentrée.
//!
//! Un fil relève la souris toutes les 50 ms : au bord bas d'un écran, la pilule de cet écran
//! sort (pas par-dessus le téléphone ni une application en plein écran) ; il dit au front quand
//! la souris est sur la pilule (elle rentre 0,7 s après son départ) et quand on clique ailleurs.
//!
//! Les signaux que Windows envoie à sa barre des tâches (shell_hook.rs) font relire les fenêtres
//! des pilules sorties, et une fenêtre qui demande l'attention (Discord à un nouveau message)
//! fait sortir un instant la pilule de son écran, sa tuile en rose.
//!
//! Seulement si Funsaiki coche « Barre pastel » dans le menu de l'icône (barre.json, à côté de
//! apps.json) ; la barre des tâches de Windows est alors masquée (taskbar.rs).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::mpsc::Receiver;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, IsZoomed,
    PostMessageW, ShowWindowAsync, MONITORINFOF_PRIMARY, SW_MINIMIZE, WM_CLOSE,
};

use crate::shell_hook::{self, Event};
use crate::{bar_preview, launcher, open_windows, switch, taskbar, tray_apps};

/// Hauteur des fenêtres de la barre, en pixels logiques : la pilule, et au-dessus la zone de
/// notification avec un menu ouvert
const HEIGHT: f64 = 520.0;
/// Relevé de la souris
const POLL: Duration = Duration::from_millis(50);

/* ── Réglage : barre.json ── */

#[derive(Default, Serialize, Deserialize)]
struct Settings {
    #[serde(default)]
    active: bool,
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("barre.json"))
}

/// Fichier absent ou illisible : barre coupée
fn settings(app: &AppHandle) -> Settings {
    file(app)
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(text.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

/// Fichier temporaire puis renommage, comme spotify.json
fn save(app: &AppHandle, settings: &Settings) {
    let Some(path) = file(app) else { return };
    let temp = path.with_extension("json.tmp");
    let written = fs::write(&temp, serde_json::to_string_pretty(settings).unwrap_or_default())
        .and_then(|_| fs::rename(&temp, &path));
    if let Err(e) = written {
        eprintln!("[lanceur] barre : {} non enregistré : {e}", path.display());
    }
}

pub fn is_active(app: &AppHandle) -> bool {
    settings(app).active
}

/// Au démarrage. Barre coupée : celle de Windows est réaffichée, au cas où un arrêt brutal du
/// lanceur l'aurait laissée masquée.
pub fn init(app: &AppHandle) {
    if is_active(app) {
        start(app);
    } else {
        taskbar::show_all();
    }
}

/// Case « Barre pastel » du menu de l'icône ; renvoie le nouvel état (d'après la barre, pas
/// d'après barre.json, qui a pu être modifié à la main)
pub fn toggle(app: &AppHandle) -> bool {
    let active = !RUNNING.load(SeqCst);
    save(app, &Settings { active });
    if active { start(app) } else { stop(app) }
    active
}

/* ── Marche et arrêt ── */

/// Par écran : là où la fenêtre prend la souris (pixels de l'écran), pilule sortie, souris dessus
#[derive(Default)]
struct Screen {
    rects: Vec<RECT>,
    out: bool,
    inside: bool,
}

static SCREENS: Mutex<Vec<Screen>> = Mutex::new(Vec::new());
static RUNNING: AtomicBool = AtomicBool::new(false);
/// Dernière fenêtre au premier plan qui n'est pas au lanceur : celle que la barre montre active
static ACTIVE: Mutex<isize> = Mutex::new(0);
/// Fenêtres qui demandent l'attention, jusqu'à ce qu'elles passent au premier plan
static ATTENTION: Mutex<Vec<isize>> = Mutex::new(Vec::new());

fn start(app: &AppHandle) {
    if RUNNING.swap(true, SeqCst) {
        return;
    }
    taskbar::hide();
    let (sender, signals) = std::sync::mpsc::channel();
    shell_hook::start(sender);
    let app = app.clone();
    std::thread::spawn(move || {
        let mut pressed = false;
        while RUNNING.load(SeqCst) {
            tick(&app, &mut pressed, &signals);
            std::thread::sleep(POLL);
        }
    });
}

fn stop(app: &AppHandle) {
    if !RUNNING.swap(false, SeqCst) {
        return;
    }
    for (label, w) in app.webview_windows() {
        if index(&label).is_some() {
            bar_preview::forget(&label);
            let _ = w.destroy();
        }
    }
    SCREENS.lock().unwrap().clear();
    ATTENTION.lock().unwrap().clear();
    shell_hook::stop();
    taskbar::restore();
}

/// À la sortie du lanceur : la barre de Windows revient
pub fn shutdown() {
    if RUNNING.swap(false, SeqCst) {
        taskbar::restore();
    }
}

fn label(screen: usize) -> String {
    format!("bar-{screen}")
}

fn index(label: &str) -> Option<usize> {
    label.strip_prefix("bar-")?.parse().ok()
}

/* ── Écrans et fenêtres de la barre ── */

/// Écrans, le principal d'abord puis de gauche à droite (pixels de l'écran)
fn monitors() -> Vec<RECT> {
    unsafe extern "system" fn visit(monitor: HMONITOR, _dc: HDC, _rect: *mut RECT, lparam: LPARAM) -> BOOL {
        let found = unsafe { &mut *(lparam.0 as *mut Vec<(bool, RECT)>) };
        if let Some((primary, rect)) = monitor_info(monitor) {
            found.push((primary, rect));
        }
        true.into()
    }
    let mut found: Vec<(bool, RECT)> = Vec::new();
    let _ = unsafe { EnumDisplayMonitors(None, None, Some(visit), LPARAM(&mut found as *mut _ as isize)) };
    found.sort_by_key(|(primary, r)| (!primary, r.left, r.top));
    found.into_iter().map(|(_, r)| r).collect()
}

fn monitor_info(monitor: HMONITOR) -> Option<(bool, RECT)> {
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    unsafe { GetMonitorInfoW(monitor, &mut info) }
        .as_bool()
        .then_some((info.dwFlags & MONITORINFOF_PRIMARY != 0, info.rcMonitor))
}

/// Une fenêtre par écran. Écran branché ou débranché : celles en trop sont détruites, les
/// autres rentrées (leur état ne vaut plus : pilule sortie, souris prise) ; une fenêtre qui n'a
/// pas pu être créée est réessayée au relevé suivant.
fn ensure_windows(app: &AppHandle, monitors: &[RECT]) {
    let changed = {
        let mut screens = SCREENS.lock().unwrap();
        let changed = screens.len() != monitors.len();
        if changed {
            screens.clear();
            screens.resize_with(monitors.len(), Screen::default);
        }
        changed
    };
    if changed {
        for (label, w) in app.webview_windows() {
            match index(&label) {
                Some(i) if i >= monitors.len() => {
                    bar_preview::forget(&label);
                    let _ = w.destroy();
                }
                Some(_) => {
                    let _ = w.set_ignore_cursor_events(true);
                    let _ = w.hide();
                    launcher::set_memory_low(&w, true);
                    let _ = app.emit_to(label, "bar:reset", ());
                }
                None => {}
            }
        }
    }
    for (i, monitor) in monitors.iter().enumerate() {
        let label = label(i);
        if app.get_webview_window(&label).is_some() {
            continue;
        }
        match create(app, &label) {
            // Barre coupée pendant la création (`stop`, depuis le fil principal)
            Ok(w) if !RUNNING.load(SeqCst) => {
                let _ = w.destroy();
                return;
            }
            Ok(w) => place(&w, monitor),
            Err(e) => eprintln!("[lanceur] barre : fenêtre {label} impossible : {e}"),
        }
    }
}

fn create(app: &AppHandle, label: &str) -> tauri::Result<WebviewWindow> {
    let w = WebviewWindowBuilder::new(app, label, WebviewUrl::App("barre.html".into()))
        .title("Barre pastel")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()?;
    w.set_ignore_cursor_events(true)?;
    launcher::set_memory_low(&w, true);   // pilule rentrée
    Ok(w)
}

/// Sur toute la largeur du bord bas de l'écran
fn place(w: &WebviewWindow, monitor: &RECT) {
    let scale = w
        .monitor_from_point(monitor.left as f64 + 1.0, monitor.top as f64 + 1.0)
        .ok()
        .flatten()
        .map_or(1.0, |m| m.scale_factor());
    let height = (HEIGHT * scale).round() as i32;
    // Position d'abord : en changeant d'écran, Windows adapte la taille à l'échelle du nouvel écran
    let _ = w.set_position(PhysicalPosition::new(monitor.left, monitor.bottom - height));
    let _ = w.set_size(PhysicalSize::new((monitor.right - monitor.left) as u32, height as u32));
}

/* ── Relevé de la souris ── */

enum Action {
    /// Souris au bord bas, ou un instant pour une fenêtre qui demande l'attention (vrai)
    Reveal(usize, RECT, bool),
    Pointer(usize, bool),
    Outside(usize),
    /// Fenêtres à relire
    Changed(usize),
}

fn tick(app: &AppHandle, pressed_before: &mut bool, signals: &Receiver<Event>) {
    let foreground = remember_foreground();
    // Lus même écran verrouillé : ils s'accumuleraient toute la nuit
    let (changed, peek) = read_signals(signals, foreground);
    let mut cursor = POINT::default();
    // Écran verrouillé, demande de l'UAC : pas de souris
    if unsafe { GetCursorPos(&mut cursor) }.is_err() {
        return;
    }
    let monitors = monitors();
    ensure_windows(app, &monitors);
    // Bouton enfoncé, ou enfoncé depuis le dernier relevé (clic de moins de 50 ms : bit de poids
    // faible, qu'un autre programme peut prendre avant nous ; jamais un faux clic)
    let (mut pressed, mut tapped) = (false, false);
    for key in [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON] {
        let state = unsafe { GetAsyncKeyState(key.0 as i32) };
        pressed |= state < 0;
        tapped |= state & 1 != 0;
    }
    let click = (pressed && !*pressed_before) || tapped;
    *pressed_before = pressed;

    // Décisions sous le verrou, appels à Tauri après (ils passent par le fil principal)
    let mut actions = Vec::new();
    {
        let mut screens = SCREENS.lock().unwrap();
        for (i, monitor) in monitors.iter().enumerate() {
            let Some(screen) = screens.get_mut(i) else { continue };
            let at_edge = contains(monitor, cursor) && cursor.y >= monitor.bottom - 2;
            let peeking = peek == Some(*monitor);
            if (at_edge || peeking) && !screen.out && !blocked(foreground, monitor) {
                screen.out = true;
                actions.push(Action::Reveal(i, *monitor, !at_edge));
            } else if changed && screen.out {
                actions.push(Action::Changed(i));
            }
            // Le bord bas compte : la pilule ne rentre pas tant que la souris y reste
            let inside = screen.out && (at_edge || screen.rects.iter().any(|r| contains(r, cursor)));
            if inside != screen.inside {
                screen.inside = inside;
                actions.push(Action::Pointer(i, inside));
            }
            if click && screen.out && !inside {
                actions.push(Action::Outside(i));
            }
        }
    }
    for action in actions {
        match action {
            Action::Reveal(i, monitor, peek) => {
                let Some(w) = app.get_webview_window(&label(i)) else { continue };
                place(&w, &monitor);
                launcher::set_memory_low(&w, false);
                let _ = w.show();
                let _ = app.emit_to(label(i), "bar:reveal", peek);
            }
            Action::Pointer(i, inside) => {
                if let Some(w) = app.get_webview_window(&label(i)) {
                    let _ = w.set_ignore_cursor_events(!inside);
                }
                let _ = app.emit_to(label(i), "bar:pointer", inside);
            }
            Action::Outside(i) => {
                let _ = app.emit_to(label(i), "bar:outside", ());
            }
            Action::Changed(i) => {
                let _ = app.emit_to(label(i), "bar:windows", ());
            }
        }
    }
}

/// Signaux de Windows depuis le dernier relevé : fenêtres à relire ? Écran d'une fenêtre qui
/// vient de demander l'attention (la pilule y sort un instant) ?
fn read_signals(signals: &Receiver<Event>, foreground: HWND) -> (bool, Option<RECT>) {
    let own = std::process::id();
    let mut changed = false;
    let mut peek = None;
    let mut attention = ATTENTION.lock().unwrap();
    for event in signals.try_iter() {
        match event {
            Event::Changed => changed = true,
            Event::Gone(h) | Event::Activated(h) => {
                attention.retain(|&a| a != h);
                changed = true;
            }
            // Répété tant qu'elle clignote : seul le premier compte
            Event::Flash(h) => {
                let hwnd = HWND(h as *mut _);
                if hwnd != foreground && !attention.contains(&h) && switch::is_app_window(hwnd) && pid_of(hwnd) != own {
                    attention.push(h);
                    peek = monitor_info(unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) }).map(|(_, r)| r);
                    changed = true;
                }
            }
        }
    }
    // Passée au premier plan sans signal
    let before = attention.len();
    attention.retain(|&a| a != foreground.0 as isize);
    (changed || attention.len() != before, peek)
}

fn contains(r: &RECT, p: POINT) -> bool {
    p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
}

fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// Fenêtre au premier plan ; gardée dans ACTIVE si elle n'est pas au lanceur
fn remember_foreground() -> HWND {
    let foreground = unsafe { GetForegroundWindow() };
    if !foreground.is_invalid() && pid_of(foreground) != std::process::id() {
        *ACTIVE.lock().unwrap() = foreground.0 as isize;
    }
    foreground
}

/// Pas par-dessus le téléphone, ni une application en plein écran sur cet écran (jeu, vidéo,
/// F11). Une fenêtre agrandie couvre aussi l'écran (la barre de Funsaiki se masque) : elle,
/// la pilule peut la recouvrir. Le bureau non plus n'est pas un plein écran.
fn blocked(foreground: HWND, monitor: &RECT) -> bool {
    if foreground.is_invalid() {
        return false;
    }
    // Le téléphone affiché ; masqué, sa fenêtre peut rester au premier plan (menu de l'icône)
    if pid_of(foreground) == std::process::id() {
        return unsafe { IsWindowVisible(foreground) }.as_bool();
    }
    let mut r = RECT::default();
    if unsafe { GetWindowRect(foreground, &mut r) }.is_err() {
        return false;
    }
    let covers = r.left <= monitor.left && r.top <= monitor.top && r.right >= monitor.right && r.bottom >= monitor.bottom;
    let mut buf = [0u16; 16];
    let len = unsafe { GetClassNameW(foreground, &mut buf) } as usize;
    let desktop = matches!(String::from_utf16_lossy(&buf[..len]).as_str(), "Progman" | "WorkerW");
    covers && !desktop && !unsafe { IsZoomed(foreground) }.as_bool()
}

/* ── Commandes du front ── */

/// Rectangles où la fenêtre prend la souris : pilule, zone de notification, menu ouvert, en
/// pixels CSS de la fenêtre (x, y, largeur, hauteur). Vide quand la pilule rentre.
#[tauri::command]
pub async fn bar_rects(window: WebviewWindow, rects: Vec<[f64; 4]>) {
    let Some(i) = index(window.label()) else { return };
    let (Ok(origin), Ok(scale)) = (window.inner_position(), window.scale_factor()) else { return };
    let at = |v: f64| (v * scale).round() as i32;
    let rects = rects
        .iter()
        .map(|r| RECT {
            left: origin.x + at(r[0]),
            top: origin.y + at(r[1]),
            right: origin.x + at(r[0] + r[2]),
            bottom: origin.y + at(r[1] + r[3]),
        })
        .collect();
    if let Some(screen) = SCREENS.lock().unwrap().get_mut(i) {
        screen.rects = rects;
    }
}

/// Pilule rentrée : la fenêtre est masquée jusqu'à la prochaine sortie, WebView2 en mémoire basse
/// comme le téléphone masqué
#[tauri::command]
pub async fn bar_hidden(window: WebviewWindow) {
    let Some(i) = index(window.label()) else { return };
    if let Some(screen) = SCREENS.lock().unwrap().get_mut(i) {
        *screen = Screen::default();
    }
    let _ = window.set_ignore_cursor_events(true);
    let _ = window.hide();
    launcher::set_memory_low(&window, true);
}

#[derive(Serialize)]
pub struct BarWindow {
    /// Handle de la fenêtre, renvoyé pour agir (Rust vérifie qu'elle est encore là)
    id: String,
    title: String,
    /// Programme (« Brave Browser »)
    app: String,
    /// Ses fenêtres partagent une tuile
    key: String,
    /// Pas pour les programmes dont le front a déjà l'icône
    icon: Option<crate::icon::Icon>,
    /// Au premier plan
    active: bool,
    /// Demande l'attention (clignote dans la barre de Windows)
    attention: bool,
}

/// Fenêtres de la barre des tâches, sauf celles du lanceur. Toutes, quel que soit leur écran
/// (demande de Funsaiki) : chaque pilule les montre toutes. Relues chaque seconde : les icônes ne
/// sont envoyées que pour les programmes que le front n'a pas déjà (`known`, des `key`).
#[tauri::command]
pub async fn bar_windows(known: Vec<String>) -> Vec<BarWindow> {
    crate::com::com_task(move || Ok(list(&known))).await.unwrap_or_default()
}

fn list(known: &[String]) -> Vec<BarWindow> {
    let active = *ACTIVE.lock().unwrap();
    let attention = ATTENTION.lock().unwrap().clone();
    let own = std::process::id();
    switch::app_windows()
        .into_iter()
        .filter_map(|hwnd| {
            let (path, pid) = switch::process_of(hwnd);
            let path = path?;
            if pid == own {
                return None;
            }
            let title = open_windows::title(hwnd);
            // Une tuile par fenêtre, nommée par son titre
            let lower = path.to_lowercase();
            let hosted = is_hosted(&path);
            let app = if hosted {
                title.clone()
            } else {
                crate::mixer::description(&path).unwrap_or_else(|| {
                    Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
                })
            };
            let key = if hosted { format!("{lower}|{title}") } else { lower };
            let icon = if known.contains(&key) { None } else { crate::icon::of(&path) };
            Some(BarWindow {
                id: (hwnd.0 as isize).to_string(),
                title,
                app,
                key,
                icon,
                active: hwnd.0 as isize == active,
                attention: attention.contains(&(hwnd.0 as isize)),
            })
        })
        .collect()
}

/// Les apps du Store comme Paramètres ou Calculatrice partagent ce programme
pub fn is_hosted(path: &str) -> bool {
    path.to_lowercase().ends_with(r"\applicationframehost.exe")
}

/// Réduite, comme un clic sur la tuile de la fenêtre au premier plan dans la barre de Windows.
/// Sans attendre le programme (il peut ne plus répondre).
#[tauri::command]
pub async fn minimize_window(id: String) -> Result<(), String> {
    crate::blocking(move || {
        let hwnd = open_windows::app_window(&id)?;
        let _ = unsafe { ShowWindowAsync(hwnd, SW_MINIMIZE) };
        Ok(())
    })
    .await?
}

/// Comme son bouton de fermeture : le programme peut demander d'enregistrer
#[tauri::command]
pub async fn close_window(id: String) -> Result<(), String> {
    crate::blocking(move || {
        let hwnd = open_windows::app_window(&id)?;
        unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }.map_err(|e| e.message().trim_end().to_string())
    })
    .await?
}

/// Relance le programme de la fenêtre (le navigateur ouvre une nouvelle fenêtre, par exemple) ;
/// une app du Store par son identifiant
#[tauri::command]
pub async fn new_window(id: String) -> Result<(), String> {
    let target = crate::com::com_task(move || Ok(relaunch_target(&id))).await??;
    crate::blocking(move || crate::shell::open_as_is(&target)).await?
}

/// Avec COM : l'identifiant d'une app hébergée est lu sur sa fenêtre (ApplicationFrameHost,
/// relancé seul, n'ouvrirait rien)
fn relaunch_target(id: &str) -> Result<String, String> {
    let hwnd = open_windows::app_window(id)?;
    let (path, pid) = switch::process_of(hwnd);
    let path = path.ok_or("Programme de la fenêtre introuvable.")?;
    let app_id = if is_hosted(&path) {
        open_windows::window_app_id(hwnd)
    } else if tray_apps::is_packaged(&path) {
        tray_apps::app_id(pid)
    } else {
        return Ok(path);
    };
    app_id
        .map(|id| format!(r"shell:AppsFolder\{id}"))
        .ok_or_else(|| "Identifiant de l'application introuvable.".to_string())
}

/// Bouton Applications : le téléphone monte sur l'écran de la souris (celui de la pilule)
#[tauri::command]
pub fn show_launcher(app: AppHandle) {
    launcher::show(&app);
}

/// Clic droit sur Applications : le menu de l'icône du lanceur (couper la barre pastel,
/// quitter…), que la barre de Windows masquée ne montre plus. Ouvert depuis la boucle du fil
/// principal, pas pendant la commande : le menu attend le choix de Funsaiki.
#[tauri::command]
pub async fn bar_menu(app: AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || crate::tray::popup_menu(&handle));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_screens() {
        assert_eq!(label(1), "bar-1");
        assert_eq!(index("bar-1"), Some(1));
        assert_eq!(index("main"), None);
        assert_eq!(index("bar-x"), None);
        let r = RECT { left: 0, top: 0, right: 10, bottom: 10 };
        assert!(contains(&r, POINT { x: 0, y: 9 }));
        assert!(!contains(&r, POINT { x: 10, y: 5 }), "bord droit exclu");
    }

    /// Diagnostic, lecture seule : cargo test real_bar_windows -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_bar_windows() {
        crate::com::with_com(|| {
            for (i, m) in monitors().iter().enumerate() {
                eprintln!("écran {i} : {},{} -> {},{}", m.left, m.top, m.right, m.bottom);
            }
            let start = std::time::Instant::now();
            for w in list(&[]) {
                eprintln!("  {:<28} {:<40} icône : {}", w.app, w.title.chars().take(40).collect::<String>(), w.icon.is_some());
            }
            for hwnd in switch::app_windows() {
                if switch::process_of(hwnd).0.as_deref().is_some_and(is_hosted) {
                    eprintln!("  hébergée : {} → {:?}", open_windows::title(hwnd), open_windows::window_app_id(hwnd));
                }
            }
            eprintln!("  durée : {:?}", start.elapsed());
        });
    }
}
