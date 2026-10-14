//! Vue réglages : profil (prénom du compte Windows, version de Windows), débit réseau
//! des cartes physiques, charge du processeur et de la mémoire.

use std::{
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_ROW2, MIB_IF_TABLE2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterValue, PdhOpenQueryW,
    PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

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

/* ── Relevés : débit réseau, processeur, mémoire ── */

#[derive(Default)]
pub struct Meter(Mutex<MeterState>);

#[derive(Default)]
struct MeterState {
    /// Dernier relevé du réseau : (instant, octets reçus, octets envoyés) depuis le démarrage de Windows
    last: Option<(Instant, u64, u64)>,
    /// Compteur du processeur, ouvert au premier relevé
    cpu: Option<Cpu>,
}

#[derive(Serialize, Default)]
pub struct Stats {
    /// Octets par seconde
    down: f64,
    up: f64,
    /// Pourcentages, comme le Gestionnaire des tâches ; None s'ils sont illisibles
    cpu: Option<u8>,
    ram: Option<u8>,
}

/// Au-delà, le relevé précédent date d'une ouverture passée : on en refait un
const STALE: Duration = Duration::from_secs(3);
/// Durée de la première mesure à l'ouverture
const FIRST_WINDOW: Duration = Duration::from_millis(250);

/// Débit et charge du processeur depuis l'appel précédent (le front interroge toutes les
/// secondes quand la vue réglages est affichée). Hors du fil principal : la première mesure
/// attend 250 ms.
#[tauri::command]
pub async fn stats(app: AppHandle) -> Stats {
    tauri::async_runtime::spawn_blocking(move || measure(&app.state::<Meter>()))
        .await
        .unwrap_or_default()
}

fn measure(meter: &Meter) -> Stats {
    let mut guard = meter.0.lock().unwrap();
    let state = &mut *guard;
    if state.cpu.is_none() {
        state.cpu = Cpu::open();
    }
    let before = match state.last {
        Some(sample) if sample.0.elapsed() < STALE => sample,
        _ => {
            let sample = totals();
            if let Some(cpu) = &state.cpu {
                cpu.collect();
            }
            thread::sleep(FIRST_WINDOW);
            sample
        }
    };
    let now = totals();
    state.last = Some(now);
    let seconds = now.0.duration_since(before.0).as_secs_f64().max(0.001);
    Stats {
        down: now.1.saturating_sub(before.1) as f64 / seconds,
        up: now.2.saturating_sub(before.2) as f64 / seconds,
        cpu: state.cpu.as_ref().and_then(Cpu::read),
        ram: memory_load(),
    }
}

/// Compteur « % Processor Utility », celui du Gestionnaire des tâches (plafonné à 100 comme lui)
struct Cpu {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
}

// Poignées PDH utilisées sous le verrou de Meter, par un seul fil à la fois
unsafe impl Send for Cpu {}

impl Cpu {
    fn open() -> Option<Cpu> {
        let mut query = PDH_HQUERY::default();
        if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) } != 0 {
            return None;
        }
        let mut counter = PDH_HCOUNTER::default();
        let path = w!("\\Processor Information(_Total)\\% Processor Utility");
        if unsafe { PdhAddEnglishCounterW(query, path, 0, &mut counter) } != 0 {
            unsafe { PdhCloseQuery(query) };
            return None;
        }
        Some(Cpu { query, counter })
    }

    fn collect(&self) {
        unsafe { PdhCollectQueryData(self.query) };
    }

    /// Charge moyenne depuis la collecte précédente
    fn read(&self) -> Option<u8> {
        let mut value = PDH_FMT_COUNTERVALUE::default();
        unsafe {
            if PdhCollectQueryData(self.query) != 0
                || PdhGetFormattedCounterValue(self.counter, PDH_FMT_DOUBLE, None, &mut value) != 0
            {
                return None;
            }
            Some(value.Anonymous.doubleValue.clamp(0.0, 100.0).round() as u8)
        }
    }
}

impl Drop for Cpu {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.query) };
    }
}

/// Mémoire physique utilisée, en %
fn memory_load() -> Option<u8> {
    let mut status = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
    let used = 1.0 - status.ullAvailPhys as f64 / status.ullTotalPhys.max(1) as f64;
    Some((used * 100.0).round() as u8)
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
        let meter = Meter::default();
        let start = Instant::now();
        let stats = measure(&meter);
        assert!(start.elapsed() >= FIRST_WINDOW);
        assert!(stats.down >= 0.0 && stats.up >= 0.0);
        assert!(stats.cpu.is_some_and(|c| c <= 100), "processeur : {:?}", stats.cpu);
        assert!(stats.ram.is_some_and(|r| r > 0 && r <= 100), "mémoire : {:?}", stats.ram);
        let start = Instant::now();
        assert!(measure(&meter).cpu.is_some());
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
        let meter = Meter::default();
        measure(&meter);
        thread::sleep(Duration::from_secs(1));
        let stats = measure(&meter);
        eprintln!("débit : ↓ {:.0} o/s, ↑ {:.0} o/s", stats.down, stats.up);
        eprintln!("processeur : {:?} %, mémoire : {:?} %", stats.cpu, stats.ram);
        eprintln!("profil : {} / {}", profile().name, profile().system);
    }
}
