//! Notifications of the quick settings view (Funsaiki's choice): Pastille update (update.rs),
//! finished downloads, Steam activity, PC alerts (restart requested by Windows
//! Update, disk almost full).
//! Windows doesn't give its own notifications to unpackaged apps.
//! Everything is read again on each call, read only.

use std::{
    fs,
    os::windows::fs::MetadataExt,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use serde::Serialize;
use tauri::AppHandle;
use windows::core::{GUID, HSTRING};
use windows_registry::LOCAL_MACHINE;
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

use crate::i18n::tr;
use crate::steam;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Event {
    /// Newer version of Pastille, installed in one click
    Update { version: String },
    /// File that arrived in Downloads; `age` in seconds
    Download { name: String, age: u64 },
    Steam(steam::Activity),
    /// Windows Update is waiting for a restart
    Reboot,
    /// Disk almost full ("C:"), free bytes
    Disk { drive: String, free: u64 },
}

/// Downloads of the last 24 h, three at most
const RECENT: Duration = Duration::from_secs(24 * 3600);
const MAX_DOWNLOADS: usize = 3;
/// Below 10 GB free, a disk is reported
const LOW_DISK: u64 = 10 * 1024 * 1024 * 1024;
/// Volumes under 20 GB are ignored
const SMALL_VOLUME: u64 = 20 * 1024 * 1024 * 1024;
const DRIVE_FIXED: u32 = 3;
const FILE_ATTRIBUTE_HIDDEN: u32 = 2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 4;

/// Update and alerts first, then Steam, then downloads from newest to oldest
#[tauri::command]
pub async fn feed(app: AppHandle) -> Vec<Event> {
    let update = crate::update::available(&app);
    crate::blocking(move || read(update)).await.unwrap_or_default()
}

fn read(update: Option<String>) -> Vec<Event> {
    let mut events = Vec::new();
    if let Some(version) = update {
        events.push(Event::Update { version });
    }
    if reboot_required() {
        events.push(Event::Reboot);
    }
    events.extend(low_disks());
    events.extend(steam::activity().into_iter().map(Event::Steam));
    events.extend(downloads());
    events
}

/// Opens a file from Downloads: the front sends its name, never a path
#[tauri::command]
pub async fn open_download(name: String) -> Result<(), String> {
    if name.is_empty() || name.contains(['/', '\\', ':']) || name == "." || name == ".." {
        return Err(tr(format!("Nom de fichier refusé : {name}."), format!("File name refused: {name}.")));
    }
    let path = downloads_dir().ok_or(tr("Dossier Téléchargements introuvable.", "Downloads folder not found."))?.join(&name);
    if !path.is_file() {
        return Err(tr(format!("{name} n'est plus dans Téléchargements."), format!("{name} is no longer in Downloads.")));
    }
    let path = path.to_string_lossy().into_owned();
    crate::blocking(move || crate::shell::open_as_is(&path)).await?
}

/// The Windows Downloads folder, even if moved by the user
fn downloads_dir() -> Option<PathBuf> {
    known_folder(&FOLDERID_Downloads)
}

/// Windows known folder (Downloads, Program Files…); also for tray_apps.rs
pub fn known_folder(id: &GUID) -> Option<PathBuf> {
    unsafe {
        let path = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let dir = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const _));
        dir.map(PathBuf::from)
    }
}

fn downloads() -> Vec<Event> {
    let Some(entries) = downloads_dir().and_then(|dir| fs::read_dir(dir).ok()) else { return Vec::new() };
    let now = SystemTime::now();
    let mut files: Vec<(Duration, String)> = entries
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let name = e.file_name().to_string_lossy().into_owned();
            let hidden = meta.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0;
            if !meta.is_file() || hidden || unfinished(&name) {
                return None;
            }
            // The latest of creation and modification: a downloaded file may
            // keep an old modification date
            let arrived = meta.modified().ok()?.max(meta.created().ok()?);
            let age = now.duration_since(arrived).ok()?;
            (age < RECENT).then_some((age, name))
        })
        .collect();
    files.sort();
    files.into_iter().take(MAX_DOWNLOADS).map(|(age, name)| Event::Download { name, age: age.as_secs() }).collect()
}

/// Browser downloads still in progress
fn unfinished(name: &str) -> bool {
    let name = name.to_lowercase();
    [".crdownload", ".part", ".partial", ".download", ".opdownload", ".tmp"].iter().any(|ext| name.ends_with(ext))
}

/// The key exists only while Windows Update is waiting for a restart
fn reboot_required() -> bool {
    LOCAL_MACHINE.open(r"SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired").is_ok()
}

fn low_disks() -> Vec<Event> {
    let drives = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| drives & (1 << i) != 0)
        .filter_map(|i| {
            let letter = (b'A' + i) as char;
            let root = HSTRING::from(format!("{letter}:\\"));
            if unsafe { GetDriveTypeW(&root) } != DRIVE_FIXED {
                return None;
            }
            let (mut free, mut total) = (0u64, 0u64);
            unsafe { GetDiskFreeSpaceExW(&root, Some(&mut free), Some(&mut total), None) }.ok()?;
            // Small partitions ("System Reserved", about 100 MB) are always full
            (total >= SMALL_VOLUME && free < LOW_DISK).then(|| Event::Disk { drive: format!("{letter}:"), free })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfinished_downloads_are_skipped() {
        assert!(unfinished("Film.mkv.crdownload"));
        assert!(unfinished("archive.zip.PART"));
        assert!(!unfinished("setup.exe"));
        assert!(!unfinished("rapport.partiel.pdf"));
    }

    #[test]
    fn events_are_tagged() {
        let json = serde_json::to_string(&[
            Event::Update { version: "1.11.0".into() },
            Event::Download { name: "a.zip".into(), age: 60 },
            Event::Reboot,
            Event::Steam(steam::Activity { game: "Hades".into(), progress: Some(40), paused: false, age: None }),
        ])
        .unwrap();
        assert_eq!(
            json,
            r#"[{"kind":"update","version":"1.11.0"},{"kind":"download","name":"a.zip","age":60},{"kind":"reboot"},{"kind":"steam","game":"Hades","progress":40,"paused":false,"age":null}]"#
        );
    }

    /// Diagnostic, read only: cargo test real_feed -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_feed() {
        eprintln!("Downloads: {:?}", downloads_dir());
        eprintln!("{}", serde_json::to_string_pretty(&read(None)).unwrap());
    }
}
