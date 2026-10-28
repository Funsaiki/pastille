//! Mixer (requested by Funsaiki): volume of each app using the default
//! output, like the Windows volume mixer. One audio session per process,
//! grouped by program (Chrome or Discord have several).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use windows::core::{w, Interface, HSTRING, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, S_OK};
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
    /// Program path, or "system" for system sounds
    key: String,
    name: String,
    /// 0 to 100, relative to the master volume as in Windows
    volume: u8,
    /// Program icon (requested by Funsaiki); None for system sounds
    icon: Option<crate::icon::Icon>,
}

const SYSTEM: &str = "system";

/// Apps playing sound first, then by name; system sounds last
#[tauri::command]
pub async fn mixer() -> Vec<AppVolume> {
    com_task(list).await.unwrap_or_default()
}

/// As in Windows: moving the slider unmutes the app if it was muted
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

/// Spotify's volume in the mixer: (program, level), for the now playing card when
/// Spotify isn't connected (spotify.rs); None while Spotify has no audio session
/// (closed, or nothing played yet). To be called with COM.
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
    // Program -> (volume of its first session, one of its sessions is playing sound)
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
    /// Program, or "system"
    key: String,
    volume: ISimpleAudioVolume,
    /// Playing sound right now
    playing: bool,
}

impl Session {
    /// 0 to 100
    fn level(&self) -> windows::core::Result<u8> {
        Ok((unsafe { self.volume.GetMasterVolume()? } * 100.0).round() as u8)
    }
}

/// Sessions of the default output, except those of closed apps
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

/// Program path of a process (also for switch.rs and tray_apps.rs)
pub fn exe_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let path = process_path(process);
        let _ = CloseHandle(process);
        path
    }
}

/// Program path of an already opened process (with PROCESS_QUERY_LIMITED_INFORMATION)
pub fn process_path(process: HANDLE) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) }.ok()?;
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

/// App name: program description ("Google Chrome"), otherwise file name
fn name_of(key: &str) -> String {
    if key == SYSTEM {
        return crate::i18n::tr("Sons système", "System sounds").into();
    }
    program_name(key)
}

/// Name of a program: its description ("Google Chrome"), otherwise its file name. Kept by
/// path: the bar names its windows on every read (every 5 s), and the version information
/// is read from the program's file.
pub fn program_name(path: &str) -> String {
    static NAMES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
    if let Some(name) = NAMES.lock().unwrap().get_or_insert_with(HashMap::new).get(path) {
        return name.clone();
    }
    let name = description(path).unwrap_or_else(|| {
        Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    });
    NAMES.lock().unwrap().get_or_insert_with(HashMap::new).insert(path.to_string(), name.clone());
    name
}

/// FileDescription from the program's version information (also for tray_apps.rs)
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
        // First declared language: (language, code page)
        if !VerQueryValueW(data.as_ptr().cast(), w!("\\VarFileInfo\\Translation"), &mut ptr, &mut len).as_bool() || len < 4 {
            return None;
        }
        let (lang, codepage) = (*(ptr as *const u16), *(ptr as *const u16).add(1));
        let entry = HSTRING::from(format!("\\StringFileInfo\\{lang:04x}{codepage:04x}\\FileDescription"));
        if !VerQueryValueW(data.as_ptr().cast(), &entry, &mut ptr, &mut len).as_bool() || len == 0 {
            return None;
        }
        // Some programs give the length in bytes rather than characters (Spotify):
        // stop at the first null character, without leaving the block read
        let available = (data.as_ptr_range().end as usize).saturating_sub(ptr as usize) / 2;
        let chars = std::slice::from_raw_parts(ptr as *const u16, (len as usize).min(available));
        let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
        let text = String::from_utf16_lossy(&chars[..end]);
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_string())
    }
}

// Read-only: never change the volume of one of Funsaiki's apps in a test.
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
        assert!(is_spotify(r"C:\Users\me\AppData\Roaming\Spotify\spotify.exe"));
        assert!(!is_spotify(r"C:\Outils\spotify-dl.exe"));
        assert!(!is_spotify(SYSTEM));
    }

    /// Diagnostic: cargo test real_mixer -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_mixer() {
        crate::com::with_com(|| {
            let start = std::time::Instant::now();
            let apps = list().unwrap();
            for app in &apps {
                eprintln!("{:>3}%  {}  ({}) icon: {}", app.volume, app.name, app.key, app.icon.is_some());
            }
            // To see the icons: MIXER_JSON=path cargo test real_mixer -- --ignored
            if let Ok(path) = std::env::var("MIXER_JSON") {
                std::fs::write(path, serde_json::to_string(&apps).unwrap()).unwrap();
            }
            eprintln!("time: {:?}", start.elapsed());
        });
    }
}
