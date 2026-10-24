//! Switch to an app already open instead of launching another one (requested by
//! Funsaiki): "Code" or "Browser" bring their window to the foreground. Only for
//! an .exe launched without arguments (with arguments, e.g. a given project, it relaunches);
//! Ctrl+click still launches a new window.

use std::path::Path;

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, SetForegroundWindow, ShowWindow, GWL_EXSTYLE, GW_OWNER, SW_RESTORE, WS_EX_TOOLWINDOW,
};

use crate::mixer::exe_path;

/// `target`: target after expanding environment variables. True if a window of
/// this program was brought to the foreground.
pub fn focus_existing(target: &str) -> bool {
    let exe = target.replace('/', "\\");
    if !exe.to_lowercase().ends_with(".exe") || !Path::new(&exe).is_absolute() {
        return false;
    }
    let Some(hwnd) = find_window(&exe) else { return false };
    bring(hwnd)
}

/// Window to the foreground, restored if it was minimized (also for open_windows.rs)
pub fn bring(hwnd: HWND) -> bool {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        // Allowed: the launcher, which has the focus, belongs to the foreground process
        SetForegroundWindow(hwnd).as_bool()
    }
}

/// Taskbar windows, the most recently active first (open_windows.rs)
pub fn app_windows() -> Vec<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let found = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
        if is_app_window(hwnd) {
            found.push(hwnd);
        }
        true.into()
    }
    let mut found = Vec::new();
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut found as *mut Vec<HWND> as isize)) };
    found
}

struct Search {
    exe: String,
    found: Option<HWND>,
}

/// First window of the program in z-order: the most recently active
fn find_window(exe: &str) -> Option<HWND> {
    let mut search = Search { exe: exe.to_lowercase(), found: None };
    // EnumWindows reports an "error" when visit stops the enumeration: ignored
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize)) };
    search.found
}

unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = unsafe { &mut *(lparam.0 as *mut Search) };
    if is_app_window(hwnd) && process_of(hwnd).0.is_some_and(|path| path.to_lowercase() == search.exe) {
        search.found = Some(hwnd);
        return false.into();
    }
    true.into()
}

/// Taskbar window: visible (or minimized), without owner, not a tool
/// window, with a title, and not cloaked by Windows (on another virtual desktop, for example)
pub fn is_app_window(hwnd: HWND) -> bool {
    unsafe {
        // Cloaking last: it asks the window manager, the other tests rule out most windows
        IsWindowVisible(hwnd).as_bool()
            && GetWindow(hwnd, GW_OWNER).is_err()
            && GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW.0 as isize == 0
            && GetWindowTextLengthW(hwnd) > 0
            && {
                let mut cloaked = 0u32;
                let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&mut cloaked as *mut u32).cast(), 4);
                cloaked == 0
            }
    }
}

/// (program, process id) of a window
pub fn process_of(hwnd: HWND) -> (Option<String>, u32) {
    let pid = pid_of(hwnd);
    (exe_path(pid), pid)
}

/// Process id of a window (0 if it is gone)
pub fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

#[cfg(test)]
mod tests {
    use super::*;

    // Never call focus_existing here: it would change the foreground window.
    #[test]
    fn only_absolute_exe_targets() {
        assert!(!focus_existing("wt.exe"), "name only: no path to compare");
        assert!(!focus_existing("steam://open/main"));
        assert!(!focus_existing(r"C:\Users"));
    }

    /// Diagnostic, read-only: cargo test real_windows -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_windows() {
        for exe in [
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
            &format!(r"{}\Programs\Microsoft VS Code\Code.exe", std::env::var("LOCALAPPDATA").unwrap()),
        ] {
            eprintln!("{exe}: window open = {}", find_window(exe).is_some());
        }
    }
}
