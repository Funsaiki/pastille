//! Quick settings view: profile (first name of the Windows account, Windows version), network
//! speed of the physical adapters, CPU and memory load.

use std::{
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use windows::core::{w, PCWSTR, PWSTR};
use windows_registry::LOCAL_MACHINE;
use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_ROW2, MIB_IF_TABLE2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterValue, PdhOpenQueryW,
    PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

/* ── Profile ── */

#[derive(Serialize, Default)]
pub struct Profile {
    /// First name: first word of the account name ("Funsaiki" → "Funsaiki")
    name: String,
    /// "Windows 11" or "Windows 10"
    system: String,
}

/// Off the main thread: the account name may need a directory lookup
#[tauri::command]
pub async fn profile() -> Profile {
    crate::blocking(read_profile).await.unwrap_or_default()
}

fn read_profile() -> Profile {
    let full = display_name().or_else(|| std::env::var("USERNAME").ok()).unwrap_or_default();
    Profile { name: full.split_whitespace().next().unwrap_or_default().to_string(), system: windows_name() }
}

/// Display name of the account (the Microsoft account's if there is one), else None.
fn display_name() -> Option<String> {
    let mut buf = [0u16; 256];
    let mut len = buf.len() as u32;
    // On failure (name too long…), `len` may hold the requested size: don't use it
    if !unsafe { GetUserNameExW(NameDisplay, Some(PWSTR(buf.as_mut_ptr())), &mut len) } {
        return None;
    }
    let name = String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]);
    (!name.trim().is_empty()).then_some(name)
}

/// Windows 11 keeps "Windows 10" in ProductName: we rely on the build number.
fn windows_name() -> String {
    let build: u32 = LOCAL_MACHINE
        .open(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .and_then(|key| key.get_string("CurrentBuild"))
        .ok()
        .and_then(|build| build.parse().ok())
        .unwrap_or(0);
    if build >= 22000 { "Windows 11" } else { "Windows 10" }.into()
}

/* ── Stats: network speed, CPU, memory ── */

#[derive(Default)]
pub struct Meter(Mutex<MeterState>);

#[derive(Default)]
struct MeterState {
    /// Last network sample: (instant, bytes received, bytes sent) since Windows started
    last: Option<(Instant, u64, u64)>,
    /// CPU counter, opened at the first sample
    cpu: Option<Cpu>,
}

#[derive(Serialize, Default)]
pub struct Stats {
    /// Bytes per second
    down: f64,
    up: f64,
    /// Percentages, like Task Manager; None if unreadable
    cpu: Option<u8>,
    ram: Option<u8>,
}

/// Beyond this, the previous sample is from a past opening: we take a new one
const STALE: Duration = Duration::from_secs(3);
/// Length of the first measure on opening
const FIRST_WINDOW: Duration = Duration::from_millis(250);

/// Network speed and CPU load since the previous call (the front polls every
/// second while the quick settings view is shown). Off the main thread: the first measure
/// waits 250 ms.
#[tauri::command]
pub async fn stats(app: AppHandle) -> Stats {
    crate::blocking(move || measure(&app.state::<Meter>())).await.unwrap_or_default()
}

fn measure(meter: &Meter) -> Stats {
    let mut guard = meter.0.lock().unwrap();
    let state = &mut *guard;
    if state.cpu.is_none() {
        state.cpu = Cpu::open();
    }
    let before = match state.last {
        Some(sample) if sample.0.elapsed() < STALE => {
            // View left then reopened right away: measuring over almost no time would multiply the
            // bytes and make the CPU unreadable
            let elapsed = sample.0.elapsed();
            if elapsed < FIRST_WINDOW {
                thread::sleep(FIRST_WINDOW - elapsed);
            }
            sample
        }
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

/// "% Processor Utility" counter, Task Manager's (capped at 100 like it)
struct Cpu {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
}

// PDH handles used under Meter's lock, by one thread at a time
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

    /// Average load since the previous collection
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

/// Physical memory used, in %
fn memory_load() -> Option<u8> {
    let mut status = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
    let used = 1.0 - status.ullAvailPhys as f64 / status.ullTotalPhys.max(1) as f64;
    Some((used * 100.0).round() as u8)
}

/// Bytes received and sent by the connected physical adapters. Virtual adapters
/// (Hyper-V, WSL, Docker, VPN) and the filter layers of each adapter are left out:
/// their traffic already goes through a physical adapter, it would be counted twice.
fn totals() -> (Instant, u64, u64) {
    let (mut down, mut up) = (0, 0);
    for_each_interface(|row| {
        if counted(row) {
            down += row.InOctets;
            up += row.OutOctets;
        }
    });
    (Instant::now(), down, up)
}

fn counted(row: &MIB_IF_ROW2) -> bool {
    const HARDWARE: u8 = 1; // bits of InterfaceAndOperStatusFlags
    const FILTER: u8 = 2;
    let flags = row.InterfaceAndOperStatusFlags._bitfield;
    flags & HARDWARE != 0 && flags & FILTER == 0 && row.OperStatus == IfOperStatusUp
}

/// Rows of the interface table, read in place (no copy) before freeing it
fn for_each_interface(mut f: impl FnMut(&MIB_IF_ROW2)) {
    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    if unsafe { GetIfTable2(&mut table) }.is_err() || table.is_null() {
        return;
    }
    unsafe {
        std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize).iter().for_each(&mut f);
        FreeMibTable(table as *const _);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_has_a_name_and_a_system() {
        let p = read_profile();
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
        // Next sample a second later, like the front: no new wait
        thread::sleep(Duration::from_secs(1));
        let start = Instant::now();
        assert!(measure(&meter).cpu.is_some());
        assert!(start.elapsed() < FIRST_WINDOW, "relevé récent : pas de nouvelle attente");
        // Sample right after the previous one: it waits for the end of the measuring window
        let start = Instant::now();
        measure(&meter);
        assert!(start.elapsed() >= FIRST_WINDOW - Duration::from_millis(20));
    }

    /// Diagnostic: cargo test real_network -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_network() {
        for_each_interface(|row| {
            let alias = String::from_utf16_lossy(&row.Alias).trim_end_matches('\0').to_string();
            if row.InOctets > 0 || counted(row) {
                eprintln!("{} {:>14} {:>14}  {alias}", if counted(row) { "✓" } else { " " }, row.InOctets, row.OutOctets);
            }
        });
        let meter = Meter::default();
        measure(&meter);
        thread::sleep(Duration::from_secs(1));
        let stats = measure(&meter);
        eprintln!("débit : ↓ {:.0} o/s, ↑ {:.0} o/s", stats.down, stats.up);
        eprintln!("processeur : {:?} %, mémoire : {:?} %", stats.cpu, stats.ram);
        let p = read_profile();
        eprintln!("profil : {} / {}", p.name, p.system);
    }
}
