//! Notification area taken over from Windows while the pastel bar runs (requested by Funsaiki,
//! 2026-10-18: with the pastel bar, the EarTrumpet icon restarted the program instead of opening its
//! mixer, and no icon gave its menu). An app declares its icon to the window
//! "Shell_TrayWnd" (`Shell_NotifyIcon`: a WM_COPYDATA), the first one Windows finds:
//! normally the taskbar. Like RetroBar, Pastille creates its own, kept in front of the Windows
//! one; it records each icon (window, callback message, tooltip) and forwards everything
//! to the Windows taskbar, which stays up to date (balloons shown as notifications, ready when it
//! comes back). A click on a tile sends the app the messages of a click on its icon: it does what
//! it usually does (its mixer, its menu). Icons already there are requested again, only from the
//! programs that have some ("TaskbarCreated", the signal of a restarted Explorer).
//!
//! Everything happens on the thread of taskbar.rs, which hides the Windows taskbar.

use std::collections::HashSet;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering::SeqCst};
use std::sync::Mutex;

use windows::core::{w, BOOL, GUID, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, ChangeWindowMessageFilterEx, CreateWindowExW, DefWindowProcW, DestroyWindow,
    EnumWindows, FindWindowExW, FindWindowW, GetCursorPos, GetWindowRect, GetWindowThreadProcessId, IsWindow,
    KillTimer, RegisterClassW, RegisterWindowMessageW, SendMessageTimeoutW, SendNotifyMessageW, SetTimer,
    SetWindowPos, HWND_TOPMOST, MSGFLT_ALLOW, SET_WINDOW_POS_FLAGS, SMTO_ABORTIFHUNG, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_SHOWWINDOW, WINDOWPOS, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_COPYDATA, WM_LBUTTONDBLCLK,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_TIMER, WM_USER,
    WM_WINDOWPOSCHANGING, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

/// Pastille's "Shell_TrayWnd" window (0: pastel bar off)
static HOST: AtomicIsize = AtomicIsize::new(0);
/// "TaskbarCreated" message, specific to each Windows session
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
static ICONS: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

const TRAY_CLASS: PCWSTR = w!("Shell_TrayWnd");
/// WM_COPYDATA of `Shell_NotifyIcon` (0: appbars)
const NOTIFY_ICON: usize = 1;
/// WM_COPYDATA of `Shell_NotifyIconGetRect`: where the icon is (to place its menu there)
const ICON_RECT: usize = 3;
/// Window timer: request the icons again after Explorer restarts
const ANNOUNCE: usize = 1;

// Shell_NotifyIcon (shellapi.h)
const NIM_ADD: u32 = 0;
const NIM_MODIFY: u32 = 1;
const NIM_DELETE: u32 = 2;
const NIM_SETVERSION: u32 = 4;
const NIF_MESSAGE: u32 = 0x1;
const NIF_TIP: u32 = 0x4;
const NIF_STATE: u32 = 0x8;
const NIF_GUID: u32 = 0x20;
const NIS_HIDDEN: u32 = 0x1;
const NIN_SELECT: u32 = 0x400;
const NIN_KEYSELECT: u32 = 0x401;

/// An icon, as the app declared it
struct Entry {
    id: u32,
    hwnd: isize,
    uid: u32,
    guid: Option<GUID>,
    /// Message sent to `hwnd` for its clicks (0: none, the icon doesn't react)
    callback: u32,
    /// 0, 3 or 4 (NOTIFYICON_VERSION_4): the form of the click messages
    version: u32,
    tip: String,
    hidden: bool,
    /// The mouse at the last click on its tile: where the icon is, as far as the app knows
    clicked: Option<POINT>,
}

impl Entry {
    fn update(&mut self, n: &Notify) {
        if n.hwnd != 0 {
            self.hwnd = n.hwnd;
            self.uid = n.uid;
        }
        if n.flags & NIF_GUID != 0 {
            self.guid = Some(n.guid);
        }
        if n.flags & NIF_MESSAGE != 0 {
            self.callback = n.callback;
        }
        if n.flags & NIF_TIP != 0 {
            self.tip = n.tip.clone();
        }
        if n.flags & NIF_STATE != 0 && n.state_mask & NIS_HIDDEN != 0 {
            self.hidden = n.state & NIS_HIDDEN != 0;
        }
    }
}

/// What `Shell_NotifyIcon` sends: SHELLTRAYDATA (signature, message), then NOTIFYICONDATA
/// in its 32-bit form (4-byte handles, whatever the app)
struct Notify {
    message: u32,
    hwnd: isize,
    uid: u32,
    flags: u32,
    callback: u32,
    tip: String,
    state: u32,
    state_mask: u32,
    version: u32,
    guid: GUID,
}

fn parse(data: &[u8]) -> Option<Notify> {
    const NID: usize = 8;
    let word = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    // Fields missing from an older, shorter format: zero
    let field = |at: usize| word(NID + at).unwrap_or(0);
    let text = |at: usize, len: usize| {
        let bytes = data.get(NID + at..).unwrap_or_default();
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .take(len)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        String::from_utf16_lossy(&units)
    };
    Some(Notify {
        message: word(4)?,
        // 32-bit handle, sign-extended as Windows does
        hwnd: word(NID + 4)? as i32 as isize,
        uid: word(NID + 8)?,
        flags: word(NID + 12)?,
        callback: field(16),
        tip: text(24, 128),
        state: field(280),
        state_mask: field(284),
        version: field(800),
        guid: guid_at(data, NID + 936),
    })
}

/// Zero GUID if missing (older format)
fn guid_at(data: &[u8], at: usize) -> GUID {
    data.get(at..at + 16).map_or(GUID::zeroed(), |b| {
        GUID::from_values(
            u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            u16::from_le_bytes([b[4], b[5]]),
            u16::from_le_bytes([b[6], b[7]]),
            [b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]],
        )
    })
}

