//! Pastel bar (requested by Funsaiki on 2026-10-16): instead of the Windows taskbar, a pill
//! at the bottom of each screen (bar.html, bar.ts), auto-hidden.
//!
//! One transparent window per screen ("bar-0" for the primary one, then left to right),
//! across the whole width of its bottom edge and HEIGHT tall, for the notification area and the
//! menus above the pill. It never takes the focus (the foreground window stays
//! Funsaiki's) and lets the mouse through, except over what the front shows (rectangles from
//! `bar_rects`). Hidden while the pill is retracted.
//!
//! A thread polls the mouse every 50 ms: at the bottom edge of a screen, that screen's pill
//! slides out (not over the phone nor over a fullscreen app); it tells the front when
//! the mouse is over the pill (it retracts 0.7 s after the mouse leaves) and on clicks elsewhere.
//!
//! The signals Windows sends to its taskbar (shell_hook.rs) make the pills that are out read
//! their windows again, and a window asking for attention (Discord on a new message)
//! briefly slides out the pill of its screen, its tile in pink.
//!
//! Only if Funsaiki turns on "Pastel bar" in the Settings page (bar.json, next to
//! apps.json, with the pinned apps: bar_pins.rs); the Windows taskbar
//! is then hidden (taskbar.rs).

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
    GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowPlacement, GetWindowRect, IsIconic,
    IsWindowVisible, IsZoomed, PostMessageW, SetWindowPlacement, SetWindowPos, ShowWindow, ShowWindowAsync,
    MONITORINFOF_PRIMARY, SWP_NOACTIVATE, SWP_NOZORDER, SW_MAXIMIZE, SW_MINIMIZE, SW_SHOWNORMAL, WINDOWPLACEMENT, WM_CLOSE,
};

use crate::bar_pins::{self, Pin};
use crate::i18n::tr;
use crate::shell_hook::{self, Event};
use crate::{bar_preview, config, launcher, open_windows, switch, taskbar, tray_apps};

/// Height of the bar windows, in logical pixels: the pill, and above it the notification
/// area with a menu open
const HEIGHT: f64 = 520.0;
/// Mouse polling
const POLL: Duration = Duration::from_millis(50);

/* ── Setting: bar.json (barre.json until 2026-10-22) ── */

#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    active: bool,
    /// Pinned apps, in their order (bar_pins.rs)
    #[serde(default)]
    pub pinned: Vec<Pin>,
}

/// File missing or unreadable: bar off
fn settings(app: &AppHandle) -> Settings {
    config::read_settings(app, "bar.json").unwrap_or_default()
}

fn save(app: &AppHandle, settings: &Settings) {
    if let Err(e) = config::write_settings(app, "bar.json", settings) {
        eprintln!("[pastille] bar: {e}");
    }
}

/// Reads, changes and rewrites bar.json, one change at a time
pub fn update(app: &AppHandle, change: impl FnOnce(&mut Settings)) {
    static WRITING: Mutex<()> = Mutex::new(());
    let _writing = WRITING.lock().unwrap();
    let mut current = settings(app);
    change(&mut current);
    save(app, &current);
}

pub fn is_active(app: &AppHandle) -> bool {
    settings(app).active
}

pub fn pins(app: &AppHandle) -> Vec<Pin> {
    settings(app).pinned
}

/// To the page of each screen
pub fn emit_all(app: &AppHandle, event: &str) {
    for label in app.webview_windows().into_keys().filter(|label| index(label).is_some()) {
        let _ = app.emit_to(label, event, ());
    }
}

/// At startup. Bar off: the Windows taskbar is shown again, in case an abrupt stop of the
/// launcher left it hidden.
pub fn init(app: &AppHandle) {
    if is_active(app) {
        start(app);
    } else {
        taskbar::show_all();
    }
}

/// Pastel bar running (according to the bar, not to bar.json, which may have been edited by hand)
pub fn running() -> bool {
    RUNNING.load(SeqCst)
}

/// "Pastel bar" switch of the Settings page (prefs.rs); returns the new state
pub fn toggle(app: &AppHandle) -> bool {
    let active = !RUNNING.load(SeqCst);
    update(app, |settings| settings.active = active);
    if active { start(app) } else { stop(app) }
    active
}

