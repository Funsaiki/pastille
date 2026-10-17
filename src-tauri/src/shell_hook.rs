//! Signaux que Windows envoie à sa barre des tâches, pour la barre pastel (demande de Funsaiki) :
//! fenêtre ouverte ou fermée, activée, titre changé, et fenêtre qui clignote pour attirer
//! l'attention (Discord à un nouveau message, par exemple). Une fenêtre invisible inscrite par
//! `RegisterShellHookWindow`, sur un fil à part ; les signaux sont passés à bar.rs par un canal.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
use std::sync::mpsc::Sender;
use std::sync::Mutex;
use std::thread::JoinHandle;

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DeregisterShellHookWindow, DestroyWindow, DispatchMessageW, GetMessageW,
    PeekMessageW, PostThreadMessageW, RegisterClassW, RegisterShellHookWindow, RegisterWindowMessageW, MSG,
    PM_NOREMOVE, WINDOW_EX_STYLE, WM_QUIT, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};

/// Fenêtres par leur handle (un HWND ne passe pas d'un fil à l'autre)
pub enum Event {
    /// Fenêtre ouverte, titre changé… : la liste est à relire
    Changed,
    /// Fenêtre fermée ou masquée
    Gone(isize),
    Activated(isize),
    /// Elle demande l'attention (`FlashWindowEx`)
    Flash(isize),
}

// Codes des signaux (HSHELL_…)
const WINDOW_CREATED: usize = 1;
const WINDOW_DESTROYED: usize = 2;
const WINDOW_ACTIVATED: usize = 4;
const REDRAW: usize = 6;
const WINDOW_REPLACED: usize = 13;
const RUDE_APP_ACTIVATED: usize = 0x8004;
const FLASH: usize = 0x8006;

/// Fil qui reçoit les signaux : (identifiant, pour l'arrêter ; le fil)
static WATCH: Mutex<Option<(u32, JoinHandle<()>)>> = Mutex::new(None);
/// Message « SHELLHOOK », propre à chaque session de Windows
static SHELL_MESSAGE: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static SENDER: RefCell<Option<Sender<Event>>> = const { RefCell::new(None) };
}

/// Jusqu'à `stop`
pub fn start(sender: Sender<Event>) {
    let mut watch = WATCH.lock().unwrap();
    if watch.is_some() {
        return;
    }
    let (id_sender, id_receiver) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        // File de messages créée avant de donner l'identifiant : `stop` peut y poster WM_QUIT
        let _ = unsafe { PeekMessageW(&mut MSG::default(), None, 0, 0, PM_NOREMOVE) };
        let _ = id_sender.send(unsafe { GetCurrentThreadId() });
        SENDER.with(|s| *s.borrow_mut() = Some(sender));
        let Some(hwnd) = create_window() else { return };
        unsafe {
            SHELL_MESSAGE.store(RegisterWindowMessageW(w!("SHELLHOOK")), SeqCst);
            if !RegisterShellHookWindow(hwnd).as_bool() {
                eprintln!("[lanceur] barre : signaux de Windows indisponibles");
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                DispatchMessageW(&msg);
            }
            let _ = DeregisterShellHookWindow(hwnd);
            let _ = DestroyWindow(hwnd);
        }
    });
    if let Ok(id) = id_receiver.recv() {
        *watch = Some((id, thread));
    }
}

pub fn stop() {
    let Some((id, thread)) = WATCH.lock().unwrap().take() else { return };
    if unsafe { PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)) }.is_ok() {
        let _ = thread.join();
    }
}

/// Fenêtre invisible, hors barre des tâches : elle ne sert qu'à recevoir les signaux
fn create_window() -> Option<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class = w!("LanceurPastelSignaux");
        // Déjà inscrite si la barre a été coupée puis remise : sans effet
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(on_message), hInstance: instance.into(), lpszClassName: class, ..Default::default() });
        CreateWindowExW(WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0), class, w!(""), WS_POPUP, 0, 0, 0, 0, None, None, Some(instance.into()), None)
            .map_err(|e| eprintln!("[lanceur] barre : fenêtre des signaux impossible : {e}"))
            .ok()
    }
}

unsafe extern "system" fn on_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let shell = SHELL_MESSAGE.load(SeqCst);
    if shell == 0 || msg != shell {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    let window = lparam.0;
    let event = match wparam.0 {
        WINDOW_CREATED | REDRAW | WINDOW_REPLACED => Some(Event::Changed),
        WINDOW_DESTROYED => Some(Event::Gone(window)),
        WINDOW_ACTIVATED | RUDE_APP_ACTIVATED => Some(Event::Activated(window)),
        FLASH => Some(Event::Flash(window)),
        _ => None,
    };
    if let Some(event) = event {
        SENDER.with(|s| {
            if let Some(sender) = s.borrow().as_ref() {
                let _ = sender.send(event);
            }
        });
    }
    LRESULT(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        FlashWindowEx, PeekMessageW, ShowWindow, FLASHWINFO, FLASHW_ALL, PM_REMOVE, SW_SHOWMINNOACTIVE, WS_OVERLAPPEDWINDOW,
    };

    unsafe extern "system" fn plain(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    /// Diagnostic : une fenêtre d'essai, réduite (visible un instant dans la barre des tâches),
    /// clignote ; le signal doit arriver. cargo test real_shell_signals -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_shell_signals() {
        let (sender, signals) = std::sync::mpsc::channel();
        start(sender);
        let test = std::thread::spawn(|| unsafe {
            let instance = GetModuleHandleW(None).unwrap();
            let class = w!("LanceurPastelEssaiSignaux");
            RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(plain), hInstance: instance.into(), lpszClassName: class, ..Default::default() });
            let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("Essai du lanceur"), WS_OVERLAPPEDWINDOW, 0, 0, 200, 100, None, None, Some(instance.into()), None).unwrap();
            let _ = ShowWindow(hwnd, SW_SHOWMINNOACTIVE);
            std::thread::sleep(std::time::Duration::from_millis(400));
            let info = FLASHWINFO { cbSize: size_of::<FLASHWINFO>() as u32, hwnd, dwFlags: FLASHW_ALL, uCount: 2, dwTimeout: 0 };
            let _ = FlashWindowEx(&info);
            // La fenêtre d'essai traite ses messages pendant qu'elle clignote
            let until = std::time::Instant::now() + std::time::Duration::from_millis(1200);
            let mut msg = MSG::default();
            while std::time::Instant::now() < until {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    DispatchMessageW(&msg);
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let _ = DestroyWindow(hwnd);
            hwnd.0 as isize
        });
        let hwnd = test.join().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        stop();
        let events: Vec<_> = signals.try_iter().collect();
        let count = |f: &dyn Fn(&Event) -> bool| events.iter().filter(|e| f(e)).count();
        eprintln!("signaux : {}", events.len());
        eprintln!("  changés : {}", count(&|e| matches!(e, Event::Changed)));
        eprintln!("  clignotements de la fenêtre d'essai : {}", count(&|e| matches!(e, Event::Flash(h) if *h == hwnd)));
        eprintln!("  fermetures de la fenêtre d'essai : {}", count(&|e| matches!(e, Event::Gone(h) if *h == hwnd)));
        assert!(events.iter().any(|e| matches!(e, Event::Flash(h) if *h == hwnd)), "clignotement non reçu");
    }
}