/// Answer to `Shell_NotifyIconGetRect`: WINNOTIFYICONIDENTIFIER (signature, requested corner:
/// 1 top left, 2 bottom right, size, padding, 32-bit window, identifier,
/// GUID). One pixel under the mouse at the last click on its tile: its menu opens there, not in the
/// corner of the hidden Windows taskbar. None: never clicked, the Windows taskbar answers.
fn icon_corner(data: &[u8]) -> Option<isize> {
    let word = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let (corner, hwnd, uid, guid) = (word(4)?, word(16)? as i32 as isize, word(20)?, guid_at(data, 24));
    let icons = ICONS.lock().unwrap();
    let entry = icons.iter().find(|e| match guid == GUID::zeroed() {
        true => e.guid.is_none() && e.hwnd == hwnd && e.uid == uid,
        false => e.guid == Some(guid),
    })?;
    let at = entry.clicked?;
    let (x, y) = match corner {
        1 => (at.x, at.y),
        2 => (at.x + 1, at.y + 1),
        _ => return None,
    };
    // Like MAKELPARAM: x low, y high (negative on a screen to the left or above)
    Some(((y as u16 as u32) << 16 | x as u16 as u32) as i32 as isize)
}

/// Updates the icons; true if Pastille accepts the message
fn notify(n: &Notify) -> bool {
    let by_guid = n.flags & NIF_GUID != 0;
    let mut icons = ICONS.lock().unwrap();
    let found = icons.iter().position(|e| match by_guid {
        true => e.guid == Some(n.guid),
        false => e.guid.is_none() && e.hwnd == n.hwnd && e.uid == n.uid,
    });
    match n.message {
        NIM_ADD | NIM_MODIFY => {
            let index = match found {
                Some(index) => index,
                // A modified icon we didn't know about (added before Pastille) is taken too
                None if n.hwnd != 0 => {
                    if cfg!(debug_assertions) {
                        eprintln!("[lanceur] zone de notification : icône de {} ({})", pid_of_window(n.hwnd), n.tip.lines().next().unwrap_or(""));
                    }
                    let id = NEXT_ID.fetch_add(1, SeqCst);
                    let entry = Entry {
                        id,
                        hwnd: 0,
                        uid: 0,
                        guid: None,
                        callback: 0,
                        version: 0,
                        tip: String::new(),
                        hidden: false,
                        clicked: None,
                    };
                    icons.push(entry);
                    icons.len() - 1
                }
                None => return false,
            };
            icons[index].update(n);
            true
        }
        NIM_DELETE => found.map(|index| icons.remove(index)).is_some(),
        NIM_SETVERSION => match found {
            Some(index) if n.version <= 4 => {
                icons[index].version = n.version;
                true
            }
            _ => false,
        },
        // NIM_SETFOCUS: the Windows taskbar answers
        _ => false,
    }
}

