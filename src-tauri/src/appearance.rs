//! Phone appearance (Customize card, customize.ts; requested by Funsaiki on
//! 2026-10-19): appearance.json next to apps.json (not in apps.json: an older version
//! would reject the field) and the image chosen for the charm, copied next to it
//! (charm.png…). The front sends the whole appearance; Rust keeps only what it recognizes.
//! Until 2026-10-22, French names: apparence.json, breloque.png, keys and charms
//! (config::rename_old_files, serde aliases, OLD_CHARMS).
//! The pastel bar pages receive the new appearance (accent and ink). The phone's tilt,
//! size and position since 2026-10-23 (`tilt`, `size`, `position`, ignored by an older version).

use std::{
    fs,
    ops::RangeInclusive,
    path::{Path, PathBuf},
    sync::Mutex,
};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT,
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, IShellItem, SHGetKnownFolderItem, FOLDERID_Pictures, FOS_FILEMUSTEXIST,
    FOS_FORCEFILESYSTEM, KF_FLAG_DEFAULT, SIGDN_FILESYSPATH,
};

use crate::i18n::tr;
use crate::{config, launcher};

/// Colors as #rrggbb; a missing field: the original one (the accent: the one from apps.json)
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Appearance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accent: Option<String>,
    #[serde(default, alias = "coque", skip_serializing_if = "Option::is_none")]
    case: Option<String>,
    #[serde(default, alias = "ecran", skip_serializing_if = "Option::is_none")]
    screen: Option<String>,
    #[serde(default, alias = "encre", skip_serializing_if = "Option::is_none")]
    ink: Option<String>,
    /// Missing: the Pastille charm
    #[serde(default, alias = "breloque", skip_serializing_if = "Option::is_none")]
    charm: Option<String>,
    /// Three colors, top to bottom
    #[serde(default, alias = "perles", skip_serializing_if = "Option::is_none")]
    beads: Option<Vec<String>>,
    /// Phone's tilt, % of the mockup's (since 2026-10-23); missing: 100
    #[serde(default, deserialize_with = "tilt_value", skip_serializing_if = "Option::is_none")]
    tilt: Option<u8>,
    /// Phone's size, % of the mockup's (since 2026-10-23); missing: 100
    #[serde(default, deserialize_with = "size_value", skip_serializing_if = "Option::is_none")]
    size: Option<u8>,
    /// Phone's place on the screen, left or right (since 2026-10-23); missing: the middle
    #[serde(default, deserialize_with = "text_value", skip_serializing_if = "Option::is_none")]
    position: Option<String>,
}

/// A number in `range`, otherwise dropped (not the whole file, as a wrong type would)
fn number_in<'de, D: serde::Deserializer<'de>>(d: D, range: RangeInclusive<f64>) -> Result<Option<u8>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(d)?;
    Ok(value.and_then(|v| v.as_f64()).filter(|t| range.contains(t)).map(|t| t.round() as u8))
}

fn tilt_value<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u8>, D::Error> {
    number_in(d, 0.0..=100.0)
}

/// From 70 to 120: beyond, the top of the phone would leave the screen
fn size_value<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u8>, D::Error> {
    number_in(d, 70.0..=120.0)
}

/// A text, otherwise dropped
fn text_value<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(d)?;
    Ok(value.and_then(|v| v.as_str().map(str::to_string)))
}

/// Positions known to the front, except the middle (default)
const POSITIONS: [&str; 2] = ["left", "right"];

/// Charms known to the front, except the Pastille one (default)
const CHARMS: [&str; 5] = ["star", "heart", "cat", "image", "none"];
/// Their names until 2026-10-22
const OLD_CHARMS: [(&str, &str); 4] = [("etoile", "star"), ("coeur", "heart"), ("chat", "cat"), ("aucune", "none")];

fn is_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

