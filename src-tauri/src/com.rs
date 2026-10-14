//! COM (et WinRT) pour les commandes qui en ont besoin : son, mélangeur, lecture en cours.

use windows::Win32::System::Com::{CoIncrementMTAUsage, CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

/// Au démarrage : garde l'appartement multithread (MTA) en vie jusqu'à la fin du processus.
/// Sinon chaque commande le créerait puis le détruirait, et le gestionnaire des lecteurs gardé
/// d'un relevé à l'autre (media.rs) survivrait à son appartement.
pub fn keep_mta() {
    // Le jeton n'est jamais rendu
    let _ = unsafe { CoIncrementMTAUsage() };
}

/// COM sur le fil courant le temps de `f` (les objets COM doivent être libérés dans `f`)
pub fn with_com<T>(f: impl FnOnce() -> T) -> T {
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let result = f();
    if com.is_ok() {
        unsafe { CoUninitialize() };
    }
    result
}

/// Hors du fil principal, avec COM. Une erreur devient un message avant CoUninitialize :
/// une erreur COM peut garder un objet (les détails de l'erreur) qui doit être libéré avant.
pub async fn com_task<T: Send + 'static>(
    f: impl FnOnce() -> windows::core::Result<T> + Send + 'static,
) -> Result<T, String> {
    crate::blocking(move || with_com(|| f().map_err(|e| e.message().trim_end().to_string()))).await?
}