/// Creates the window and requests the icons again. On the thread of taskbar.rs, which processes its messages.
pub fn start() {
    unsafe {
        let Ok(instance) = GetModuleHandleW(None) else { return };
        // Already registered if the bar was turned off then on again: no effect
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(on_message), hInstance: instance.into(), lpszClassName: TRAY_CLASS, ..Default::default() });
        // Same rectangle as the Windows taskbar, for programs that ask it for its rectangle
        let mut rect = RECT::default();
        if let Some(bar) = windows_tray() {
            let _ = GetWindowRect(bar, &mut rect);
        }
        let created = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            TRAY_CLASS,
            w!(""),
            WS_POPUP,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            Some(instance.into()),
            None,
        );
        let hwnd = match created {
            Ok(hwnd) => hwnd,
            Err(e) => return eprintln!("[lanceur] zone de notification impossible : {e}"),
        };
        // Programs at a lower integrity level can declare their icon too
        let _ = ChangeWindowMessageFilterEx(hwnd, WM_COPYDATA, MSGFLT_ALLOW, None);
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), SeqCst);
        HOST.store(hwnd.0 as isize, SeqCst);
    }
    keep_first();
    announce();
}

/// On the same thread as `start`. The Windows taskbar already has everything: nothing to give back.
pub fn stop() {
    let host = HOST.swap(0, SeqCst);
    if host != 0 {
        let _ = unsafe { DestroyWindow(HWND(host as *mut _)) };
    }
    ICONS.lock().unwrap().clear();
}

/// Pastille's window stays the first one found (Explorer sometimes brings its own back up).
/// Checked every 250 ms by taskbar.rs: ~7 µs.
pub fn keep_first() {
    let host = HOST.load(SeqCst);
    if host == 0 {
        return;
    }
    let first = unsafe { FindWindowW(TRAY_CLASS, PCWSTR::null()) }.map_or(0, |h| h.0 as isize);
    if first != host {
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
        let _ = unsafe { SetWindowPos(HWND(host as *mut _), Some(HWND_TOPMOST), 0, 0, 0, 0, flags) };
    }
}

/// Pastille's window (its "Shell_TrayWnd"): taskbar.rs neither hides nor shows it
pub fn own(hwnd: HWND) -> bool {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid == std::process::id()
}

/// The Windows taskbar: a "Shell_TrayWnd" that isn't Pastille's
fn windows_tray() -> Option<HWND> {
    let mut after = None;
    while let Ok(bar) = unsafe { FindWindowExW(None, after, TRAY_CLASS, None) } {
        if !own(bar) {
            return Some(bar);
        }
        after = Some(bar);
    }
    None
}

