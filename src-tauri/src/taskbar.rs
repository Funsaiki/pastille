//! Barre des tâches de Windows masquée tant que la barre pastel tourne (demande de Funsaiki : ne
//! plus la voir du tout, même souris en bas). Aucun réglage de Windows ne la supprime : ses
//! fenêtres (`Shell_TrayWnd`, et `Shell_SecondaryTrayWnd` par écran secondaire) sont masquées,
//! et recachées dès que Windows les réaffiche (crochet `EVENT_OBJECT_SHOW`, plus un relevé toutes
//! les 250 ms pour un Explorateur relancé). Essai du 16/10/2026 chez Funsaiki : jamais visible.
//! Rien dans le registre ; elles reviennent à l'arrêt (`restore`), et au démarrage si la barre
//! pastel est coupée. Arrêt brutal du lanceur (plantage, Gestionnaire des tâches) : un processus
//! de garde les réaffiche (`spawn_guard`, demande de Funsaiki).

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

/// Fil qui garde les barres masquées : (identifiant du fil, pour l'arrêter ; le fil)
static WATCH: Mutex<Option<(u32, JoinHandle<()>)>> = Mutex::new(None);

/// Masque les barres de Windows jusqu'à `restore`
pub fn hide() {
    let mut watch = WATCH.lock().unwrap();
    if watch.is_some() {
        return;
    }
    spawn_guard();
    let (sender, receiver) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        // File de messages créée avant de donner l'identifiant : `restore` peut y poster WM_QUIT
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
    if unsafe { PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)) }.is_ok() {
        let _ = thread.join();
    } else {
        show_all();
    }
}

/// Argument du processus de garde, suivi de l'identifiant du lanceur
const GUARD: &str = "--garde-barre";

/// Filet de sécurité : un second processus du lanceur (le même exe), qui ne fait qu'attendre la
/// fin du premier pour réafficher la barre de Windows. Un seul par lanceur, lancé la première
/// fois que la barre de Windows est masquée.
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
        eprintln!("[lanceur] barre : processus de garde impossible : {e}");
        SPAWNED.store(false, SeqCst);
    }
}

/// Appelé par `run` avant tout : vrai pour le processus de garde, qui attend la fin du lanceur,
/// réaffiche la barre de Windows (sans effet si le lanceur l'a déjà rendue) et s'arrête
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

/// Sans effet sur une barre déjà affichée. Sans attendre l'Explorateur : s'il ne répond plus,
/// le lanceur ne se fige pas (appelé depuis le fil principal).
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
