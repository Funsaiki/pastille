//! Post-it of the pastel bar: the drawing copied to the clipboard, or saved as PNG in
//! Pictures\Pastille. The bar's window never has the focus the browser's clipboard asks for:
//! the page sends the image here, as raw bytes (no JSON).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use tauri::ipc::{InvokeBody, Request};
use tauri::WebviewWindow;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Shell::FOLDERID_Pictures;

use crate::i18n::tr;

const CF_DIB: u32 = 8;       // Win32_System_Ole's constant, not worth the feature
const MAX_SIDE: usize = 4096;
const PNG_MAX: usize = 16 << 20;
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
const OPEN_TRIES: usize = 10;  // another program may hold the clipboard for a moment
const OPEN_WAIT: Duration = Duration::from_millis(20);

fn raw(request: &Request<'_>) -> Result<Vec<u8>, String> {
    match request.body() {
        InvokeBody::Raw(bytes) => Ok(bytes.clone()),
        _ => Err("image expected as raw bytes".into()),
    }
}

/// Copy: the page's pixels (RGBA from the top, `width` header) as a bitmap (CF_DIB: every
/// app pastes it)
#[tauri::command]
pub async fn note_copy(window: WebviewWindow, request: Request<'_>) -> Result<(), String> {
    let rgba = raw(&request)?;
    let width: usize = request.headers().get("width").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0);
    let height = if width > 0 { rgba.len() / 4 / width } else { 0 };
    if width == 0 || width > MAX_SIDE || height == 0 || height > MAX_SIDE || rgba.len() != width * height * 4 {
        return Err("image size doesn't match its pixels".into());
    }
    let owner = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    crate::blocking(move || to_clipboard(HWND(owner as *mut _), &dib(width, height, &rgba))).await?
}

/// Device-independent bitmap: 40-byte header, then the rows from the bottom, in BGRA
fn dib(width: usize, height: usize, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(40 + rgba.len());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes());   // positive: from the bottom
    out.extend_from_slice(&1u16.to_le_bytes());              // planes
    out.extend_from_slice(&32u16.to_le_bytes());             // bits per pixel
    out.extend_from_slice(&0u32.to_le_bytes());              // BI_RGB
    out.extend_from_slice(&(rgba.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0; 16]);                         // resolution, colors
    for row in rgba.chunks_exact(width * 4).rev() {
        for p in row.chunks_exact(4) {
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    out
}

fn to_clipboard(owner: HWND, data: &[u8]) -> Result<(), String> {
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, data.len()).map_err(|e| e.message())?;
        let at = GlobalLock(memory);
        if at.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err("GlobalLock failed".into());
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), at as *mut u8, data.len());
        let _ = GlobalUnlock(memory);
        if !(0..OPEN_TRIES).any(|i| {
            if i > 0 {
                thread::sleep(OPEN_WAIT);
            }
            OpenClipboard(Some(owner)).is_ok()
        }) {
            let _ = GlobalFree(Some(memory));
            return Err(tr("Presse-papiers occupé par un autre programme", "Clipboard busy with another program").into());
        }
        let set = EmptyClipboard().and_then(|_| SetClipboardData(CF_DIB, Some(HANDLE(memory.0))));
        let _ = CloseClipboard();
        // Once set, the memory belongs to the clipboard
        if let Err(e) = set {
            let _ = GlobalFree(Some(memory));
            return Err(e.message());
        }
        Ok(())
    }
}

/// Save: the page's PNG in Pictures\Pastille, named after the date and time like Windows'
/// screenshots; never over another file. Returns its path.
#[tauri::command]
pub async fn note_save(request: Request<'_>) -> Result<String, String> {
    let png = raw(&request)?;
    if png.len() > PNG_MAX || !png.starts_with(&PNG_SIGNATURE) {
        return Err("not a PNG image".into());
    }
    crate::blocking(move || save(&png)).await?
}

fn save(png: &[u8]) -> Result<String, String> {
    let pictures = crate::feed::known_folder(&FOLDERID_Pictures).ok_or_else(|| tr("Dossier Images introuvable", "Pictures folder not found").to_string())?;
    let dir = pictures.join("Pastille");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let t = unsafe { GetLocalTime() };
    let stem = format!(
        "{} {:04}-{:02}-{:02} {:02}{:02}{:02}",
        tr("Post-it", "Sticky note"),
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    );
    // create_new: a file made meanwhile isn't overwritten, the next name is tried
    for n in 1..100 {
        let path = file_name(&dir, &stem, n);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(png).map_err(|e| e.to_string())?;
                return Ok(path.to_string_lossy().into_owned());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("no free file name".into())
}

fn file_name(dir: &Path, stem: &str, n: usize) -> PathBuf {
    dir.join(if n == 1 { format!("{stem}.png") } else { format!("{stem} ({n}).png") })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_from_the_bottom_in_bgra() {
        // 2 × 2: red, green on the first row; blue, white on the second
        let rgba = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
        let d = dib(2, 2, &rgba);
        assert_eq!(d.len(), 40 + 16);
        assert_eq!(&d[0..4], &40u32.to_le_bytes());
        assert_eq!(&d[4..8], &2i32.to_le_bytes());
        assert_eq!(&d[8..12], &2i32.to_le_bytes());
        assert_eq!(&d[14..16], &32u16.to_le_bytes());
        assert_eq!(&d[20..24], &16u32.to_le_bytes());
        // Bottom row first: blue, white; then red, green
        assert_eq!(&d[40..56], &[255, 0, 0, 255, 255, 255, 255, 255, 0, 0, 255, 255, 0, 255, 0, 255]);
    }

    #[test]
    fn names_numbered_after_the_first() {
        let dir = Path::new(r"C:\Pictures\Pastille");
        assert_eq!(file_name(dir, "Post-it 2026-10-25 101500", 1), dir.join("Post-it 2026-10-25 101500.png"));
        assert_eq!(file_name(dir, "Post-it 2026-10-25 101500", 2), dir.join("Post-it 2026-10-25 101500 (2).png"));
    }
}
