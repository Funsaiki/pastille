//! Apparence du téléphone (carte Personnaliser, customize.ts ; demandée par Funsaiki le
//! 19/10/2026) : apparence.json à côté de apps.json (pas dans apps.json : une version plus
//! ancienne refuserait le champ) et l'image choisie pour la breloque, copiée à côté
//! (breloque.png…). Le front envoie l'apparence entière ; Rust ne garde que ce qu'il reconnaît.
//! Les pages de la barre pastel reçoivent la nouvelle apparence (accent et encre).

use std::{
    fs,
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
use crate::launcher;

/// Couleurs en #rrggbb ; un champ absent : celui d'origine (l'accent : celui de apps.json)
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Appearance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    coque: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ecran: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    encre: Option<String>,
    /// Absente : la pastille
    #[serde(default, skip_serializing_if = "Option::is_none")]
    breloque: Option<String>,
    /// Trois couleurs, de haut en bas
    #[serde(default, skip_serializing_if = "Option::is_none")]
    perles: Option<Vec<String>>,
}

/// Breloques connues du front, sauf la pastille (par défaut)
const CHARMS: [&str; 5] = ["etoile", "coeur", "chat", "image", "aucune"];

fn is_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

impl Appearance {
    /// Ce qui n'est pas reconnu est retiré (fichier modifié à la main, version plus récente)
    fn cleaned(mut self) -> Self {
        for color in [&mut self.accent, &mut self.coque, &mut self.ecran, &mut self.encre] {
            *color = color.take().filter(|c| is_color(c)).map(|c| c.to_ascii_lowercase());
        }
        self.breloque = self.breloque.filter(|b| CHARMS.contains(&b.as_str()));
        self.perles = self
            .perles
            .filter(|p| p.len() == 3 && p.iter().all(|c| is_color(c)))
            .map(|p| p.into_iter().map(|c| c.to_ascii_lowercase()).collect());
        self
    }
}

fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map_err(|e| tr(format!("Dossier de config introuvable : {e}."), format!("Config folder not found: {e}.")))
}

/// Fichier absent ou illisible : l'apparence d'origine
fn load(app: &AppHandle) -> Appearance {
    let Ok(text) = dir(app).and_then(|dir| fs::read_to_string(dir.join("apparence.json")).map_err(|e| e.to_string()))
    else {
        return Appearance::default();
    };
    serde_json::from_str::<Appearance>(text.trim_start_matches('\u{feff}'))
        .map(Appearance::cleaned)
        .unwrap_or_else(|e| {
            eprintln!("[lanceur] apparence.json illisible : {e}");
            Appearance::default()
        })
}

/// Fichier temporaire puis renommage, comme barre.json ; une écriture à la fois
fn save(app: &AppHandle, appearance: &Appearance) -> Result<(), String> {
    static WRITING: Mutex<()> = Mutex::new(());
    let _writing = WRITING.lock().unwrap();
    let dir = dir(app)?;
    let path = dir.join("apparence.json");
    let temp = path.with_extension("json.tmp");
    fs::create_dir_all(&dir)
        .and_then(|_| fs::write(&temp, serde_json::to_string_pretty(appearance).unwrap_or_default()))
        .and_then(|_| fs::rename(&temp, &path))
        .map_err(|e| tr(format!("Apparence non enregistrée : {e}."), format!("Appearance not saved: {e}.")))
}

/* ── Image de la breloque ── */

/// Formats acceptés : extension et type
const IMAGES: [(&str, &str); 6] = [
    ("png", "image/png"),
    ("svg", "image/svg+xml"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
];
/// Elle est affichée en 52 × 52 px au plus
const MAX_IMAGE: u64 = 4 * 1024 * 1024;

/// Le début du fichier correspond à son extension
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

/// L'image gardée dans `dir`, en image intégrable
fn charm_image(dir: &Path) -> Option<String> {
    IMAGES.iter().find_map(|(ext, mime)| {
        let bytes = fs::read(dir.join(format!("breloque.{ext}"))).ok()?;
        (bytes.len() as u64 <= MAX_IMAGE && looks_like(ext, &bytes)).then(|| data_url(mime, &bytes))
    })
}

/// Copie l'image choisie dans `dir`, à côté de apps.json (le fichier d'origine peut être déplacé
/// ou effacé)
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
    let target = dir.join(format!("breloque.{ext}"));
    let temp = dir.join("breloque.tmp");
    fs::create_dir_all(dir)
        .and_then(|_| fs::write(&temp, &bytes))
        .and_then(|_| fs::rename(&temp, &target))
        .map_err(|e| tr(format!("Image non enregistrée : {e}."), format!("Image not saved: {e}.")))?;
    // Une seule image gardée
    for (other, _) in IMAGES.iter().filter(|(e, _)| *e != ext) {
        let _ = fs::remove_file(dir.join(format!("breloque.{other}")));
    }
    Ok(data_url(mime, &bytes))
}

