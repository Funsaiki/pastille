//! Hover previews of windows over their tile in the pastel bar (requested by Funsaiki): Windows'
//! live thumbnails (`DwmRegisterThumbnail`), as in its taskbar.
//! Windows draws them over the bar window, at the places the front gives it
//! in its preview card (bar.ts). One card at a time per bar window.

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::WebviewWindow;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Dwm::{
    DwmRegisterThumbnail, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties, DWM_THUMBNAIL_PROPERTIES,
    DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION, DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
};
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, IsIconic};

use crate::open_windows;

/// Per bar window: one thumbnail per requested window (None: minimized or closed)
static THUMBS: Mutex<Option<HashMap<String, Vec<Option<isize>>>>> = Mutex::new(None);

/// Thumbnails of the windows `ids`; for each, the size of its client area (pixels), or None
/// if it is minimized (Windows keeps no image of it) or closed. Replaces the previous card.
#[tauri::command]
pub fn bar_preview(window: WebviewWindow, ids: Vec<String>) -> Vec<Option<[i32; 2]>> {
    close(&window);
    let Ok(destination) = window.hwnd() else { return vec![None; ids.len()] };
    let (thumbs, sizes): (Vec<_>, Vec<_>) = ids
        .iter()
        .map(|id| {
            let Ok(source) = open_windows::app_window(id) else { return (None, None) };
            if unsafe { IsIconic(source) }.as_bool() {
                return (None, None);
            }
            let mut client = RECT::default();
            if unsafe { GetClientRect(source, &mut client) }.is_err() {
                return (None, None);
            }
            match unsafe { DwmRegisterThumbnail(destination, source) } {
                Ok(thumb) => (Some(thumb), Some([client.right, client.bottom])),
                Err(_) => (None, None),
            }
        })
        .unzip();
    THUMBS.lock().unwrap().get_or_insert_with(HashMap::new).insert(window.label().to_string(), thumbs);
    sizes
}

/// Places the thumbnails in the card, in the order of `bar_preview` (CSS pixels of the window:
/// x, y, width, height), and shows them
#[tauri::command]
pub fn bar_preview_place(window: WebviewWindow, rects: Vec<[f64; 4]>) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let at = |v: f64| (v * scale).round() as i32;
    let thumbs = THUMBS.lock().unwrap();
    let Some(thumbs) = thumbs.as_ref().and_then(|all| all.get(window.label())) else { return };
    for (thumb, r) in thumbs.iter().zip(rects) {
        let Some(thumb) = *thumb else { continue };
        let properties = DWM_THUMBNAIL_PROPERTIES {
            dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY,
            rcDestination: RECT { left: at(r[0]), top: at(r[1]), right: at(r[0] + r[2]), bottom: at(r[1] + r[3]) },
            opacity: 255,
            fVisible: true.into(),
            fSourceClientAreaOnly: true.into(),
            ..Default::default()
        };
        let _ = unsafe { DwmUpdateThumbnailProperties(thumb, &properties) };
    }
}

/// Card closed, or pill retracted
#[tauri::command]
pub fn bar_preview_close(window: WebviewWindow) {
    close(&window);
}

fn close(window: &WebviewWindow) {
    forget(window.label());
}

/// Releases the thumbnails of the bar window `label` (also when it is destroyed)
pub fn forget(label: &str) {
    let removed = THUMBS.lock().unwrap().as_mut().and_then(|all| all.remove(label));
    for thumb in removed.into_iter().flatten().flatten() {
        let _ = unsafe { DwmUnregisterThumbnail(thumb) };
    }
}
