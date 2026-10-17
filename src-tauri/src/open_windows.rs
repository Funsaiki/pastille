//! Fenêtres ouvertes proposées par la recherche (demande de Funsaiki) : taper « disc » propose
//! aussi la fenêtre de Discord, comme un Alt+Tab avec recherche. Les fenêtres de la barre des
//! tâches (switch.rs), sauf celles du lanceur.

use std::path::Path;

use serde::Serialize;
use windows::core::GUID;
use windows::Win32::Foundation::{HWND, PROPERTYKEY};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};

use crate::switch;

#[derive(Serialize)]
pub struct OpenWindow {
    /// Handle de la fenêtre, renvoyé pour la ramener (Rust vérifie qu'elle est encore là)
    id: String,
    title: String,
    /// Programme (« Discord », « Brave Browser »)
    app: String,
    icon: Option<crate::icon::Icon>,
}

/// La plus récemment active d'abord
#[tauri::command]
pub async fn open_windows() -> Vec<OpenWindow> {
    crate::com::com_task(|| Ok(list())).await.unwrap_or_default()
}

/// Ramène la fenêtre au premier plan (restaurée si elle était réduite)
#[tauri::command]
pub async fn focus_window(id: String) -> Result<(), String> {
    crate::blocking(move || {
        let hwnd = app_window(&id)?;
        if switch::bring(hwnd) { Ok(()) } else { Err("Windows a refusé de passer à cette fenêtre.".into()) }
    })
    .await?
}

/// Fenêtre envoyée par le front (recherche, barre pastel) : encore une fenêtre de la barre des
/// tâches, et pas le lanceur. Rien d'autre n'est touché.
pub(crate) fn app_window(id: &str) -> Result<HWND, String> {
    let handle: isize = id.parse().map_err(|_| format!("Fenêtre {id} inconnue."))?;
    let hwnd = HWND(handle as *mut _);
    if !switch::is_app_window(hwnd) || switch::process_of(hwnd).1 == std::process::id() {
        return Err("Cette fenêtre n'est plus ouverte.".to_string());
    }
    Ok(hwnd)
}

/// Identifiant d'application donné à la fenêtre (`PKEY_AppUserModel_ID`) : celui des apps du
/// Store hébergées par ApplicationFrameHost (Paramètres, Calculatrice). Avec COM.
pub(crate) fn window_app_id(hwnd: HWND) -> Option<String> {
    const APP_ID: PROPERTYKEY = PROPERTYKEY { fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3), pid: 5 };
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd).ok()?;
        let mut value = store.GetValue(&APP_ID).ok()?;
        let text = PropVariantToStringAlloc(&value).ok();
        let _ = PropVariantClear(&mut value);
        let text = text?;
        let id = text.to_string().ok();
        CoTaskMemFree(Some(text.0 as *const _));
        id.filter(|id| !id.is_empty())
    }
}

fn list() -> Vec<OpenWindow> {
    let own = std::process::id();
    switch::app_windows()
        .into_iter()
        .filter_map(|hwnd| {
            let (path, pid) = switch::process_of(hwnd);
            let path = path?;
            if pid == own {
                return None;
            }
            let app = crate::mixer::description(&path).unwrap_or_else(|| {
                Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
            });
            Some(OpenWindow { id: (hwnd.0 as isize).to_string(), title: title(hwnd), app, icon: crate::icon::of(&path) })
        })
        .collect()
}

pub(crate) fn title(hwnd: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    let mut buf = vec![0u16; len as usize + 1];
    let copied = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..copied.max(0) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diagnostic, lecture seule : cargo test real_open_windows -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_open_windows() {
        crate::com::with_com(|| {
            let start = std::time::Instant::now();
            for w in list() {
                eprintln!("{:<22} {}  (icône : {})", w.app, w.title, w.icon.is_some());
            }
            eprintln!("durée : {:?}", start.elapsed());
        });
    }
}