impl Appearance {
    /// What isn't recognized is dropped (file edited by hand, newer version)
    fn cleaned(mut self) -> Self {
        for color in [&mut self.accent, &mut self.case, &mut self.screen, &mut self.ink] {
            *color = color.take().filter(|c| is_color(c)).map(|c| c.to_ascii_lowercase());
        }
        self.charm = self
            .charm
            .map(|c| OLD_CHARMS.iter().find(|(old, _)| *old == c).map_or(c, |(_, new)| new.to_string()))
            .filter(|c| CHARMS.contains(&c.as_str()));
        self.beads = self
            .beads
            .filter(|p| p.len() == 3 && p.iter().all(|c| is_color(c)))
            .map(|p| p.into_iter().map(|c| c.to_ascii_lowercase()).collect());
        // The mockup's tilt and size are not kept
        self.tilt = self.tilt.filter(|t| *t < 100);
        self.size = self.size.filter(|s| *s != 100);
        self.position = self.position.filter(|p| POSITIONS.contains(&p.as_str()));
        self
    }
}

fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map_err(|e| tr(format!("Dossier de config introuvable : {e}."), format!("Config folder not found: {e}.")))
}

/// File missing or unreadable: the original appearance
fn load(app: &AppHandle) -> Appearance {
    let Ok(text) = dir(app).and_then(|dir| fs::read_to_string(config::to_read(dir.join("appearance.json"))).map_err(|e| e.to_string()))
    else {
        return Appearance::default();
    };
    serde_json::from_str::<Appearance>(text.trim_start_matches('\u{feff}'))
        .map(Appearance::cleaned)
        .unwrap_or_else(|e| {
            eprintln!("[pastille] appearance.json unreadable: {e}");
            Appearance::default()
        })
}

/// Temporary file then rename, like bar.json; one write at a time
fn save(app: &AppHandle, appearance: &Appearance) -> Result<(), String> {
    static WRITING: Mutex<()> = Mutex::new(());
    let _writing = WRITING.lock().unwrap();
    let dir = dir(app)?;
    let path = dir.join("appearance.json");
    let temp = path.with_extension("json.tmp");
    fs::create_dir_all(&dir)
        .and_then(|_| fs::write(&temp, serde_json::to_string_pretty(appearance).unwrap_or_default()))
        .and_then(|_| fs::rename(&temp, &path))
        .map_err(|e| tr(format!("Apparence non enregistrée : {e}."), format!("Appearance not saved: {e}.")))
}

/* ── Charm image ── */