/// Forwarded to the Windows taskbar; its answer, unless it doesn't answer (4 s) or there is
/// none (Explorer restarting)
fn forward(msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<isize> {
    let bar = windows_tray()?;
    let mut result = 0usize;
    let sent = unsafe { SendMessageTimeoutW(bar, msg, wparam, lparam, SMTO_ABORTIFHUNG, 4000, Some(&mut result)) };
    (sent.0 != 0).then_some(result as isize)
}

/// Asks the programs that have icons (notification area registry) for them again, as
/// after an Explorer restart; not Windows nor Pastille
fn announce() {
    let message = TASKBAR_CREATED.load(SeqCst);
    if message == 0 {
        return;
    }
    let mut targets = (message, crate::tray_apps::tray_pids());
    // EnumWindows reports an "error" when the callback stops the enumeration: never here
    let _ = unsafe { EnumWindows(Some(tell), LPARAM(&mut targets as *mut (u32, HashSet<u32>) as isize)) };
}

unsafe extern "system" fn tell(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let (message, pids) = unsafe { &*(lparam.0 as *const (u32, HashSet<u32>)) };
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pids.contains(&pid) {
        // Without waiting for the answer: each one gives its icon back when it wants
        let _ = unsafe { SendNotifyMessageW(hwnd, *message, WPARAM(0), LPARAM(0)) };
    }
    true.into()
}

unsafe extern "system" fn on_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COPYDATA => {
            let copy = unsafe { &*(lparam.0 as *const COPYDATASTRUCT) };
            let data = match copy.lpData.is_null() {
                true => &[][..],
                false => unsafe { std::slice::from_raw_parts(copy.lpData as *const u8, copy.cbData as usize) },
            };
            if copy.dwData == ICON_RECT {
                if let Some(corner) = icon_corner(data) {
                    return LRESULT(corner);
                }
            }
            let accepted = copy.dwData == NOTIFY_ICON && parse(data).is_some_and(|n| notify(&n));
            let answer = forward(msg, wparam, lparam);
            // An icon that Pastille took is accepted, even if Windows already had it (requested again
            // by `announce`, it refuses to add it a second time)
            LRESULT(if accepted { 1 } else { answer.unwrap_or(0) })
        }
        // Never shown, even by a program that takes it for the Windows taskbar
        WM_WINDOWPOSCHANGING => {
            let pos = unsafe { &mut *(lparam.0 as *mut WINDOWPOS) };
            pos.flags = SET_WINDOW_POS_FLAGS(pos.flags.0 & !SWP_SHOWWINDOW.0);
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_CLOSE => LRESULT(0),
        WM_TIMER if wparam.0 == ANNOUNCE => {
            let _ = unsafe { KillTimer(Some(hwnd), ANNOUNCE) };
            announce();
            LRESULT(0)
        }
        // Commands and private messages of the taskbar (WM_USER to WM_APP): to the real one. Not the
        // registered messages (0xC000 and up), often broadcast to every window: it has them.
        WM_COMMAND | WM_USER..0xC000 => {
            forward(msg, wparam, lparam).map_or_else(|| unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }, LRESULT)
        }
        _ if msg != 0 && msg == TASKBAR_CREATED.load(SeqCst) => {
            // Explorer restarted: its new taskbar may have moved in front, and the
            // programs gave their icons to it again. Requested again a bit later.
            keep_first();
            unsafe { SetTimer(Some(hwnd), ANNOUNCE, 2000, None) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Icon shown in the app list (tray_apps.rs)
pub struct Live {
    pub key: String,
    pub pid: u32,
    pub tip: String,
    /// It has a callback message: its clicks go to the app
    pub clicks: bool,
}

/// Visible icons (not hidden by their app), in order of arrival; None: Pastille's
/// notification area isn't running (pastel bar off)
pub fn icons() -> Option<Vec<Live>> {
    if HOST.load(SeqCst) == 0 {
        return None;
    }
    let mut icons = ICONS.lock().unwrap();
    // App stopped without removing its icon (crash, forced stop)
    icons.retain(|e| unsafe { IsWindow(Some(HWND(e.hwnd as *mut _))) }.as_bool());
    let live = icons
        .iter()
        .filter(|e| !e.hidden)
        .map(|e| Live { key: key_of(e.id), pid: pid_of_window(e.hwnd), tip: e.tip.clone(), clicks: e.callback != 0 })
        .collect();
    Some(live)
}

fn key_of(id: u32) -> String {
    format!("#{id}")
}

fn pid_of_window(hwnd: isize) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid)) };
    pid
}

