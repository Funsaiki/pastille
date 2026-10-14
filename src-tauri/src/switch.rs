//! Basculer vers une application déjà ouverte au lieu d'en lancer une autre (demande de
//! Funsaiki) : « Code » ou « Navigateur » ramènent leur fenêtre au premier plan. Seulement pour
//! un .exe lancé sans arguments (avec arguments, ex. un projet précis, on relance) ; Ctrl+clic
//! lance quand même une nouvelle fenêtre.

use std::path::Path;

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, SetForegroundWindow, ShowWindow, GWL_EXSTYLE, GW_OWNER, SW_RESTORE, WS_EX_TOOLWINDOW,
};

use crate::mixer::exe_path;

/// `target` : cible après développement des variables d'environnement. Vrai si une fenêtre de
/// ce programme a été ramenée au premier plan.
pub fn focus_existing(target: &str) -> bool {
    let exe = target.replace('/', "\\");
    if !exe.to_lowercase().ends_with(".exe") || !Path::new(&exe).is_absolute() {
        return false;
    }
    let Some(hwnd) = find_window(&exe) else { return false };
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        // Permis : le lanceur, qui a le focus, appartient au processus au premier plan
        SetForegroundWindow(hwnd).as_bool()
    }
}

struct Search {
    exe: String,
    found: Option<HWND>,
}

/// Première fenêtre du programme dans l'ordre d'empilement : la plus récemment active
fn find_window(exe: &str) -> Option<HWND> {
    let mut search = Search { exe: exe.to_lowercase(), found: None };
    // EnumWindows signale une « erreur » quand visit arrête l'énumération : ignorée
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize)) };
    search.found
}

unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = unsafe { &mut *(lparam.0 as *mut Search) };
    if is_app_window(hwnd) && process_of(hwnd).is_some_and(|path| path.to_lowercase() == search.exe) {
        search.found = Some(hwnd);
        return false.into();
    }
    true.into()
}

/// Fenêtre de la barre des tâches : visible (ou réduite), sans propriétaire, pas une fenêtre
/// outil, avec un titre, et pas masquée par Windows (sur un autre bureau virtuel, par exemple)
fn is_app_window(hwnd: HWND) -> bool {
    unsafe {
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&mut cloaked as *mut u32).cast(), 4);
        IsWindowVisible(hwnd).as_bool()
            && GetWindow(hwnd, GW_OWNER).is_err()
            && GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW.0 as isize == 0
            && GetWindowTextLengthW(hwnd) > 0
            && cloaked == 0
    }
}

fn process_of(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    exe_path(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ne jamais appeler focus_existing ici : il changerait la fenêtre au premier plan.
    #[test]
    fn only_absolute_exe_targets() {
        assert!(!focus_existing("wt.exe"), "nom seul : pas de chemin à comparer");
        assert!(!focus_existing("steam://open/main"));
        assert!(!focus_existing(r"C:\Users"));
    }

    /// Diagnostic, lecture seule : cargo test real_windows -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_windows() {
        for exe in [
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
            &format!(r"{}\Programs\Microsoft VS Code\Code.exe", std::env::var("LOCALAPPDATA").unwrap()),
        ] {
            eprintln!("{exe} : fenêtre ouverte = {}", find_window(exe).is_some());
        }
    }
}
