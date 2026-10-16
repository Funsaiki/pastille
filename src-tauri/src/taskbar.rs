//! Barre des tâches de Windows masquée tant que la barre pastel tourne (demande de Funsaiki : ne
//! plus la voir du tout, même souris en bas). Aucun réglage de Windows ne la supprime : ses
//! fenêtres (`Shell_TrayWnd`, et `Shell_SecondaryTrayWnd` par écran secondaire) sont masquées,
//! et recachées dès que Windows les réaffiche (crochet `EVENT_OBJECT_SHOW`, plus un relevé toutes
//! les 250 ms pour un Explorateur relancé). Essai du 16/10/2026 chez Funsaiki : jamais visible.
//! Rien dans le registre ; elles reviennent à l'arrêt (`restore`), et au démarrage si la barre
//! pastel est coupée (après un arrêt brutal du lanceur, elles seraient restées masquées).

use std::sync::Mutex;
use std::thread::JoinHandle;

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, FindWindowExW, GetClassNameW, GetMessageW, IsWindowVisible, KillTimer, PostThreadMessageW,
    SetTimer, ShowWindow, EVENT_OBJECT_SHOW, MSG, OBJID_WINDOW, SW_HIDE, SW_SHOWNA, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_QUIT,
};

/// Fil qui garde les barres masquées : (identifiant du fil, pour l'arrêter ; le fil)
static WATCH: Mutex<Option<(u32, JoinHandle<()>)>> = Mutex::new(None);

/// Masque les barres de Windows jusqu'à `restore`
pub fn hide() {
    let mut watch = WATCH.lock().unwrap();
    if watch.is_some() {
        return;
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
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
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                DispatchMessageW(&msg);
            }
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

/// Réaffiche les barres de Windows (arrêt de la barre pastel ou du lanceur)
pub fn restore() {
    let Some((id, thread)) = WATCH.lock().unwrap().take() else { return };
    let _ = unsafe { PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)) };
    let _ = thread.join();
}

/// Sans effet sur une barre déjà affichée
pub fn show_all() {
    for_each_bar(|bar| unsafe {
        let _ = ShowWindow(bar, SW_SHOWNA);
    });
}

fn is_bar(hwnd: HWND) -> bool {
    let mut buf = [0u16; 32];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
    matches!(String::from_utf16_lossy(&buf[..len]).as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
}

/// Demandées à Windows par leur classe, sans passer en revue toutes les fenêtres (326 chez
/// Funsaiki : 0,25 ms à chaque relevé, contre 7 µs)
fn for_each_bar(mut f: impl FnMut(HWND)) {
    for class in [w!("Shell_TrayWnd"), w!("Shell_SecondaryTrayWnd")] {
        let mut after = None;
        while let Ok(bar) = unsafe { FindWindowExW(None, after, class, None) } {
            f(bar);
            after = Some(bar);
        }
    }
}

/// Windows réaffiche une barre : recachée avant d'être dessinée
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

/// Barre réapparue sans événement (Explorateur relancé, nouvel écran)
unsafe extern "system" fn on_tick(_hwnd: HWND, _msg: u32, _id: usize, _time: u32) {
    for_each_bar(|bar| unsafe {
        if IsWindowVisible(bar).as_bool() {
            let _ = ShowWindow(bar, SW_HIDE);
        }
    });
}
