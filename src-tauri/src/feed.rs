//! Notifications de la vue réglages (choix de Funsaiki) : téléchargements terminés, activité de
//! Steam, alertes du PC (redémarrage demandé par Windows Update, disque presque plein).
//! Windows ne donne pas ses propres notifications aux applications non empaquetées.
//! Tout est relu à chaque appel, en lecture seule.

use std::{
    fs,
    os::windows::fs::MetadataExt,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use serde::Serialize;
use windows::core::{GUID, HSTRING};
use windows_registry::LOCAL_MACHINE;
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

use crate::steam;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Event {
    /// Fichier arrivé dans Téléchargements ; `age` en secondes
    Download { name: String, age: u64 },
    Steam(steam::Activity),
    /// Windows Update attend un redémarrage
    Reboot,
    /// Disque presque plein (« C: »), octets libres
    Disk { drive: String, free: u64 },
}

/// Téléchargements des dernières 24 h, trois au plus
const RECENT: Duration = Duration::from_secs(24 * 3600);
const MAX_DOWNLOADS: usize = 3;
/// En dessous de 10 Go libres, un disque est signalé
const LOW_DISK: u64 = 10 * 1024 * 1024 * 1024;
/// Volumes de moins de 20 Go ignorés
const SMALL_VOLUME: u64 = 20 * 1024 * 1024 * 1024;
const DRIVE_FIXED: u32 = 3;
const FILE_ATTRIBUTE_HIDDEN: u32 = 2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 4;

/// Alertes d'abord, puis Steam, puis les téléchargements du plus récent au plus ancien
#[tauri::command]
pub async fn feed() -> Vec<Event> {
    crate::blocking(|| {
        let mut events = Vec::new();
        if reboot_required() {
            events.push(Event::Reboot);
        }
        events.extend(low_disks());
        events.extend(steam::activity().into_iter().map(Event::Steam));
        events.extend(downloads());
        events
    })
    .await
    .unwrap_or_default()
}

/// Ouvre un fichier de Téléchargements : le front envoie son nom, jamais un chemin
#[tauri::command]
pub async fn open_download(name: String) -> Result<(), String> {
    if name.is_empty() || name.contains(['/', '\\', ':']) || name == "." || name == ".." {
        return Err(format!("Nom de fichier refusé : {name}."));
    }
    let path = downloads_dir().ok_or("Dossier Téléchargements introuvable.")?.join(&name);
    if !path.is_file() {
        return Err(format!("{name} n'est plus dans Téléchargements."));
    }
    let path = path.to_string_lossy().into_owned();
    crate::blocking(move || crate::shell::open_as_is(&path)).await?
}

/// Dossier Téléchargements de Windows, même déplacé par l'utilisateur
fn downloads_dir() -> Option<PathBuf> {
    known_folder(&FOLDERID_Downloads)
}

/// Dossier connu de Windows (Téléchargements, Program Files…) ; aussi pour tray_apps.rs
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
            // Le plus récent de la création et de la modification : un fichier téléchargé peut
            // garder une date de modification ancienne
            let arrived = meta.modified().ok()?.max(meta.created().ok()?);
            let age = now.duration_since(arrived).ok()?;
            (age < RECENT).then_some((age, name))
        })
        .collect();
    files.sort();
    files.into_iter().take(MAX_DOWNLOADS).map(|(age, name)| Event::Download { name, age: age.as_secs() }).collect()
}

/// Téléchargements encore en cours des navigateurs
fn unfinished(name: &str) -> bool {
    let name = name.to_lowercase();
    [".crdownload", ".part", ".partial", ".download", ".opdownload", ".tmp"].iter().any(|ext| name.ends_with(ext))
}

/// La clé n'existe que tant que Windows Update attend un redémarrage
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
            // Les petites partitions (« Réservé au système », 100 Mo chez Funsaiki) sont toujours pleines
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
            Event::Download { name: "a.zip".into(), age: 60 },
            Event::Reboot,
            Event::Steam(steam::Activity { game: "Hades".into(), progress: Some(40), paused: false, age: None }),
        ])
        .unwrap();
        assert_eq!(
            json,
            r#"[{"kind":"download","name":"a.zip","age":60},{"kind":"reboot"},{"kind":"steam","game":"Hades","progress":40,"paused":false,"age":null}]"#
        );
    }

    /// Diagnostic, lecture seule : cargo test real_feed -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_feed() {
        eprintln!("Téléchargements : {:?}", downloads_dir());
        eprintln!("{}", serde_json::to_string_pretty(&tauri::async_runtime::block_on(feed())).unwrap());
    }
}
