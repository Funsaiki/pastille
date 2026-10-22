//! Volume of the default output and default microphone, those of the Windows Sound settings
//! (IAudioEndpointVolume).

use serde::Serialize;
use windows::core::Interface;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eCapture, eConsole, eRender, EDataFlow, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

use crate::com::com_task;

#[derive(Serialize, Default)]
pub struct AudioState {
    /// 0 to 100; None without an audio output
    volume: Option<u8>,
    muted: bool,
    /// None without a microphone
    mic_muted: Option<bool>,
}

#[tauri::command]
pub async fn audio() -> AudioState {
    com_task(|| {
        let (volume, muted) = match endpoint(eRender) {
            Ok(out) => unsafe {
                (Some((out.GetMasterVolumeLevelScalar()? * 100.0).round() as u8), out.GetMute()?.as_bool())
            },
            Err(_) => (None, false),
        };
        let mic_muted = endpoint(eCapture).and_then(|mic| unsafe { mic.GetMute() }).ok().map(|m| m.as_bool());
        Ok(AudioState { volume, muted, mic_muted })
    })
    .await
    .unwrap_or_default()
}

/// Like the Windows slider: moving the volume unmutes the sound if it was muted.
#[tauri::command]
pub async fn set_volume(level: u8) -> Result<(), String> {
    com_task(move || {
        let out = endpoint(eRender)?;
        unsafe {
            out.SetMasterVolumeLevelScalar(f32::from(level.min(100)) / 100.0, std::ptr::null())?;
            if level > 0 {
                out.SetMute(false, std::ptr::null())?;
            }
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn set_mic_muted(muted: bool) -> Result<(), String> {
    com_task(move || unsafe { endpoint(eCapture)?.SetMute(muted, std::ptr::null()) }).await
}

/// Volume of the default output or microphone
fn endpoint(flow: EDataFlow) -> windows::core::Result<IAudioEndpointVolume> {
    default_device(flow)
}

/// Interface `T` of the default device (output or microphone) in the Sound settings
pub fn default_device<T: Interface>(flow: EDataFlow) -> windows::core::Result<T> {
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        devices.GetDefaultAudioEndpoint(flow, eConsole)?.Activate(CLSCTX_ALL, None)
    }
}

// Read-only: never change Funsaiki's volume or microphone in a test.
#[cfg(test)]
mod tests {
    use super::*;

    /// Diagnostic: cargo test real_audio -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_audio() {
        let state = tauri::async_runtime::block_on(audio());
        eprintln!("volume: {:?}, muted: {}, mic muted: {:?}", state.volume, state.muted, state.mic_muted);
        assert!(state.volume.is_some_and(|v| v <= 100));
    }
}
