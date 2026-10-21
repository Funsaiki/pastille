//! Mettre en veille, se déconnecter, redémarrer, arrêter.
//! RÈGLE 1 de CLAUDE.md : dans un build de développement, rien n'est exécuté, une ligne de
//! journal seulement. Le code qui agit n'est compilé que dans le build de production.

#[derive(Debug, PartialEq)]
enum Plan {
    Sleep,
    /// shutdown.exe (dans System32) avec ces arguments
    Shutdown(&'static [&'static str]),
}

#[tauri::command]
pub async fn power(action: String) -> Result<(), String> {
    run(plan(&action)?)
}

fn plan(action: &str) -> Result<Plan, String> {
    Ok(match action {
        "sleep" => Plan::Sleep,
        "signout" => Plan::Shutdown(&["/l"]),
        "restart" => Plan::Shutdown(&["/r", "/t", "0"]),
        "shutdown" => Plan::Shutdown(&["/s", "/t", "0"]),
        _ => return Err(crate::i18n::tr(format!("Action inconnue : {action}."), format!("Unknown action: {action}."))),
    })
}

#[cfg(debug_assertions)]
fn run(plan: Plan) -> Result<(), String> {
    eprintln!("[lanceur] alimentation (debug : NON exécuté) : {plan:?}");
    Ok(())
}

#[cfg(not(debug_assertions))]
fn run(plan: Plan) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use windows::Win32::System::Power::{IsPwrSuspendAllowed, SetSuspendState};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    match plan {
        Plan::Sleep => {
            if !unsafe { IsPwrSuspendAllowed() } {
                return Err(crate::i18n::tr("La mise en veille n'est pas disponible sur ce PC.", "Sleep is not available on this PC.").into());
            }
            // Après la descente du lanceur (0,5 s) : sinon il réapparaîtrait au réveil,
            // figé au milieu de son animation. SetSuspendState ne rend la main qu'au réveil.
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(800));
                unsafe { SetSuspendState(false, false, false) };
            });
            Ok(())
        }
        Plan::Shutdown(args) => {
            // Chemin complet : pas de shutdown.exe pris ailleurs dans le PATH
            let windows = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
            let program = PathBuf::from(windows).join("System32").join("shutdown.exe");
            std::process::Command::new(&program)
                .args(args)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("{} : {e}", program.display()))
        }
    }
}

// Ne JAMAIS appeler `power` ou `run` ici : `cargo test --release` exécuterait l'action.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_map_to_system_commands() {
        assert!(plan("lock").is_err(), "retiré le 14/10/2026");
        assert_eq!(plan("sleep"), Ok(Plan::Sleep));
        assert_eq!(plan("signout"), Ok(Plan::Shutdown(&["/l"])));
        assert_eq!(plan("restart"), Ok(Plan::Shutdown(&["/r", "/t", "0"])));
        assert_eq!(plan("shutdown"), Ok(Plan::Shutdown(&["/s", "/t", "0"])));
        assert!(plan("format c:").is_err());
    }
}