/// Accepted formats: extension and type
const IMAGES: [(&str, &str); 6] = [
    ("png", "image/png"),
    ("svg", "image/svg+xml"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
];
/// It is shown at 52 × 52 px at most
const MAX_IMAGE: u64 = 4 * 1024 * 1024;

/// The start of the file matches its extension
fn looks_like(ext: &str, bytes: &[u8]) -> bool {
    match ext {
        "png" => bytes.starts_with(b"\x89PNG"),
        "jpg" | "jpeg" => bytes.starts_with(&[0xFF, 0xD8]),
        "gif" => bytes.starts_with(b"GIF8"),
        "webp" => bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        "svg" => bytes.windows(4).any(|w| w == b"<svg"),
        _ => false,
    }
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// The image kept in `dir`, as a data URL; under its old name as long as the installed
/// version hasn't renamed it (development)
fn charm_image(dir: &Path) -> Option<String> {
    ["charm", "breloque"].iter().find_map(|name| {
        IMAGES.iter().find_map(|(ext, mime)| {
            let bytes = fs::read(dir.join(format!("{name}.{ext}"))).ok()?;
            (bytes.len() as u64 <= MAX_IMAGE && looks_like(ext, &bytes)).then(|| data_url(mime, &bytes))
        })
    })
}

/// Installed version, at startup (config::rename_old_files): the image of a version from
/// before 2026-10-22 (breloque.png…) replaces the one a development build may have chosen
pub fn rename_old_image(dir: &Path) {
    let Some(ext) = IMAGES.iter().map(|(e, _)| *e).find(|e| dir.join(format!("breloque.{e}")).exists()) else { return };
    for (other, _) in IMAGES {
        let _ = fs::remove_file(dir.join(format!("charm.{other}")));
    }
    if let Err(e) = fs::rename(dir.join(format!("breloque.{ext}")), dir.join(format!("charm.{ext}"))) {
        eprintln!("[pastille] breloque.{ext} not renamed: {e}");
    }
}

/// Copies the chosen image into `dir`, next to apps.json (the original file may be moved
/// or deleted)
fn keep_image(dir: &Path, source: &Path) -> Result<String, String> {
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    let Some(&(ext, mime)) = IMAGES.iter().find(|(e, _)| *e == ext) else {
        return Err(tr("Image PNG, SVG, JPEG, WebP ou GIF seulement.", "PNG, SVG, JPEG, WebP or GIF images only.").into());
    };
    let unreadable = |e: std::io::Error| tr(format!("Lecture de l'image impossible : {e}."), format!("Couldn't read the image: {e}."));
    let size = fs::metadata(source).map_err(unreadable)?.len();
    if size > MAX_IMAGE {
        return Err(tr("Image trop lourde : 4 Mo au plus.", "Image too large: 4 MB at most.").into());
    }
    let bytes = fs::read(source).map_err(unreadable)?;
    if !looks_like(ext, &bytes) {
        let kind = ext.to_ascii_uppercase();
        return Err(tr(format!("Ce fichier n'est pas une image {kind}."), format!("This file is not a {kind} image.")));
    }
    let target = dir.join(format!("charm.{ext}"));
    let temp = dir.join("charm.tmp");
    fs::create_dir_all(dir)
        .and_then(|_| fs::write(&temp, &bytes))
        .and_then(|_| fs::rename(&temp, &target))
        .map_err(|e| tr(format!("Image non enregistrée : {e}."), format!("Image not saved: {e}.")))?;
    // Only one image kept
    for (other, _) in IMAGES.iter().filter(|(e, _)| *e != ext) {
        let _ = fs::remove_file(dir.join(format!("charm.{other}")));
    }
    Ok(data_url(mime, &bytes))
}

/// Windows "Open" dialog, above the launcher (its owner: the launcher, always on top, would
/// hide it otherwise). On a separate thread in a COM single-threaded apartment, as
/// Explorer requires. None: cancelled.
fn pick_file(owner: Option<isize>) -> Result<Option<PathBuf>, String> {
    let com = unsafe { CoInitializeEx(None, COINIT(COINIT_APARTMENTTHREADED.0 | COINIT_DISABLE_OLE1DDE.0)) };
    let result = show_dialog(owner.map(|h| HWND(h as *mut _))).map_err(|e| e.message().trim_end().to_string());
    if com.is_ok() {
        unsafe { CoUninitialize() };
    }
    result
}

fn show_dialog(owner: Option<HWND>) -> windows::core::Result<Option<PathBuf>> {
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        let types = [COMDLG_FILTERSPEC { pszName: w!("Images"), pszSpec: w!("*.png;*.svg;*.jpg;*.jpeg;*.webp;*.gif") }];
        dialog.SetFileTypes(&types)?;
        dialog.SetTitle(&HSTRING::from(tr("Image de la breloque", "Charm image")))?;
        dialog.SetOptions(dialog.GetOptions()? | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM)?;
        // The first time, in Pictures; after that, Windows reopens the last folder
        if let Ok(pictures) = SHGetKnownFolderItem::<IShellItem>(&FOLDERID_Pictures, KF_FLAG_DEFAULT, None) {
            let _ = dialog.SetDefaultFolder(&pictures);
        }
        if let Err(e) = dialog.Show(owner) {
            return if e.code() == ERROR_CANCELLED.to_hresult() { Ok(None) } else { Err(e) };
        }
        let name = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
        let path = name.to_string();
        CoTaskMemFree(Some(name.0 as *const _));
        Ok(path.ok().map(PathBuf::from))
    }
}

/* ── Commands ── */

#[derive(Serialize)]
pub struct Payload {
    appearance: Appearance,
    /// Charm image (data:…); the pastel bar doesn't ask for it
    image: Option<String>,
}

#[tauri::command]
pub async fn get_appearance(app: AppHandle, image: bool) -> Result<Payload, String> {
    crate::blocking(move || Payload {
        appearance: load(&app),
        image: if image { dir(&app).ok().and_then(|dir| charm_image(&dir)) } else { None },
    })
    .await
}

#[tauri::command]
pub async fn set_appearance(app: AppHandle, appearance: Appearance) -> Result<(), String> {
    let appearance = appearance.cleaned();
    let (handle, saved) = (app.clone(), appearance.clone());
    crate::blocking(move || save(&handle, &saved)).await??;
    let _ = app.emit("appearance:changed", appearance);
    Ok(())
}

/// Your image: the Windows "Open" dialog, then the image copied next to apps.json; None if
/// nothing is chosen. The launcher stays shown meanwhile (launcher::set_dialog).
#[tauri::command]
pub async fn pick_charm_image(app: AppHandle) -> Result<Option<String>, String> {
    let main = app.get_webview_window("main");
    let owner = main.as_ref().and_then(|w| w.hwnd().ok()).map(|h| h.0 as isize);
    launcher::set_dialog(true);
    let picked = crate::blocking(move || pick_file(owner)).await;
    launcher::set_dialog(false);
    // Focus returns to the launcher (a click elsewhere during the dialog would have left it without)
    if let Some(w) = main.filter(|w| w.is_visible().unwrap_or(false)) {
        let _ = w.set_focus();
    }
    let Some(path) = picked?? else { return Ok(None) };
    let dir = dir(&app)?;
    crate::blocking(move || keep_image(&dir, &path)).await?.map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_names_are_still_read() {
        let read: Appearance = serde_json::from_str(
            r##"{ "coque": "#5B2C56", "ecran": "#fbe7ef", "encre": "#3a1735", "breloque": "chat",
                 "perles": ["#111111", "#222222", "#333333"] }"##,
        )
        .unwrap();
        let text = serde_json::to_string(&read.cleaned()).unwrap();
        assert_eq!(
            text,
            r##"{"case":"#5b2c56","screen":"#fbe7ef","ink":"#3a1735","charm":"cat","beads":["#111111","#222222","#333333"]}"##
        );
    }

    #[test]
    fn unknown_values_are_dropped() {
        let read: Appearance = serde_json::from_str(
            r##"{ "accent": "#ABCDEF", "case": "red", "ink": "#12345", "charm": "unicorn",
                 "beads": ["#111111", "#222222"], "future": true }"##,
        )
        .unwrap();
        let clean = read.cleaned();
        assert_eq!(clean.accent.as_deref(), Some("#abcdef"));
        assert!(clean.case.is_none() && clean.ink.is_none() && clean.charm.is_none() && clean.beads.is_none());
        assert_eq!(serde_json::to_string(&clean).unwrap(), r##"{"accent":"#abcdef"}"##);
    }

    #[test]
    fn tilt_from_0_to_99_is_kept() {
        let tilt = |json: &str| serde_json::from_str::<Appearance>(json).unwrap().cleaned().tilt;
        assert_eq!(tilt(r#"{ "tilt": 40 }"#), Some(40));
        assert_eq!(tilt(r#"{ "tilt": 0 }"#), Some(0));
        assert_eq!(tilt(r#"{ "tilt": 12.6 }"#), Some(13));
        // The mockup's, out of range or not a number: the mockup's, and the rest is still read
        assert_eq!(tilt(r#"{ "tilt": 100 }"#), None);
        assert_eq!(tilt(r#"{ "tilt": 250 }"#), None);
        assert_eq!(tilt(r#"{ "tilt": -5 }"#), None);
        let read: Appearance = serde_json::from_str(r##"{ "accent": "#abcdef", "tilt": "flat" }"##).unwrap();
        assert_eq!(serde_json::to_string(&read.cleaned()).unwrap(), r##"{"accent":"#abcdef"}"##);
        let read: Appearance = serde_json::from_str(r#"{ "tilt": 35 }"#).unwrap();
        assert_eq!(serde_json::to_string(&read.cleaned()).unwrap(), r#"{"tilt":35}"#);
    }

    #[test]
    fn size_and_position_are_kept() {
        let size = |json: &str| serde_json::from_str::<Appearance>(json).unwrap().cleaned().size;
        assert_eq!(size(r#"{ "size": 70 }"#), Some(70));
        assert_eq!(size(r#"{ "size": 120 }"#), Some(120));
        assert_eq!(size(r#"{ "size": 84.6 }"#), Some(85));
        // The mockup's, out of range or not a number: the mockup's
        assert_eq!(size(r#"{ "size": 100 }"#), None);
        assert_eq!(size(r#"{ "size": 65 }"#), None);
        assert_eq!(size(r#"{ "size": 200 }"#), None);
        assert_eq!(size(r#"{ "size": "big" }"#), None);
        let position = |json: &str| serde_json::from_str::<Appearance>(json).unwrap().cleaned().position;
        assert_eq!(position(r#"{ "position": "left" }"#).as_deref(), Some("left"));
        assert_eq!(position(r#"{ "position": "right" }"#).as_deref(), Some("right"));
        // The middle (default), unknown or not a text: the middle, and the rest is still read
        assert_eq!(position(r#"{ "position": "center" }"#), None);
        assert_eq!(position(r#"{ "position": "top" }"#), None);
        let read: Appearance = serde_json::from_str(r##"{ "accent": "#abcdef", "position": 2 }"##).unwrap();
        assert_eq!(serde_json::to_string(&read.cleaned()).unwrap(), r##"{"accent":"#abcdef"}"##);
        let read: Appearance = serde_json::from_str(r#"{ "size": 90, "position": "left" }"#).unwrap();
        assert_eq!(serde_json::to_string(&read.cleaned()).unwrap(), r#"{"size":90,"position":"left"}"#);
    }

    #[test]
    fn images_are_recognized_by_their_start() {
        assert!(looks_like("png", b"\x89PNG\r\n\x1a\n...."));
        assert!(looks_like("jpeg", &[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(looks_like("svg", b"<?xml version=\"1.0\"?><svg viewBox=\"0 0 1 1\"/>"));
        assert!(looks_like("webp", b"RIFF\x10\0\0\0WEBPVP8 "));
        assert!(!looks_like("png", b"GIF89a"));
        assert!(!looks_like("svg", b"<html></html>"));
    }

    #[test]
    fn one_image_is_kept() {
        let dir = std::env::temp_dir().join(format!("pastille-charm-{}", std::process::id()));
        let source = std::env::temp_dir().join(format!("pastille-source-{}", std::process::id()));
        fs::create_dir_all(&source).unwrap();
        let png = source.join("My Image.PNG");
        fs::write(&png, b"\x89PNG\r\n\x1a\nend").unwrap();
        let svg = source.join("drawing.svg");
        fs::write(&svg, "<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();
        let fake = source.join("fake.gif");
        fs::write(&fake, "not an image").unwrap();

        assert!(charm_image(&dir).is_none());
        let url = keep_image(&dir, &png).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert_eq!(charm_image(&dir), Some(url));
        assert!(keep_image(&dir, &svg).unwrap().starts_with("data:image/svg+xml;base64,"));
        assert!(!dir.join("charm.png").exists() && dir.join("charm.svg").exists());
        assert!(keep_image(&dir, &fake).is_err());
        assert!(keep_image(&dir, &source.join("missing.png")).is_err());
        assert!(charm_image(&dir).unwrap().starts_with("data:image/svg+xml"));

        fs::remove_dir_all(&dir).unwrap();
        fs::remove_dir_all(&source).unwrap();
    }

    /// Diagnostic: the "Open" dialog opens (it appears on screen), then the test closes it
    /// like Cancel after one second. `cargo test pick_dialog_opens -- --ignored`
    #[test]
    #[ignore]
    fn pick_dialog_opens_and_cancels() {
        use std::time::Duration;
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};
        std::thread::spawn(|| {
            for _ in 0..50 {
                std::thread::sleep(Duration::from_millis(100));
                if let Ok(dialog) = unsafe { FindWindowW(w!("#32770"), w!("Image de la breloque")) } {
                    std::thread::sleep(Duration::from_millis(900));
                    let _ = unsafe { PostMessageW(Some(dialog), WM_CLOSE, WPARAM(0), LPARAM(0)) };
                    return;
                }
            }
        });
        assert!(matches!(pick_file(None), Ok(None)));
    }
}
