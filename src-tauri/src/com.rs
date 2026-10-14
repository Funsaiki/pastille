//! COM (et WinRT) pour les commandes qui en ont besoin : son, lecture en cours.

use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

/// COM sur le fil courant le temps de `f` (les objets COM doivent être libérés dans `f`)
pub fn with_com<T>(f: impl FnOnce() -> T) -> T {
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let result = f();
    if com.is_ok() {
        unsafe { CoUninitialize() };
    }
    result
}

/// Hors du fil principal : les appels COM peuvent attendre
pub async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}
