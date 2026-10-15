//! Mélangeur (demandé par Funsaiki) : volume de chaque application qui utilise la sortie par
//! défaut, comme le mélangeur de volume de Windows. Une session audio par processus,
//! regroupées par programme (Chrome ou Discord en ont plusieurs).

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use windows::core::{w, Interface, HSTRING, PWSTR};
use windows::Win32::Foundation::{CloseHandle, S_OK};
use windows::Win32::Media::Audio::{
    eRender, AudioSessionStateActive, AudioSessionStateExpired, IAudioSessionControl2, IAudioSessionManager2,
    ISimpleAudioVolume,
};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::audio::default_device;
use crate::com::com_task;

#[derive(Serialize)]
pub struct AppVolume {
    /// Chemin du programme, ou « system » pour les sons système
    key: String,
    name: String,
    /// 0 à 100, relatif au volume principal comme dans Windows
    volume: u8,
    /// Icône du programme (demande de Funsaiki) ; None pour les sons système
    icon: Option<crate::icon::Icon>,
}

const SYSTEM: &str = "system";

/// Applications qui jouent du son d'abord, puis par nom ; sons système à la fin
#[tauri::command]
pub async fn mixer() -> Vec<AppVolume> {
    com_task(list).await.unwrap_or_default()
}

/// Comme dans Windows : bouger le curseur rétablit le son de l'application s'il était coupé
#[tauri::command]
pub async fn set_app_volume(key: String, level: u8) -> Result<(), String> {
    com_task(move || {
        for session in sessions()?.into_iter().filter(|s| s.key == key) {
            unsafe {
                session.volume.SetMasterVolume(f32::from(level.min(100)) / 100.0, std::ptr::null())?;
                if level > 0 {
                    session.volume.SetMute(false, std::ptr::null())?;
                }
            }
        }
        Ok(())
    })
    .await
}

/// Volume de Spotify dans le mélangeur : (programme, niveau), pour la carte de lecture quand
/// Spotify n'est pas connecté (spotify.rs) ; None tant que Spotify n'a pas de session audio
/// (fermé, ou rien joué encore). À appeler avec COM.
pub fn spotify_session() -> windows::core::Result<Option<(String, u8)>> {
    match sessions()?.into_iter().find(|s| is_spotify(&s.key)) {
        Some(session) => {
            let level = session.level()?;
            Ok(Some((session.key, level)))
        }
        None => Ok(None),
    }
}

fn is_spotify(key: &str) -> bool {
    Path::new(key).file_name().is_some_and(|name| name.eq_ignore_ascii_case("spotify.exe"))
}

fn list() -> windows::core::Result<Vec<AppVolume>> {
    // Programme -> (volume de sa première session, l'une de ses sessions joue du son)
    let mut apps: HashMap<String, (u8, bool)> = HashMap::new();
    for session in sessions()? {
        let level = session.level()?;
        apps.entry(session.key).or_insert((level, false)).1 |= session.playing;
    }
    let mut apps: Vec<(AppVolume, bool)> = apps
        .into_iter()
        .map(|(key, (volume, playing))| {
            let icon = if key == SYSTEM { None } else { crate::icon::of(&key) };
            (AppVolume { name: name_of(&key), key, volume, icon }, playing)
        })
        .collect();
    apps.sort_by_key(|(app, playing)| (app.key == SYSTEM, !playing, app.name.to_lowercase()));
    Ok(apps.into_iter().map(|(app, _)| app).collect())
}

struct Session {
    /// Programme, ou « system »
    key: String,
    volume: ISimpleAudioVolume,
    /// Joue du son en ce moment
    playing: bool,
}

impl Session {
    /// 0 à 100
    fn level(&self) -> windows::core::Result<u8> {
        Ok((unsafe { self.volume.GetMasterVolume()? } * 100.0).round() as u8)
    }
}

