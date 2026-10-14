//! Haut de la vue réglages : profil (prénom du compte Windows, version de Windows)
//! et débit réseau des cartes physiques.

use std::{
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use windows::core::{w, PWSTR};
use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_ROW2, MIB_IF_TABLE2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

/* ── Profil ── */

#[derive(Serialize)]
pub struct Profile {
    /// Prénom : premier mot du nom du compte (« Funsaiki » → « Funsaiki »)
    name: String,
    /// « Windows 11 » ou « Windows 10 »
    system: String,
}

#[tauri::command]
pub fn profile() -> Profile {
    let full = display_name().or_else(|| std::env::var("USERNAME").ok()).unwrap_or_default();
    Profile { name: full.split_whitespace().next().unwrap_or_default().to_string(), system: windows_name() }
}

/// Nom affiché du compte (celui du compte Microsoft s'il y en a un), sinon None.
fn display_name() -> Option<String> {
    let mut buf = [0u16; 256];
    let mut len = buf.len() as u32;
    let ok = unsafe { GetUserNameExW(NameDisplay, Some(PWSTR(buf.as_mut_ptr())), &mut len) };
    let name = String::from_utf16_lossy(&buf[..len as usize]);
    (ok && !name.trim().is_empty()).then_some(name)
}

/// Windows 11 garde « Windows 10 » dans ProductName : on se fie au numéro de build.
fn windows_name() -> String {
    let mut buf = [0u16; 32];
    let mut size = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
            w!("CurrentBuild"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    let build: u32 = if status.is_ok() {
        String::from_utf16_lossy(&buf[..(size as usize / 2).saturating_sub(1)]).parse().unwrap_or(0)
    } else {
        0
    };
    if build >= 22000 { "Windows 11" } else { "Windows 10" }.into()
}

/* ── Débit réseau ── */

/// Dernier relevé : (instant, octets reçus, octets envoyés) depuis le démarrage de Windows
#[derive(Default)]
pub struct NetMeter(Mutex<Option<(Instant, u64, u64)>>);

#[derive(Serialize, Default)]
pub struct NetRate {
    /// Octets par seconde
    down: f64,
    up: f64,
}

/// Au-delà, le relevé précédent date d'une ouverture passée : on en refait un
const STALE: Duration = Duration::from_secs(3);
/// Durée de la première mesure à l'ouverture
const FIRST_WINDOW: Duration = Duration::from_millis(250);

/// Débit depuis l'appel précédent (le front interroge toutes les secondes quand le lanceur
/// est visible). Hors du fil principal : la première mesure attend 250 ms.
#[tauri::command]
pub async fn network(app: AppHandle) -> NetRate {
    tauri::async_runtime::spawn_blocking(move || measure(&app.state::<NetMeter>()))
        .await
        .unwrap_or_default()
}

fn measure(meter: &NetMeter) -> NetRate {
    let mut last = meter.0.lock().unwrap();
    let before = match *last {
        Some(sample) if sample.0.elapsed() < STALE => sample,
        _ => {
            let sample = totals();
            thread::sleep(FIRST_WINDOW);
            sample
        }
    };
    let now = totals();
    *last = Some(now);
    let seconds = now.0.duration_since(before.0).as_secs_f64().max(0.001);
    NetRate {
        down: now.1.saturating_sub(before.1) as f64 / seconds,
        up: now.2.saturating_sub(before.2) as f64 / seconds,
    }
}

/// Octets reçus et envoyés par les cartes physiques connectées. Les cartes virtuelles
/// (Hyper-V, WSL, Docker, VPN) et les couches de filtrage de chaque carte sont écartées :
/// leur trafic passe déjà par une carte physique, il serait compté deux fois.
fn totals() -> (Instant, u64, u64) {
    let (mut down, mut up) = (0, 0);
    for row in interfaces().iter().filter(|r| counted(r)) {
        down += row.InOctets;
        up += row.OutOctets;
    }
    (Instant::now(), down, up)
}

fn counted(row: &MIB_IF_ROW2) -> bool {
    const HARDWARE: u8 = 1; // bits de InterfaceAndOperStatusFlags
    const FILTER: u8 = 2;
    let flags = row.InterfaceAndOperStatusFlags._bitfield;
    flags & HARDWARE != 0 && flags & FILTER == 0 && row.OperStatus == IfOperStatusUp
}

fn interfaces() -> Vec<MIB_IF_ROW2> {
    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    if unsafe { GetIfTable2(&mut table) }.is_err() || table.is_null() {
        return Vec::new();
    }
    let rows = unsafe { std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize).to_vec() };
    unsafe { FreeMibTable(table as *const _) };
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_has_a_name_and_a_system() {
        let p = profile();
        assert!(!p.name.is_empty() && !p.name.contains(' '), "{}", p.name);
        assert!(p.system.starts_with("Windows "), "{}", p.system);
    }

    #[test]
    fn first_measure_waits_then_reuses_the_last_sample() {
        let meter = NetMeter::default();
        let start = Instant::now();
        let rate = measure(&meter);
        assert!(start.elapsed() >= FIRST_WINDOW);
        assert!(rate.down >= 0.0 && rate.up >= 0.0);
        let start = Instant::now();
        measure(&meter);
        assert!(start.elapsed() < FIRST_WINDOW, "relevé récent : pas de nouvelle attente");
    }

    /// Diagnostic : cargo test real_network -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_network() {
        for row in interfaces() {
            let alias = String::from_utf16_lossy(&row.Alias).trim_end_matches('\0').to_string();
            if row.InOctets > 0 || counted(&row) {
                eprintln!("{} {:>14} {:>14}  {alias}", if counted(&row) { "✓" } else { " " }, row.InOctets, row.OutOctets);
            }
        }
        let meter = NetMeter::default();
        measure(&meter);
        thread::sleep(Duration::from_secs(1));
        let rate = measure(&meter);
        eprintln!("débit : ↓ {:.0} o/s, ↑ {:.0} o/s", rate.down, rate.up);
        eprintln!("profil : {} / {}", profile().name, profile().system);
    }
}