/* ── Start and stop ── */

/// Per screen: where the window takes the mouse (screen pixels), pill out, mouse over it
#[derive(Default)]
struct Screen {
    rects: Vec<RECT>,
    out: bool,
    inside: bool,
}

static SCREENS: Mutex<Vec<Screen>> = Mutex::new(Vec::new());
static RUNNING: AtomicBool = AtomicBool::new(false);
/// Last foreground window that isn't the launcher's: the one the bar shows as active
static ACTIVE: Mutex<isize> = Mutex::new(0);
/// Windows asking for attention, until they come to the foreground
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

/// When the launcher exits: the Windows taskbar comes back
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

/* ── Screens and bar windows ── */

/// A screen (screen pixels): the whole screen, and its work area (without the taskbar)
struct Display {
    primary: bool,
    area: RECT,
    work: RECT,
}

/// Screens, the primary one first, then left to right
fn displays() -> Vec<Display> {
    unsafe extern "system" fn visit(monitor: HMONITOR, _dc: HDC, _rect: *mut RECT, lparam: LPARAM) -> BOOL {
        let found = unsafe { &mut *(lparam.0 as *mut Vec<Display>) };
        if let Some(display) = monitor_info(monitor) {
            found.push(display);
        }
        true.into()
    }
    let mut found: Vec<Display> = Vec::new();
    let _ = unsafe { EnumDisplayMonitors(None, None, Some(visit), LPARAM(&mut found as *mut _ as isize)) };
    found.sort_by_key(|d| (!d.primary, d.area.left, d.area.top));
    found
}

fn monitors() -> Vec<RECT> {
    displays().into_iter().map(|d| d.area).collect()
}

fn monitor_info(monitor: HMONITOR) -> Option<Display> {
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool().then_some(Display {
        primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
        area: info.rcMonitor,
        work: info.rcWork,
    })
}

/// One window per screen. Screen plugged in or unplugged: the extra ones are destroyed, the
/// others retracted (their state no longer holds: pill out, mouse captured); a window that
/// couldn't be created is retried at the next poll.
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
            // Bar turned off during the creation (`stop`, from the main thread)
            Ok(w) if !RUNNING.load(SeqCst) => {
                let _ = w.destroy();
                return;
            }
            Ok(w) => place(&w, monitor),
            Err(e) => eprintln!("[pastille] bar: couldn't create window {label}: {e}"),
        }
    }
}

fn create(app: &AppHandle, label: &str) -> tauri::Result<WebviewWindow> {
    let w = WebviewWindowBuilder::new(app, label, WebviewUrl::App("bar.html".into()))
        .title(tr("Barre pastel", "Pastel bar"))
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
    launcher::set_memory_low(&w, true);   // pill retracted
    Ok(w)
}

/// Across the whole width of the screen's bottom edge
fn place(w: &WebviewWindow, monitor: &RECT) {
    let scale = w
        .monitor_from_point(monitor.left as f64 + 1.0, monitor.top as f64 + 1.0)
        .ok()
        .flatten()
        .map_or(1.0, |m| m.scale_factor());
    let height = (HEIGHT * scale).round() as i32;
    // Position first: when the window changes screens, Windows adapts its size to the new screen's scale
    let _ = w.set_position(PhysicalPosition::new(monitor.left, monitor.bottom - height));
    let _ = w.set_size(PhysicalSize::new((monitor.right - monitor.left) as u32, height as u32));
}

/* ── Mouse polling ── */

enum Action {
    /// Mouse at the bottom edge, or briefly for a window asking for attention (true)
    Reveal(usize, RECT, bool),
    Pointer(usize, bool),
    Outside(usize),
    /// Windows to read again
    Changed(usize),
}

