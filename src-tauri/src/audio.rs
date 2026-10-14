//! Volume de la sortie par défaut et micro par défaut, ceux des réglages Son de Windows
//! (IAudioEndpointVolume).

use serde::Serialize;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eCapture, eConsole, eRender, EDataFlow, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

use crate::com::{blocking, with_com};

#[derive(Serialize, Default)]
pub struct AudioState {
    /// 0 à 100 ; None sans sortie audio
    volume: Option<u8>,
    muted: bool,
    /// None sans micro
    mic_muted: Option<bool>,
}

#[tauri::command]
pub async fn audio() -> AudioState {
    blocking(|| {
        with_com(|| -> windows::core::Result<AudioState> {
            let (volume, muted) = match endpoint(eRender) {
                Ok(out) => unsafe {
                    (Some((out.GetMasterVolumeLevelScalar()? * 100.0).round() as u8), out.GetMute()?.as_bool())
                },
                Err(_) => (None, false),
            };
            let mic_muted = endpoint(eCapture).and_then(|mic| unsafe { mic.GetMute() }).ok().map(|m| m.as_bool());
            Ok(AudioState { volume, muted, mic_muted })
        })
        .unwrap_or_default()
    })
    .await
    .unwrap_or_default()
}

/// Comme le curseur de Windows : bouger le volume rétablit le son s'il était coupé.
#[tauri::command]
pub async fn set_volume(level: u8) -> Result<(), String> {
    blocking(move || {
        with_com(|| -> windows::core::Result<()> {
            let out = endpoint(eRender)?;
            unsafe {
                out.SetMasterVolumeLevelScalar(f32::from(level.min(100)) / 100.0, std::ptr::null())?;
                if level > 0 {
                    out.SetMute(false, std::ptr::null())?;
                }
            }
            Ok(())
        })
        .map_err(|e| e.message().trim_end().to_string())
    })
    .await?
}

#[tauri::command]
pub async fn set_mic_muted(muted: bool) -> Result<(), String> {
    blocking(move || {
        with_com(|| unsafe { endpoint(eCapture)?.SetMute(muted, std::ptr::null()) })
            .map_err(|e| e.message().trim_end().to_string())
    })
    .await?
}

/// Appareil par défaut (sortie ou micro) des réglages Son
fn endpoint(flow: EDataFlow) -> windows::core::Result<IAudioEndpointVolume> {
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        devices.GetDefaultAudioEndpoint(flow, eConsole)?.Activate(CLSCTX_ALL, None)
    }
}

// Lecture seule : ne jamais changer le volume ni le micro de Funsaiki dans un test.
#[cfg(test)]
mod tests {
    use super::*;

    /// Diagnostic : cargo test real_audio -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_audio() {
        let state = tauri::async_runtime::block_on(audio());
        eprintln!("volume : {:?}, coupé : {}, micro coupé : {:?}", state.volume, state.muted, state.mic_muted);
        assert!(state.volume.is_some_and(|v| v <= 100));
    }
}