/// Boîte « Ouvrir » de Windows, au-dessus du lanceur (sa propriétaire : le lanceur, toujours
/// au-dessus, la cacherait sinon). Sur un fil à part en appartement COM monothread, comme
/// l'Explorateur le demande. None : annulée.
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
        // La première fois, dans Images ; ensuite, Windows rouvre le dernier dossier
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

/* ── Commandes ── */

#[derive(Serialize)]
pub struct Payload {
    appearance: Appearance,
    /// Image de la breloque (data:…) ; la barre pastel ne la demande pas
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

/// Ton image : la boîte « Ouvrir » de Windows, puis l'image copiée à côté de apps.json ; None si
/// rien n'est choisi. Le lanceur reste affiché pendant ce temps (launcher::set_dialog).
#[tauri::command]
pub async fn pick_charm_image(app: AppHandle) -> Result<Option<String>, String> {
    let main = app.get_webview_window("main");
    let owner = main.as_ref().and_then(|w| w.hwnd().ok()).map(|h| h.0 as isize);
    launcher::set_dialog(true);
    let picked = crate::blocking(move || pick_file(owner)).await;
    launcher::set_dialog(false);
    // Le focus revient au lanceur (un clic ailleurs pendant la boîte l'aurait laissé sans)
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
    fn unknown_values_are_dropped() {
        let read: Appearance = serde_json::from_str(
            r##"{ "accent": "#ABCDEF", "coque": "rouge", "encre": "#12345", "breloque": "licorne",
                 "perles": ["#111111", "#222222"], "futur": true }"##,
        )
        .unwrap();
        let clean = read.cleaned();
        assert_eq!(clean.accent.as_deref(), Some("#abcdef"));
        assert!(clean.coque.is_none() && clean.encre.is_none() && clean.breloque.is_none() && clean.perles.is_none());
        assert_eq!(serde_json::to_string(&clean).unwrap(), r##"{"accent":"#abcdef"}"##);
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
        let dir = std::env::temp_dir().join(format!("pastille-breloque-{}", std::process::id()));
        let source = std::env::temp_dir().join(format!("pastille-source-{}", std::process::id()));
        fs::create_dir_all(&source).unwrap();
        let png = source.join("Mon Image.PNG");
        fs::write(&png, b"\x89PNG\r\n\x1a\nfin").unwrap();
        let svg = source.join("dessin.svg");
        fs::write(&svg, "<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();
        let fake = source.join("faux.gif");
        fs::write(&fake, "pas une image").unwrap();

        assert!(charm_image(&dir).is_none());
        let url = keep_image(&dir, &png).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert_eq!(charm_image(&dir), Some(url));
        assert!(keep_image(&dir, &svg).unwrap().starts_with("data:image/svg+xml;base64,"));
        assert!(!dir.join("breloque.png").exists() && dir.join("breloque.svg").exists());
        assert!(keep_image(&dir, &fake).is_err());
        assert!(keep_image(&dir, &source.join("absente.png")).is_err());
        assert!(charm_image(&dir).unwrap().starts_with("data:image/svg+xml"));

        fs::remove_dir_all(&dir).unwrap();
        fs::remove_dir_all(&source).unwrap();
    }

    /// Diagnostic : la boîte « Ouvrir » s'ouvre (elle apparaît à l'écran), puis le test la ferme
    /// comme Annuler au bout d'une seconde. `cargo test pick_dialog_opens -- --ignored`
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
