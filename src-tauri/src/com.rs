//! COM (and WinRT) for the commands that need it: sound, mixer, now playing.

use windows::Win32::System::Com::{CoIncrementMTAUsage, CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

/// At startup: keeps the multithreaded apartment (MTA) alive until the process ends.
/// Otherwise each command would create then destroy it, and the media session manager kept
/// from one stats reading to the next (media.rs) would outlive its apartment.
pub fn keep_mta() {
    // The cookie is never released
    let _ = unsafe { CoIncrementMTAUsage() };
}

/// COM on the current thread for the duration of `f` (COM objects must be released in `f`)
pub fn with_com<T>(f: impl FnOnce() -> T) -> T {
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let result = f();
    if com.is_ok() {
        unsafe { CoUninitialize() };
    }
    result
}

/// Off the main thread, with COM. An error becomes a message before CoUninitialize:
/// a COM error may hold an object (the error details) that must be released before.
pub async fn com_task<T: Send + 'static>(
    f: impl FnOnce() -> windows::core::Result<T> + Send + 'static,
) -> Result<T, String> {
    crate::blocking(move || with_com(|| f().map_err(|e| e.message().trim_end().to_string()))).await?
}