/// Process of an icon in the list
pub fn pid_of(key: &str) -> Option<u32> {
    let icons = ICONS.lock().unwrap();
    icons.iter().find(|e| key_of(e.id) == key).map(|e| pid_of_window(e.hwnd)).filter(|&pid| pid != 0)
}

/// The messages of a click on the icon, sent to its app. `kind`: left, double (the
/// second click of a double click), right, middle, key (Enter). False: it has no callback
/// message (nothing would happen with Windows either).
pub fn click(key: &str, kind: &str) -> Result<bool, String> {
    let mut at = POINT::default();
    let _ = unsafe { GetCursorPos(&mut at) };
    let found = ICONS.lock().unwrap().iter_mut().find(|e| key_of(e.id) == key).map(|e| {
        e.clicked = Some(at);
        (e.hwnd, e.uid, e.callback, e.version)
    });
    let (hwnd, uid, callback, version) = found.ok_or(crate::i18n::tr("Cette icône n'est plus là.", "This icon is gone."))?;
    if callback == 0 {
        return Ok(false);
    }
    let hwnd = HWND(hwnd as *mut _);
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return Err(crate::i18n::tr("Cette application n'est plus ouverte.", "This app is no longer open.").into());
    }
    // It may come to the foreground (its menu, its window), as after a click in the Windows
    // taskbar
    let _ = unsafe { AllowSetForegroundWindow(pid_of_window(hwnd.0 as isize)) };
    for (wparam, lparam) in messages(kind, version, uid, at)? {
        // Without waiting: a busy app doesn't freeze Pastille
        unsafe { SendNotifyMessageW(hwnd, callback, wparam, lparam) }.map_err(|e| e.message())?;
    }
    Ok(true)
}

/// Like the Windows taskbar: version 4, the icon's identifier with the event, and the anchor
/// point (the mouse); before, the identifier alone. NIN_SELECT and WM_CONTEXTMENU since version 3.
fn messages(kind: &str, version: u32, uid: u32, at: POINT) -> Result<Vec<(WPARAM, LPARAM)>, String> {
    let recent = version >= 3;
    let events: &[u32] = match kind {
        "left" if recent => &[WM_LBUTTONDOWN, WM_LBUTTONUP, NIN_SELECT],
        "left" | "key" if !recent => &[WM_LBUTTONDOWN, WM_LBUTTONUP],
        "double" if recent => &[WM_LBUTTONDBLCLK, WM_LBUTTONUP, NIN_SELECT],
        "double" => &[WM_LBUTTONDBLCLK, WM_LBUTTONUP],
        "right" if recent => &[WM_RBUTTONDOWN, WM_RBUTTONUP, WM_CONTEXTMENU],
        "right" => &[WM_RBUTTONDOWN, WM_RBUTTONUP],
        "middle" => &[WM_MBUTTONDOWN, WM_MBUTTONUP],
        "key" => &[NIN_KEYSELECT],
        _ => return Err(crate::i18n::tr(format!("Clic inconnu : {kind}."), format!("Unknown click: {kind}."))),
    };
    let anchor = (at.x as u16 as usize) | (at.y as u16 as usize) << 16;
    Ok(events
        .iter()
        .map(|&event| match version {
            4.. => (WPARAM(anchor), LPARAM((event | (uid & 0xffff) << 16) as isize)),
            _ => (WPARAM(uid as usize), LPARAM(event as isize)),
        })
        .collect())
}

