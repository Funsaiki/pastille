//! Icône d'un programme pour le mélangeur (demande de Funsaiki) : la grande icône du shell
//! (celle de l'Explorateur), en pixels RGBA que le front dessine dans un canvas. Gardée en
//! mémoire par chemin : le mélangeur la redemande à chaque ouverture.

use std::{collections::HashMap, sync::Mutex};

use base64::Engine;
use serde::Serialize;
use windows::core::HSTRING;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

#[derive(Clone, Serialize)]
pub struct Icon {
    width: u32,
    height: u32,
    /// Pixels RGBA, ligne par ligne depuis le haut, en base64
    rgba: String,
}

static CACHE: Mutex<Option<HashMap<String, Option<Icon>>>> = Mutex::new(None);

/// À appeler avec COM initialisé (com_task)
pub fn of(path: &str) -> Option<Icon> {
    if let Some(cached) = CACHE.lock().unwrap().get_or_insert_with(HashMap::new).get(path) {
        return cached.clone();
    }
    let icon = extract(path);
    CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(path.to_string(), icon.clone());
    icon
}

fn extract(path: &str) -> Option<Icon> {
    let mut info = SHFILEINFOW::default();
    let flags = SHGFI_ICON | SHGFI_LARGEICON;
    let size = size_of::<SHFILEINFOW>() as u32;
    if unsafe { SHGetFileInfoW(&HSTRING::from(path), FILE_FLAGS_AND_ATTRIBUTES(0), Some(&mut info), size, flags) } == 0
        || info.hIcon.is_invalid()
    {
        return None;
    }
    let icon = pixels(info.hIcon);
    unsafe {
        let _ = DestroyIcon(info.hIcon);
    }
    icon
}

fn pixels(icon: HICON) -> Option<Icon> {
    let mut parts = ICONINFO::default();
    unsafe { GetIconInfo(icon, &mut parts) }.ok()?;
    let read = || {
        let mut bitmap = BITMAP::default();
        let filled = unsafe { GetObjectW(parts.hbmColor.into(), size_of::<BITMAP>() as i32, Some((&mut bitmap as *mut BITMAP).cast())) };
        if filled == 0 {
            return None; // icône monochrome, sans image couleur
        }
        let (width, height) = (bitmap.bmWidth, bitmap.bmHeight);
        let mut bgra = bits(parts.hbmColor, width, height)?;
        // Icône ancienne, sans transparence dans l'image : le masque dit quels pixels sont vides
        if bgra.chunks(4).all(|p| p[3] == 0) {
            let mask = bits(parts.hbmMask, width, height)?;
            for (pixel, m) in bgra.chunks_mut(4).zip(mask.chunks(4)) {
                pixel[3] = if m[0] == 0 { 255 } else { 0 };
            }
        }
        for pixel in bgra.chunks_mut(4) {
            pixel.swap(0, 2); // BGRA → RGBA
        }
        Some(Icon {
            width: width as u32,
            height: height as u32,
            rgba: base64::engine::general_purpose::STANDARD.encode(bgra),
        })
    };
    let icon = read();
    unsafe {
        let _ = DeleteObject(parts.hbmColor.into());
        let _ = DeleteObject(parts.hbmMask.into());
    }
    icon
}

/// Pixels d'une image en BGRA 32 bits, de haut en bas
fn bits(bitmap: HBITMAP, width: i32, height: i32) -> Option<Vec<u8>> {
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // négatif : première ligne en haut
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buffer = vec![0u8; (width * height * 4) as usize];
    unsafe {
        let dc = GetDC(None);
        let lines = GetDIBits(dc, bitmap, 0, height as u32, Some(buffer.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        (lines == height).then_some(buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_icon_is_read() {
        let windir = std::env::var("SystemRoot").unwrap();
        let icon = crate::com::with_com(|| of(&format!(r"{windir}\explorer.exe"))).expect("icône de l'Explorateur");
        let bytes = base64::engine::general_purpose::STANDARD.decode(&icon.rgba).unwrap();
        assert_eq!(bytes.len() as u32, icon.width * icon.height * 4);
        assert!(bytes.chunks(4).any(|p| p[3] == 255), "des pixels opaques");
        assert!(bytes.chunks(4).any(|p| p[3] == 0), "des pixels transparents");
    }
}
