//! Aperçus des fenêtres au survol de leur tuile dans la barre pastel (demande de Funsaiki) : les
//! vignettes en direct de Windows (`DwmRegisterThumbnail`), comme dans sa barre des tâches.
//! Windows les dessine par-dessus la fenêtre de la barre, aux places que le front lui donne
//! dans sa carte d'aperçu (bar.ts). Une carte à la fois par fenêtre de la barre.

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

/// Par fenêtre de la barre : une vignette par fenêtre demandée (None : réduite ou fermée)
static THUMBS: Mutex<Option<HashMap<String, Vec<Option<isize>>>>> = Mutex::new(None);

/// Vignettes des fenêtres `ids` ; pour chacune, la taille de sa zone cliente (pixels), ou None
/// si elle est réduite (Windows n'en garde pas d'image) ou fermée. Remplace la carte précédente.
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
            let (Ok(thumb), Ok(())) = (unsafe { DwmRegisterThumbnail(destination, source) }, unsafe {
                GetClientRect(source, &mut client)
            }) else {
                return (None, None);
            };
            (Some(thumb), Some([client.right, client.bottom]))
        })
        .unzip();
    THUMBS.lock().unwrap().get_or_insert_with(HashMap::new).insert(window.label().to_string(), thumbs);
    sizes
}

/// Places des vignettes dans la carte, dans l'ordre de `bar_preview` (pixels CSS de la fenêtre :
/// x, y, largeur, hauteur) ; elles apparaissent
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

/// Carte refermée, ou pilule rentrée
#[tauri::command]
pub fn bar_preview_close(window: WebviewWindow) {
    close(&window);
}

fn close(window: &WebviewWindow) {
    let removed = THUMBS.lock().unwrap().as_mut().and_then(|all| all.remove(window.label()));
    for thumb in removed.into_iter().flatten().flatten() {
        let _ = unsafe { DwmUnregisterThumbnail(thumb) };
    }
}