/// Sessions de la sortie par défaut, sauf celles des applications fermées
fn sessions() -> windows::core::Result<Vec<Session>> {
    let manager: IAudioSessionManager2 = default_device(eRender)?;
    let list = unsafe { manager.GetSessionEnumerator()? };
    let mut out = Vec::new();
    for i in 0..unsafe { list.GetCount()? } {
        let control: IAudioSessionControl2 = unsafe { list.GetSession(i)? }.cast()?;
        let state = unsafe { control.GetState()? };
        if state == AudioSessionStateExpired {
            continue;
        }
        let key = if unsafe { control.IsSystemSoundsSession() } == S_OK {
            SYSTEM.to_string()
        } else {
            match unsafe { control.GetProcessId() }.ok().and_then(exe_path) {
                Some(path) => path,
                None => continue,
            }
        };
        out.push(Session { key, volume: control.cast()?, playing: state == AudioSessionStateActive });
    }
    Ok(out)
}

/// Chemin du programme d'un processus (aussi pour switch.rs)
pub fn exe_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let found = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        found.ok()?;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

/// Nom de l'application : description du programme (« Google Chrome »), sinon nom du fichier
fn name_of(key: &str) -> String {
    if key == SYSTEM {
        return "Sons système".into();
    }
    description(key).unwrap_or_else(|| {
        Path::new(key).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    })
}

/// FileDescription des informations de version du programme (aussi pour tray_apps.rs)
pub fn description(path: &str) -> Option<String> {
    let path = HSTRING::from(path);
    unsafe {
        let size = GetFileVersionInfoSizeW(&path, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&path, None, size, data.as_mut_ptr().cast()).ok()?;
        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;
        // Première langue déclarée : (langue, page de code)
        if !VerQueryValueW(data.as_ptr().cast(), w!("\\VarFileInfo\\Translation"), &mut ptr, &mut len).as_bool() || len < 4 {
            return None;
        }
        let (lang, codepage) = (*(ptr as *const u16), *(ptr as *const u16).add(1));
        let entry = HSTRING::from(format!("\\StringFileInfo\\{lang:04x}{codepage:04x}\\FileDescription"));
        if !VerQueryValueW(data.as_ptr().cast(), &entry, &mut ptr, &mut len).as_bool() || len == 0 {
            return None;
        }
        // Certains programmes donnent la longueur en octets et non en caractères (Spotify) :
        // on s'arrête au premier caractère nul, sans sortir du bloc lu
        let available = (data.as_ptr_range().end as usize).saturating_sub(ptr as usize) / 2;
        let chars = std::slice::from_raw_parts(ptr as *const u16, (len as usize).min(available));
        let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
        let text = String::from_utf16_lossy(&chars[..end]);
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_string())
    }
}

// Lecture seule : ne jamais changer le volume d'une application de Funsaiki dans un test.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(name_of(SYSTEM), "Sons système");
        let windir = std::env::var("SystemRoot").unwrap();
        assert!(description(&format!("{windir}\\System32\\cmd.exe")).is_some());
        assert_eq!(name_of("C:\\nulle-part\\MonJeu.exe"), "MonJeu");
    }

    #[test]
    fn spotify_is_found_by_its_program() {
        assert!(is_spotify(r"C:\Program Files\WindowsApps\SpotifyAB.SpotifyMusic_1.304.258.0_x64__zpdnekdrzrea0\Spotify.exe"));
        assert!(is_spotify(r"C:\Users\johnny\AppData\Roaming\Spotify\spotify.exe"));
        assert!(!is_spotify(r"C:\Outils\spotify-dl.exe"));
        assert!(!is_spotify(SYSTEM));
    }

    /// Diagnostic : cargo test real_mixer -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_mixer() {
        crate::com::with_com(|| {
            let start = std::time::Instant::now();
            let apps = list().unwrap();
            for app in &apps {
                eprintln!("{:>3} %  {}  ({}) icône : {}", app.volume, app.name, app.key, app.icon.is_some());
            }
            // Pour voir les icônes : MIXER_JSON=chemin cargo test real_mixer -- --ignored
            if let Ok(path) = std::env::var("MIXER_JSON") {
                std::fs::write(path, serde_json::to_string(&apps).unwrap()).unwrap();
            }
            eprintln!("durée : {:?}", start.elapsed());
        });
    }
}
