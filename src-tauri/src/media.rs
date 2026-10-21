//! Lecture en cours de Spotify, par les contrôles multimédias de Windows (ceux qui
//! s'affichent avec les touches de volume). Spotify seulement, à la demande de Funsaiki.

use std::sync::Mutex;

use base64::Engine;
use serde::Serialize;
use windows::Storage::Streams::{Buffer, DataReader, InputStreamOptions};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};

use crate::com::com_task;
use crate::i18n::tr;

#[derive(Serialize)]
pub struct NowPlaying {
    title: String,
    artist: String,
    playing: bool,
}

/// None quand Spotify n'est pas ouvert
#[tauri::command]
pub async fn media() -> Option<NowPlaying> {
    com_task(now_playing).await.ok().flatten()
}

/// `toggle` (lecture/pause), `next`, `previous`
#[tauri::command]
pub async fn media_control(action: String) -> Result<(), String> {
    if !matches!(action.as_str(), "toggle" | "next" | "previous") {
        return Err(tr(format!("Commande inconnue : {action}."), format!("Unknown command: {action}.")));
    }
    let sent = com_task(move || {
        let Some(session) = spotify()? else { return Ok(false) };
        let request = match action.as_str() {
            "toggle" => session.TryTogglePlayPauseAsync()?,
            "next" => session.TrySkipNextAsync()?,
            _ => session.TrySkipPreviousAsync()?,
        };
        request.join()?;
        Ok(true)
    })
    .await?;
    if sent { Ok(()) } else { Err(tr("Spotify n'est pas ouvert.", "Spotify is not open.").into()) }
}

/// Pochette du morceau en cours, en image intégrable (data:…) ; None sans pochette. Le front
/// ne la demande qu'au changement de morceau.
#[tauri::command]
pub async fn media_cover() -> Option<String> {
    com_task(cover).await.ok().flatten()
}

/// Au-delà, ce n'est plus une pochette (Spotify en donne de 300 × 300, quelques dizaines de Ko)
const MAX_COVER: u64 = 2 * 1024 * 1024;

fn cover() -> windows::core::Result<Option<String>> {
    let Some(session) = spotify()? else { return Ok(None) };
    let props = session.TryGetMediaPropertiesAsync()?.join()?;
    let Ok(thumbnail) = props.Thumbnail() else { return Ok(None) };   // pas de pochette
    let stream = thumbnail.OpenReadAsync()?.join()?;
    let size = stream.Size()?;
    if size == 0 || size > MAX_COVER {
        return Ok(None);
    }
    let read = stream.ReadAsync(&Buffer::Create(size as u32)?, size as u32, InputStreamOptions::None)?.join()?;
    let mut bytes = vec![0u8; read.Length()? as usize];
    DataReader::FromBuffer(&read)?.ReadBytes(&mut bytes)?;
    let mime = stream
        .ContentType()
        .map(|t| t.to_string())
        .ok()
        .filter(|t| t.starts_with("image/"))
        .unwrap_or_else(|| "image/jpeg".into());
    Ok(Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))))
}

fn now_playing() -> windows::core::Result<Option<NowPlaying>> {
    let Some(session) = spotify()? else { return Ok(None) };
    let props = session.TryGetMediaPropertiesAsync()?.join()?;
    Ok(Some(NowPlaying {
        title: props.Title()?.to_string(),
        artist: props.Artist()?.to_string(),
        playing: session.GetPlaybackInfo()?.PlaybackStatus()? == PlaybackStatus::Playing,
    }))
}

/// Gestionnaire des lecteurs, gardé d'un relevé à l'autre : le demander coûte ~5 ms, le relevé
/// a lieu toutes les secondes. L'appartement multithread reste en vie (com::keep_mta).
static MANAGER: Mutex<Option<SessionManager>> = Mutex::new(None);

fn manager() -> windows::core::Result<SessionManager> {
    let mut slot = MANAGER.lock().unwrap();
    if let Some(manager) = slot.as_ref() {
        return Ok(manager.clone());
    }
    let manager = SessionManager::RequestAsync()?.join()?;
    *slot = Some(manager.clone());
    Ok(manager)
}

/// Session de Spotify parmi celles des lecteurs ouverts : appli classique (« Spotify.exe »)
/// ou celle du Microsoft Store (« SpotifyAB.SpotifyMusic_…!Spotify », celle de Funsaiki)
fn spotify() -> windows::core::Result<Option<Session>> {
    let sessions = match manager().and_then(|m| m.GetSessions()) {
        Ok(sessions) => sessions,
        Err(e) => {
            *MANAGER.lock().unwrap() = None;   // service redémarré… : redemandé au prochain relevé
            return Err(e);
        }
    };
    Ok(sessions.into_iter().find(|s| is_spotify(&s.SourceAppUserModelId().unwrap_or_default().to_string())))
}

fn is_spotify(app_id: &str) -> bool {
    app_id.to_lowercase().contains("spotify")
}

// Lecture seule : ne jamais mettre en pause ni changer le morceau de Funsaiki dans un test.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::com::with_com;

    #[test]
    fn spotify_ids() {
        assert!(is_spotify("Spotify.exe"));
        assert!(is_spotify("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"));
        assert!(!is_spotify("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"));
        assert!(!is_spotify("chrome.exe"));
    }

    /// Diagnostic : cargo test real_media -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_media() {
        crate::com::keep_mta();
        with_com(|| {
            for s in manager().unwrap().GetSessions().unwrap() {
                eprintln!("session : {}", s.SourceAppUserModelId().unwrap());
            }
            for _ in 0..2 {
                let start = std::time::Instant::now();
                match now_playing().unwrap() {
                    Some(n) => eprintln!("Spotify : « {} » de {} (lecture : {})", n.title, n.artist, n.playing),
                    None => eprintln!("Spotify n'est pas ouvert"),
                }
                let cover = cover().unwrap();
                eprintln!("pochette : {:?}", cover.as_deref().map(|c| (&c[..c.find(',').unwrap_or(0)], c.len())));
                eprintln!("durée : {:?}", start.elapsed());
            }
        });
    }
}