fn tick(app: &AppHandle, pressed_before: &mut bool, signals: &Receiver<Event>) {
    let foreground = remember_foreground();
    // Read even when the screen is locked: they would pile up all night
    let (changed, peek) = read_signals(signals, foreground);
    let mut cursor = POINT::default();
    // Screen locked, UAC prompt: no mouse
    if unsafe { GetCursorPos(&mut cursor) }.is_err() {
        return;
    }
    let monitors = monitors();
    ensure_windows(app, &monitors);
    // Button down, or pressed since the last poll (click shorter than 50 ms: low-order bit,
    // which another program may consume before us; never a false click)
    let (mut pressed, mut tapped) = (false, false);
    for key in [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON] {
        let state = unsafe { GetAsyncKeyState(key.0 as i32) };
        pressed |= state < 0;
        tapped |= state & 1 != 0;
    }
    let click = (pressed && !*pressed_before) || tapped;
    *pressed_before = pressed;

    // Decisions under the lock, calls to Tauri afterwards (they go through the main thread)
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
            // The bottom edge counts: the pill doesn't retract while the mouse stays there
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

/// Windows signals since the last poll: windows to read again? Screen of a window that
/// just asked for attention (the pill briefly slides out there)?
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
            // Repeated while it flashes: only the first one counts
            Event::Flash(h) => {
                let hwnd = HWND(h as *mut _);
                if hwnd != foreground && !attention.contains(&h) && switch::is_app_window(hwnd) && switch::pid_of(hwnd) != own {
                    attention.push(h);
                    peek = monitor_info(unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) }).map(|d| d.area);
                    changed = true;
                }
            }
        }
    }
    // Came to the foreground without a signal
    let before = attention.len();
    attention.retain(|&a| a != foreground.0 as isize);
    (changed || attention.len() != before, peek)
}

fn contains(r: &RECT, p: POINT) -> bool {
    p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
}


/// Foreground window; kept in ACTIVE if it isn't the launcher's
fn remember_foreground() -> HWND {
    let foreground = unsafe { GetForegroundWindow() };
    if !foreground.is_invalid() && switch::pid_of(foreground) != std::process::id() {
        *ACTIVE.lock().unwrap() = foreground.0 as isize;
    }
    foreground
}

/// Not over the phone, nor over a fullscreen app on this screen (game, video,
/// F11). A maximized window also covers the screen (Funsaiki's taskbar auto-hides): that one
/// the pill may cover. Nor is the desktop a fullscreen app.
fn blocked(foreground: HWND, monitor: &RECT) -> bool {
    if foreground.is_invalid() {
        return false;
    }
    // The phone is shown; when hidden, its window may stay in the foreground (tray icon menu)
    if switch::pid_of(foreground) == std::process::id() {
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

/* ── Front commands ── */

/// Rectangles where the window takes the mouse: pill, notification area, open menu, in
/// CSS pixels of the window (x, y, width, height). Empty when the pill retracts.
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

/// Pill retracted: the window is hidden until the pill slides out again, WebView2 in low memory
/// like the hidden phone
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
    /// Window handle, sent back to act on it (Rust checks it is still there)
    id: String,
    title: String,
    /// Program ("Brave Browser")
    pub app: String,
    /// Its windows share a tile
    key: String,
    /// Program name in lowercase ("brave.exe"): the one of a pinned app.
    /// Empty for hosted apps, which can't be pinned.
    pub exe: String,
    /// Program path (bar_pins::follow), not sent to the front
    #[serde(skip)]
    pub path: String,
    /// Not for programs whose icon the front already has
    icon: Option<crate::icon::Icon>,
    /// In the foreground
    active: bool,
    /// Asks for attention (flashes in the Windows taskbar)
    attention: bool,
}

/// Taskbar windows, except the launcher's. All of them, whatever their screen
/// (requested by Funsaiki): each pill shows them all. Read often: icons are only
/// sent for programs the front doesn't already have (`known`, a list of `key`s).
#[tauri::command]
pub async fn bar_windows(app: AppHandle, known: Vec<String>) -> Vec<BarWindow> {
    let windows = crate::com::com_task(move || Ok(list(&known))).await.unwrap_or_default();
    bar_pins::follow(&app, &windows);
    windows
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
            // One tile per window, named by its title
            let lower = path.to_lowercase();
            let hosted = is_hosted(&path);
            let app = if hosted { title.clone() } else { crate::mixer::program_name(&path) };
            let key = if hosted { format!("{lower}|{title}") } else { lower };
            let icon = if known.contains(&key) { None } else { crate::icon::of(&path) };
            let exe = if hosted { String::new() } else { bar_pins::exe_of(&path).unwrap_or_default() };
            Some(BarWindow {
                id: (hwnd.0 as isize).to_string(),
                title,
                app,
                key,
                exe,
                path,
                icon,
                active: hwnd.0 as isize == active,
                attention: attention.contains(&(hwnd.0 as isize)),
            })
        })
        .collect()
}

