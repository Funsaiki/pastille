//! Lecture en cours de Spotify, par les contrôles multimédias de Windows (ceux qui
//! s'affichent avec les touches de volume). Spotify seulement, à la demande de Funsaiki.

use std::sync::Mutex;

use serde::Serialize;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};

use crate::com::com_task;

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
        return Err(format!("Commande inconnue : {action}."));
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
    if sent { Ok(()) } else { Err("Spotify n'est pas ouvert.".into()) }
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
                eprintln!("durée : {:?}", start.elapsed());
            }
        });
    }
}
