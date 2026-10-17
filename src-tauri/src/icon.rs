//! Icône d'un programme pour le mélangeur (demande de Funsaiki) : la grande icône du shell
//! (celle de l'Explorateur), en pixels RGBA que le front dessine dans un canvas. Gardée en
//! mémoire par chemin : le mélangeur la redemande à chaque ouverture. Celles des applications
//! du menu Démarrer (start_menu.rs) viennent du menu lui-même, apps du Store comprises.

use std::{collections::HashMap, sync::Mutex};

use base64::Engine;
use serde::Serialize;
use windows::core::{Interface, HSTRING};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SIIGBF_ICONONLY,
};
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
    cached(path, || extract(path))
}

/// Icône d'une application du menu Démarrer (cible `shell:AppsFolder\…`), celle du menu, en
/// 48 px (les tuiles de la vue applications sont plus grandes que la liste du mélangeur). Avec COM.
pub fn of_start_app(target: &str) -> Option<Icon> {
    const SIZE_PX: i32 = 48;
    cached(target, || {
        let factory: IShellItemImageFactory = crate::start_menu::icon_item(target)?.cast().ok()?;
        let bitmap = unsafe { factory.GetImage(SIZE { cx: SIZE_PX, cy: SIZE_PX }, SIIGBF_ICONONLY) }.ok()?;
        let icon = image(bitmap);
        unsafe {
            let _ = DeleteObject(bitmap.into());
        }
        icon
    })
}

fn cached(key: &str, extract: impl FnOnce() -> Option<Icon>) -> Option<Icon> {
    if let Some(cached) = CACHE.lock().unwrap().get_or_insert_with(HashMap::new).get(key) {
        return cached.clone();
    }
    let icon = extract();
    CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(key.to_string(), icon.clone());
    icon
}

/// Image 32 bits du shell, en alpha prémultiplié (couleurs jamais au-dessus de l'alpha) :
/// remise en alpha simple, celui du canvas
fn image(bitmap: HBITMAP) -> Option<Icon> {
    let (width, height) = size(bitmap)?;
    let mut bgra = bits(bitmap, width, height)?;
    if bgra.chunks(4).all(|p| p[3] == 0) {
        bgra.chunks_mut(4).for_each(|p| p[3] = 255); // sans transparence
    } else if bgra.chunks(4).all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]) {
        for p in bgra.chunks_mut(4).filter(|p| p[3] > 0 && p[3] < 255) {
            let alpha = p[3] as u32;
            for c in &mut p[..3] {
                *c = (*c as u32 * 255 / alpha) as u8;
            }
        }
    }
    Some(rgba(width, height, bgra))
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
        let (width, height) = size(parts.hbmColor)?; // rien : icône monochrome, sans image couleur
        let mut bgra = bits(parts.hbmColor, width, height)?;
        // Icône ancienne, sans transparence dans l'image : le masque dit quels pixels sont vides
        if bgra.chunks(4).all(|p| p[3] == 0) {
            let mask = bits(parts.hbmMask, width, height)?;
            for (pixel, m) in bgra.chunks_mut(4).zip(mask.chunks(4)) {
                pixel[3] = if m[0] == 0 { 255 } else { 0 };
            }
        }
        Some(rgba(width, height, bgra))
    };
    let icon = read();
    unsafe {
        let _ = DeleteObject(parts.hbmColor.into());
        let _ = DeleteObject(parts.hbmMask.into());
    }
    icon
}

fn rgba(width: i32, height: i32, mut bgra: Vec<u8>) -> Icon {
    for pixel in bgra.chunks_mut(4) {
        pixel.swap(0, 2); // BGRA → RGBA
    }
    Icon {
        width: width as u32,
        height: height as u32,
        rgba: base64::engine::general_purpose::STANDARD.encode(bgra),
    }
}

fn size(bitmap: HBITMAP) -> Option<(i32, i32)> {
    let mut info = BITMAP::default();
    let filled = unsafe { GetObjectW(bitmap.into(), size_of::<BITMAP>() as i32, Some((&mut info as *mut BITMAP).cast())) };
    (filled != 0).then_some((info.bmWidth, info.bmHeight))
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