/// Store apps such as Windows Settings or Calculator share this program
pub fn is_hosted(path: &str) -> bool {
    path.to_lowercase().ends_with(r"\applicationframehost.exe")
}

/// Minimized, like a click on the foreground window's tile in the Windows taskbar.
/// Without waiting for the program (it may have stopped responding).
#[tauri::command]
pub async fn minimize_window(id: String) -> Result<(), String> {
    crate::blocking(move || {
        let hwnd = open_windows::app_window(&id)?;
        let _ = unsafe { ShowWindowAsync(hwnd, SW_MINIMIZE) };
        Ok(())
    })
    .await?
}

/// Like its close button: the program may ask to save
#[tauri::command]
pub async fn close_window(id: String) -> Result<(), String> {
    crate::blocking(move || {
        let hwnd = open_windows::app_window(&id)?;
        unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }.map_err(|e| e.message().trim_end().to_string())
    })
    .await?
}

/// Launches the window's program again (the browser opens a new window, for example);
/// a Store app by its identifier
#[tauri::command]
pub async fn new_window(id: String) -> Result<(), String> {
    let target = crate::com::com_task(move || Ok(relaunch_target(&id))).await??;
    crate::blocking(move || crate::shell::open_as_is(&target)).await?
}

/// With COM: the identifier of a hosted app is read from its window (ApplicationFrameHost,
/// launched again on its own, would open nothing)
fn relaunch_target(id: &str) -> Result<String, String> {
    let hwnd = open_windows::app_window(id)?;
    let (path, pid) = switch::process_of(hwnd);
    let path = path.ok_or(tr("Programme de la fenêtre introuvable.", "The window's program was not found."))?;
    let app_id = if is_hosted(&path) {
        open_windows::window_app_id(hwnd)
    } else if tray_apps::is_packaged(&path) {
        tray_apps::app_id(pid)
    } else {
        return Ok(path);
    };
    app_id
        .map(|id| format!(r"shell:AppsFolder\{id}"))
        .ok_or_else(|| tr("Identifiant de l'application introuvable.", "The app's identifier was not found.").to_string())
}

/// Number of screens: "Move to the other screen" in a window's menu if there are several
#[tauri::command]
pub fn bar_screens() -> usize {
    monitors().len()
}

/// Window moved to the next screen (the other screen when there are two), at the same place in the
/// work area, then brought to the front. Maximized, it is restored to its normal size on the
/// new screen then maximized again (moving a maximized window confuses some apps);
/// minimized, it opens on the new screen.
#[tauri::command]
pub async fn move_window(id: String) -> Result<(), String> {
    crate::blocking(move || {
        let hwnd = open_windows::app_window(&id)?;
        let displays = displays();
        let here = monitor_info(unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) }).ok_or(tr("Écran de la fenêtre introuvable.", "The window's screen was not found."))?;
        let i = displays.iter().position(|d| d.area == here.area).unwrap_or(0);
        if displays.len() < 2 {
            return Err(tr("Il n'y a qu'un écran.", "There is only one screen.").to_string());
        }
        let to = &displays[(i + 1) % displays.len()];
        let error = |e: windows::core::Error| e.message().trim_end().to_string();
        unsafe {
            let zoomed = IsZoomed(hwnd).as_bool();
            if zoomed || IsIconic(hwnd).as_bool() {
                // Normal placement, in coordinates of the primary screen's work area
                let mut placement = WINDOWPLACEMENT { length: size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
                GetWindowPlacement(hwnd, &mut placement).map_err(error)?;
                let primary = displays.iter().find(|d| d.primary).unwrap_or(&displays[0]);
                let (dx, dy) = (primary.work.left - primary.area.left, primary.work.top - primary.area.top);
                let normal = offset(placement.rcNormalPosition, dx, dy);
                placement.rcNormalPosition = offset(shift(normal, &here.work, &to.work), -dx, -dy);
                if zoomed {
                    placement.showCmd = SW_SHOWNORMAL.0 as u32;
                }
                SetWindowPlacement(hwnd, &placement).map_err(error)?;
                if zoomed {
                    let _ = ShowWindow(hwnd, SW_MAXIMIZE);
                }
            } else {
                let mut r = RECT::default();
                GetWindowRect(hwnd, &mut r).map_err(error)?;
                let r = shift(r, &here.work, &to.work);
                SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE)
                    .map_err(error)?;
            }
        }
        switch::bring(hwnd);
        Ok(())
    })
    .await?
}

