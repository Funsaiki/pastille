//! Sleep, sign out, restart, shut down.
//! RULE 1 of CLAUDE.md: in a development build, nothing is run, only a log
//! line. The code that acts is compiled only in the production build.

#[derive(Debug, PartialEq)]
enum Plan {
    Sleep,
    /// shutdown.exe (in System32) with these arguments
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
    eprintln!("[pastille] power (debug: NOT run): {plan:?}");
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
            // After the launcher sinks (0.5 s): otherwise it would reappear on wake,
            // frozen in the middle of its animation. SetSuspendState only returns on wake.
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(800));
                unsafe { SetSuspendState(false, false, false) };
            });
            Ok(())
        }
        Plan::Shutdown(args) => {
            // Full path: no shutdown.exe picked up elsewhere in the PATH
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

// NEVER call `power` or `run` here: `cargo test --release` would run the action.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_map_to_system_commands() {
        assert!(plan("lock").is_err(), "removed on 2026-10-14");
        assert_eq!(plan("sleep"), Ok(Plan::Sleep));
        assert_eq!(plan("signout"), Ok(Plan::Shutdown(&["/l"])));
        assert_eq!(plan("restart"), Ok(Plan::Shutdown(&["/r", "/t", "0"])));
        assert_eq!(plan("shutdown"), Ok(Plan::Shutdown(&["/s", "/t", "0"])));
        assert!(plan("format c:").is_err());
    }
}