// The tests never create the real window (`start`): every icon on the PC would go through it.
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, PostQuitMessage, GetMessageW, MSG, PM_NOREMOVE, WINDOW_EX_STYLE,
    };

    /// SHELLTRAYDATA as `Shell_NotifyIcon` sends it
    fn tray_data(message: u32, hwnd: u32, uid: u32, flags: u32, callback: u32, tip: &str) -> Vec<u8> {
        let mut data = vec![0u8; 8 + 956];
        let put = |data: &mut Vec<u8>, at: usize, value: u32| data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        put(&mut data, 0, 0x3475_3423);
        put(&mut data, 4, message);
        put(&mut data, 8, 956);
        put(&mut data, 12, hwnd);
        put(&mut data, 16, uid);
        put(&mut data, 20, flags);
        put(&mut data, 24, callback);
        for (i, unit) in tip.encode_utf16().enumerate() {
            data[32 + i * 2..34 + i * 2].copy_from_slice(&unit.to_le_bytes());
        }
        data
    }

    fn guid_bytes(guid: GUID) -> Vec<u8> {
        [&guid.data1.to_le_bytes()[..], &guid.data2.to_le_bytes(), &guid.data3.to_le_bytes(), &guid.data4].concat()
    }

    /// WINNOTIFYICONIDENTIFIER as `Shell_NotifyIconGetRect` sends it
    fn identifier(corner: u32, hwnd: u32, uid: u32, guid: GUID) -> Vec<u8> {
        let mut data = vec![0u8; 40];
        data[0..4].copy_from_slice(&0x3475_3423u32.to_le_bytes());
        data[4..8].copy_from_slice(&corner.to_le_bytes());
        data[8..12].copy_from_slice(&40u32.to_le_bytes());
        data[16..20].copy_from_slice(&hwnd.to_le_bytes());
        data[20..24].copy_from_slice(&uid.to_le_bytes());
        data[24..40].copy_from_slice(&guid_bytes(guid));
        data
    }

    fn entry(hwnd: isize) -> Option<(u32, u32, String, bool, u32)> {
        let icons = ICONS.lock().unwrap();
        icons.iter().find(|e| e.hwnd == hwnd).map(|e| (e.uid, e.callback, e.tip.clone(), e.hidden, e.version))
    }

    #[test]
    fn shell_tray_data_is_read() {
        let mut data = tray_data(NIM_MODIFY, 0x0012_3456, 7, NIF_MESSAGE | NIF_TIP | NIF_GUID, 0x8001, "EarTrumpet : 42 %");
        let guid = GUID::from_u128(0x7820ae73_23e3_4229_82c1_e41cb67d5b9c);
        data[8 + 936..8 + 952].copy_from_slice(&guid_bytes(guid));
        let n = parse(&data).unwrap();
        assert_eq!((n.message, n.hwnd, n.uid, n.callback), (NIM_MODIFY, 0x0012_3456, 7, 0x8001));
        assert_eq!(n.tip, "EarTrumpet : 42 %");
        assert_eq!(n.guid, guid);
        // "Negative" 32-bit handle: sign-extended like Windows does
        assert_eq!(parse(&tray_data(NIM_ADD, 0xFFFF_FFF0, 1, 0, 0, "")).unwrap().hwnd, -16);
        // Too short to be a Shell_NotifyIcon message
        assert!(parse(&data[..12]).is_none());
    }

    #[test]
    fn icons_follow_their_messages() {
        let hwnd = 0x5A5A_0001;
        let send = |message, flags, callback, tip: &str| notify(&parse(&tray_data(message, hwnd as u32, 3, flags, callback, tip)).unwrap());
        assert!(send(NIM_ADD, NIF_MESSAGE | NIF_TIP, 0x8001, "Essai"));
        assert_eq!(entry(hwnd), Some((3, 0x8001, "Essai".into(), false, 0)));
        // Modified: only the announced fields change
        assert!(send(NIM_MODIFY, NIF_TIP, 0, "Essai : 2 messages"));
        assert_eq!(entry(hwnd), Some((3, 0x8001, "Essai : 2 messages".into(), false, 0)));
        let mut version = parse(&tray_data(NIM_SETVERSION, hwnd as u32, 3, 0, 0, "")).unwrap();
        version.version = 4;
        assert!(notify(&version));
        // Hidden by its app
        let mut hidden = parse(&tray_data(NIM_MODIFY, hwnd as u32, 3, NIF_STATE, 0, "")).unwrap();
        (hidden.state, hidden.state_mask) = (NIS_HIDDEN, NIS_HIDDEN);
        assert!(notify(&hidden));
        assert_eq!(entry(hwnd), Some((3, 0x8001, "Essai : 2 messages".into(), true, 4)));
        assert!(send(NIM_DELETE, 0, 0, ""));
        assert_eq!(entry(hwnd), None);
        // Unknown: nothing to remove, the Windows taskbar answers
        assert!(!send(NIM_DELETE, 0, 0, ""));
    }

    #[test]
    fn icons_with_a_guid_follow_it() {
        let guid = GUID::from_u128(0x0a0b0c0d_0000_4000_8000_5a5a5a5a0002);
        let message = |kind, hwnd: u32| {
            let mut n = parse(&tray_data(kind, hwnd, 0, NIF_GUID | NIF_TIP, 0, "Guid")).unwrap();
            n.guid = guid;
            n
        };
        assert!(notify(&message(NIM_ADD, 0x5A5A_0003)));
        // Same GUID, new window (app restarted): the same icon
        assert!(notify(&message(NIM_MODIFY, 0x5A5A_0004)));
        assert_eq!(ICONS.lock().unwrap().iter().filter(|e| e.guid == Some(guid)).count(), 1);
        assert!(entry(0x5A5A_0004).is_some());
        assert!(notify(&message(NIM_DELETE, 0)));
        assert!(entry(0x5A5A_0004).is_none());
    }

    #[test]
    fn menus_open_where_the_icon_was_clicked() {
        let hwnd = 0x5A5A_0005;
        assert!(notify(&parse(&tray_data(NIM_ADD, hwnd as u32, 2, NIF_MESSAGE, 0x8001, "")).unwrap()));
        let corner = |n| icon_corner(&identifier(n, hwnd as u32, 2, GUID::zeroed()));
        let point = |l: isize| (l as u16 as i16, (l >> 16) as u16 as i16);
        // Never clicked: the Windows taskbar answers
        assert_eq!(corner(1), None);
        ICONS.lock().unwrap().iter_mut().find(|e| e.hwnd == hwnd).unwrap().clicked = Some(POINT { x: -20, y: 1070 });
        assert_eq!(corner(1).map(point), Some((-20, 1070)));
        assert_eq!(corner(2).map(point), Some((-19, 1071)));
        // Another icon of the same window: unknown
        assert_eq!(icon_corner(&identifier(1, hwnd as u32, 3, GUID::zeroed())), None);
        assert!(notify(&parse(&tray_data(NIM_DELETE, hwnd as u32, 2, 0, 0, "")).unwrap()));

        // Icon with a GUID: found by it, without a window
        let guid = GUID::from_u128(0x0a0b0c0d_0000_4000_8000_5a5a5a5a0006);
        let mut added = parse(&tray_data(NIM_ADD, 0x5A5A_0006, 0, NIF_GUID | NIF_MESSAGE, 0x8001, "")).unwrap();
        added.guid = guid;
        assert!(notify(&added));
        ICONS.lock().unwrap().iter_mut().find(|e| e.guid == Some(guid)).unwrap().clicked = Some(POINT { x: 900, y: 1040 });
        assert_eq!(icon_corner(&identifier(1, 0, 0, guid)).map(point), Some((900, 1040)));
        let mut deleted = parse(&tray_data(NIM_DELETE, 0, 0, NIF_GUID, 0, "")).unwrap();
        deleted.guid = guid;
        assert!(notify(&deleted));
    }

    #[test]
    fn clicks_follow_the_version() {
        let at = POINT { x: -20, y: 1070 };
        let pairs = |kind, version| messages(kind, version, 5, at).unwrap();
        let events = |list: Vec<(WPARAM, LPARAM)>| list.iter().map(|(_, l)| l.0 as u32 & 0xffff).collect::<Vec<_>>();
        assert_eq!(events(pairs("left", 0)), [WM_LBUTTONDOWN, WM_LBUTTONUP]);
        assert_eq!(events(pairs("left", 4)), [WM_LBUTTONDOWN, WM_LBUTTONUP, NIN_SELECT]);
        assert_eq!(events(pairs("right", 3)), [WM_RBUTTONDOWN, WM_RBUTTONUP, WM_CONTEXTMENU]);
        assert_eq!(events(pairs("key", 4)), [NIN_KEYSELECT]);
        assert_eq!(events(pairs("key", 0)), [WM_LBUTTONDOWN, WM_LBUTTONUP]);
        assert_eq!(events(pairs("double", 0)), [WM_LBUTTONDBLCLK, WM_LBUTTONUP]);
        // Before version 4: the icon's identifier in wParam
        assert_eq!(pairs("left", 3)[0].0 .0, 5);
        // Version 4: the identifier with the event, the mouse in wParam (negative x: second screen on the left)
        let (wparam, lparam) = pairs("left", 4)[0];
        assert_eq!(lparam.0 as u32 >> 16, 5);
        assert_eq!((wparam.0 as u16 as i16, (wparam.0 >> 16) as u16 as i16), (-20, 1070));
        assert!(messages("triple", 4, 5, at).is_err());
    }

    /// The test's own window: it records the callback messages it receives
    static RECEIVED: Mutex<Vec<(usize, isize)>> = Mutex::new(Vec::new());
    const TEST_CALLBACK: u32 = WM_USER + 1024;

    unsafe extern "system" fn record(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if msg == TEST_CALLBACK {
            RECEIVED.lock().unwrap().push((wparam.0, lparam.0));
            if lparam.0 as u32 == WM_LBUTTONUP {
                unsafe { PostQuitMessage(0) };
            }
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    /// The click reaches the icon's window (a window of the test, never an app)
    #[test]
    fn click_reaches_the_icon_window() {
        let (sender, receiver) = mpsc::channel();
        let window = std::thread::spawn(move || unsafe {
            let _ = PeekMessageW(&mut MSG::default(), None, 0, 0, PM_NOREMOVE);
            let instance = GetModuleHandleW(None).unwrap();
            let class = w!("PastilleEssaiIcone");
            RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(record), hInstance: instance.into(), lpszClassName: class, ..Default::default() });
            let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!(""), WS_POPUP, 0, 0, 0, 0, None, None, Some(instance.into()), None).unwrap();
            sender.send(hwnd.0 as isize).unwrap();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                DispatchMessageW(&msg);
            }
            let _ = DestroyWindow(hwnd);
        });
        let hwnd = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(notify(&parse(&tray_data(NIM_ADD, hwnd as u32, 9, NIF_MESSAGE, TEST_CALLBACK, "")).unwrap()));
        let key = key_of(ICONS.lock().unwrap().iter().find(|e| e.hwnd == hwnd).unwrap().id);
        assert_eq!(pid_of(&key), Some(std::process::id()));
        assert_eq!(click(&key, "left"), Ok(true));
        window.join().unwrap();
        // Version 0 (no NIM_SETVERSION): the icon's identifier, then the mouse message
        assert_eq!(*RECEIVED.lock().unwrap(), [(9, WM_LBUTTONDOWN as isize), (9, WM_LBUTTONUP as isize)]);
        // Window closed: no more clicks possible
        assert!(click(&key, "left").is_err());
        assert!(click("#0", "left").is_err());
    }
}
