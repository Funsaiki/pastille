//! Lecture en cours de Spotify, par les contrôles multimédias de Windows (ceux qui
//! s'affichent avec les touches de volume). Spotify seulement, à la demande de Funsaiki.

use serde::Serialize;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};

use crate::com::{blocking, with_com};

#[derive(Serialize)]
pub struct NowPlaying {
    title: String,
    artist: String,
    playing: bool,
}

/// None quand Spotify n'est pas ouvert
#[tauri::command]
pub async fn media() -> Option<NowPlaying> {
    blocking(|| with_com(now_playing).ok().flatten()).await.ok().flatten()
}

/// `toggle` (lecture/pause), `next`, `previous`
#[tauri::command]
pub async fn media_control(action: String) -> Result<(), String> {
    blocking(move || with_com(|| control(&action))).await?
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

fn control(action: &str) -> Result<(), String> {
    let message = |e: windows::core::Error| e.message().trim_end().to_string();
    let session = spotify().map_err(message)?.ok_or("Spotify n'est pas ouvert.")?;
    let request = match action {
        "toggle" => session.TryTogglePlayPauseAsync(),
        "next" => session.TrySkipNextAsync(),
        "previous" => session.TrySkipPreviousAsync(),
        _ => return Err(format!("Commande inconnue : {action}.")),
    };
    request.and_then(|r| r.join()).map(|_| ()).map_err(message)
}

/// Session de Spotify parmi celles des lecteurs ouverts : appli classique (« Spotify.exe »)
/// ou celle du Microsoft Store (« SpotifyAB.SpotifyMusic_…!Spotify », celle de Funsaiki)
fn spotify() -> windows::core::Result<Option<Session>> {
    let sessions = SessionManager::RequestAsync()?.join()?.GetSessions()?;
    Ok(sessions.into_iter().find(|s| is_spotify(&s.SourceAppUserModelId().unwrap_or_default().to_string())))
}

fn is_spotify(app_id: &str) -> bool {
    app_id.to_lowercase().contains("spotify")
}

// Lecture seule : ne jamais mettre en pause ni changer le morceau de Funsaiki dans un test.
#[cfg(test)]
mod tests {
    use super::*;

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
        with_com(|| {
            let start = std::time::Instant::now();
            for s in SessionManager::RequestAsync().unwrap().join().unwrap().GetSessions().unwrap() {
                eprintln!("session : {}", s.SourceAppUserModelId().unwrap());
            }
            match now_playing().unwrap() {
                Some(n) => eprintln!("Spotify : « {} » de {} (lecture : {})", n.title, n.artist, n.playing),
                None => eprintln!("Spotify n'est pas ouvert"),
            }
            eprintln!("durée : {:?}", start.elapsed());
        });
    }
}