fn offset(r: RECT, dx: i32, dy: i32) -> RECT {
    RECT { left: r.left + dx, top: r.top + dy, right: r.right + dx, bottom: r.bottom + dy }
}

/// Same place relative to the work area; larger than the new one, it is brought
/// back inside, without going out (16 px margin: the invisible borders of Windows 11 windows)
fn shift(r: RECT, from: &RECT, to: &RECT) -> RECT {
    const MARGIN: i32 = 16;
    let width = (r.right - r.left).min(to.right - to.left);
    let height = (r.bottom - r.top).min(to.bottom - to.top);
    let left = (to.left + r.left - from.left).clamp(to.left - MARGIN, to.right - width + MARGIN);
    let top = (to.top + r.top - from.top).clamp(to.top - MARGIN, to.bottom - height + MARGIN);
    RECT { left, top, right: left + width, bottom: top + height }
}

/// Apps button: the phone rises on the mouse's screen (the pill's one)
#[tauri::command]
pub fn show_launcher(app: AppHandle) {
    launcher::show(&app);
}

/// Right-click on Apps: the launcher's tray icon menu (Show, Settings, Quit),
/// which the hidden Windows taskbar no longer shows. Opened from the main thread's
/// loop, not during the command: the menu waits for Funsaiki's choice.
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

    #[test]
    fn shift_to_other_screen() {
        let left = RECT { left: 0, top: 0, right: 1920, bottom: 1080 };
        let right = RECT { left: 1920, top: 0, right: 3840, bottom: 1080 };
        let window = RECT { left: 100, top: 50, right: 1100, bottom: 750 };
        let moved = shift(window, &left, &right);
        assert_eq!((moved.left, moved.top, moved.right, moved.bottom), (2020, 50, 3020, 750));
        // Snapped to the left (7 px invisible border): kept as is
        let snapped = shift(RECT { left: -7, top: 0, right: 967, bottom: 1087 }, &left, &right);
        assert_eq!((snapped.left, snapped.right), (1913, 2887));
        // Larger than the destination screen: brought inside
        let small = RECT { left: 3840, top: 0, right: 5120, bottom: 720 };
        let big = shift(RECT { left: 0, top: 0, right: 1920, bottom: 1080 }, &left, &small);
        assert_eq!((big.left, big.right - big.left, big.bottom - big.top), (3840, 1280, 720));
    }

    #[test]
    fn settings_file() {
        // bar.json from before pins
        let old: Settings = serde_json::from_str(r#"{ "active": true }"#).unwrap();
        assert!(old.active && old.pinned.is_empty());
        let text = r#"{ "active": false, "pinned": [{ "exe": "discord.exe", "name": "Discord", "path": "C:/Discord.exe" }] }"#;
        let file: Settings = serde_json::from_str(text).unwrap();
        assert_eq!(file.pinned[0].exe, "discord.exe");
    }

    /// Diagnostic, read-only: cargo test real_bar_windows -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_bar_windows() {
        crate::com::with_com(|| {
            for (i, m) in monitors().iter().enumerate() {
                eprintln!("screen {i}: {},{} -> {},{}", m.left, m.top, m.right, m.bottom);
            }
            let start = std::time::Instant::now();
            for w in list(&[]) {
                eprintln!("  {:<28} {:<40} icon: {}", w.app, w.title.chars().take(40).collect::<String>(), w.icon.is_some());
            }
            for hwnd in switch::app_windows() {
                if switch::process_of(hwnd).0.as_deref().is_some_and(is_hosted) {
                    eprintln!("  hosted: {} → {:?}", open_windows::title(hwnd), open_windows::window_app_id(hwnd));
                }
            }
            eprintln!("  time: {:?}", start.elapsed());
        });
    }
}
