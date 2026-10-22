//! Windows taskbar hidden while the pastel bar runs (requested by Funsaiki: never see it
//! again, even with the mouse at the bottom). No Windows setting removes it: its
//! windows (`Shell_TrayWnd`, and `Shell_SecondaryTrayWnd` per secondary screen) are hidden,
//! and hidden again as soon as Windows shows them (`EVENT_OBJECT_SHOW` hook, plus a check every
//! 250 ms for a restarted Explorer). Trial on 2026-10-16: never visible.
//! Nothing in the registry; they come back on stop (`restore`), and at startup if the pastel
//! bar is off. Abrupt stop of the launcher (crash, Task Manager): a watchdog process
//! shows them again (`spawn_guard`, requested by Funsaiki). Meanwhile, Pastille also holds
//! the notification area (tray_host.rs), on the same thread.

use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::Mutex;
use std::thread::JoinHandle;

use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{GetCurrentThreadId, OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, FindWindowExW, GetClassNameW, GetMessageW, IsWindowVisible, KillTimer, PeekMessageW,
    PostThreadMessageW, SetTimer, ShowWindow, ShowWindowAsync, EVENT_OBJECT_SHOW, MSG, OBJID_WINDOW, PM_NOREMOVE,
    SW_HIDE, SW_SHOWNA, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_QUIT,
};

/// Thread that keeps the bars hidden: (thread identifier, to stop it; the thread)
static WATCH: Mutex<Option<(u32, JoinHandle<()>)>> = Mutex::new(None);

/// Hides the Windows bars until `restore`
pub fn hide() {
    let mut watch = WATCH.lock().unwrap();
    if watch.is_some() {
        return;
    }
    spawn_guard();
    let (sender, receiver) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        // Message queue created before handing out the identifier: `restore` can post WM_QUIT to it
        let _ = unsafe { PeekMessageW(&mut MSG::default(), None, 0, 0, PM_NOREMOVE) };
        let _ = sender.send(unsafe { GetCurrentThreadId() });
        unsafe {
            for_each_bar(|bar| {
                let _ = ShowWindow(bar, SW_HIDE);
            });
            let hook = SetWinEventHook(
                EVENT_OBJECT_SHOW,
                EVENT_OBJECT_SHOW,
                None,
                Some(on_show),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
            let timer = SetTimer(None, 0, 250, Some(on_tick));
            crate::tray_host::start();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                DispatchMessageW(&msg);
            }
            crate::tray_host::stop();
            let _ = KillTimer(None, timer);
            if !hook.is_invalid() {
                let _ = UnhookWinEvent(hook);
            }
        }
        show_all();
    });
    if let Ok(id) = receiver.recv() {
        *watch = Some((id, thread));
    }
}

/// Shows the Windows bars again (pastel bar or launcher stopped)
pub fn restore() {
    let Some((id, thread)) = WATCH.lock().unwrap().take() else { return };
    if unsafe { PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)) }.is_ok() {
        let _ = thread.join();
    } else {
        show_all();
    }
}

/// Argument of the watchdog process, followed by the launcher's identifier
const GUARD: &str = "--garde-barre";

/// Safety net: a second process of the launcher (the same exe), which only waits for the
/// first one to end to show the Windows taskbar again. One per launcher, started the first
/// time the Windows taskbar is hidden.
fn spawn_guard() {
    static SPAWNED: AtomicBool = AtomicBool::new(false);
    if SPAWNED.swap(true, SeqCst) {
        return;
    }
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let spawned = std::env::current_exe().and_then(|exe| {
        Command::new(exe)
            .args([GUARD, &std::process::id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
    });
    if let Err(e) = spawned {
        eprintln!("[pastille] bar: couldn't start the watchdog process: {e}");
        SPAWNED.store(false, SeqCst);
    }
}

/// Called by `run` before anything else: true for the watchdog process, which waits for the launcher
/// to end, shows the Windows taskbar again (no effect if the launcher already gave it back) and exits
pub fn guard() -> bool {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some(GUARD) {
        return false;
    }
    if let Some(pid) = args.next().and_then(|p| p.parse::<u32>().ok()) {
        if let Ok(process) = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
            unsafe {
                WaitForSingleObject(process, INFINITE);
                let _ = CloseHandle(process);
            }
        }
    }
    show_all();
    true
}

/// No effect on a bar already shown. Without waiting for Explorer: if it stops responding,
/// the launcher doesn't freeze (called from the main thread).
pub fn show_all() {
    for_each_bar(|bar| unsafe {
        let _ = ShowWindowAsync(bar, SW_SHOWNA);
    });
}

fn is_bar(hwnd: HWND) -> bool {
    let mut buf = [0u16; 32];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
    matches!(String::from_utf16_lossy(&buf[..len]).as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
}

/// Asked of Windows by their class, without going through every window (326 on Funsaiki's
/// PC: 0.25 ms per check, versus 7 µs). Not Pastille's "Shell_TrayWnd".
fn for_each_bar(mut f: impl FnMut(HWND)) {
    for class in [w!("Shell_TrayWnd"), w!("Shell_SecondaryTrayWnd")] {
        let mut after = None;
        while let Ok(bar) = unsafe { FindWindowExW(None, after, class, None) } {
            if !crate::tray_host::own(bar) {
                f(bar);
            }
            after = Some(bar);
        }
    }
}

/// Windows shows a bar again: hidden again before it is drawn
unsafe extern "system" fn on_show(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    if object == OBJID_WINDOW.0 && child == 0 && is_bar(hwnd) {
        let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
    }
}

/// Bar back without an event (Explorer restarted, new screen); Pastille's notification area
/// fallen behind the Windows one
unsafe extern "system" fn on_tick(_hwnd: HWND, _msg: u32, _id: usize, _time: u32) {
    for_each_bar(|bar| unsafe {
        if IsWindowVisible(bar).as_bool() {
            let _ = ShowWindow(bar, SW_HIDE);
        }
    });
    crate::tray_host::keep_first();
}
