//! Machine, drive and theme info. Registry + Win32 reads only.

use serde_json::{json, Value};
use winreg::enums::*;
use winreg::RegKey;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn reg_str(root: &RegKey, path: &str, name: &str) -> Option<String> {
    root.open_subkey(path).ok()?.get_value::<String, _>(name).ok()
}

pub fn drives() -> Vec<Value> {
    use windows_sys::Win32::Storage::FileSystem::*;
    let mut out = vec![];
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26u32 {
        if mask & (1 << i) == 0 { continue; }
        let letter = (b'A' + i as u8) as char;
        let root = format!("{letter}:\\");
        let w = wide(&root);
        let kind = unsafe { GetDriveTypeW(w.as_ptr()) };
        if kind != 3 { continue; } // DRIVE_FIXED
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        let ok = unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) };
        if ok == 0 { continue; }
        let mut label = [0u16; 261];
        let mut fsname = [0u16; 261];
        let (mut serial, mut maxlen, mut flags) = (0u32, 0u32, 0u32);
        unsafe {
            GetVolumeInformationW(w.as_ptr(), label.as_mut_ptr(), 261, &mut serial, &mut maxlen, &mut flags, fsname.as_mut_ptr(), 261);
        }
        let s = |b: &[u16]| String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(0)]);
        out.push(json!({"root": root, "label": s(&label), "fs": s(&fsname), "total": total, "free": free, "used": total - free}));
    }
    out
}

pub fn memory() -> (u64, u64, u32) {
    use windows_sys::Win32::System::SystemInformation::*;
    let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    unsafe { GlobalMemoryStatusEx(&mut m) };
    (m.ullTotalPhys, m.ullAvailPhys, m.dwMemoryLoad)
}

pub fn gpus() -> Vec<Value> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut out = vec![];
    let Ok(cls) = hklm.open_subkey(r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}") else { return out };
    for k in cls.enum_keys().flatten() {
        if k.len() != 4 { continue; }
        let Ok(sub) = cls.open_subkey(&k) else { continue };
        let Ok(desc) = sub.get_value::<String, _>("DriverDesc") else { continue };
        if desc.contains("Basic Display") || desc.contains("Remote") { continue; }
        out.push(json!({
            "name": desc,
            "driverVersion": sub.get_value::<String, _>("DriverVersion").unwrap_or_default(),
            "driverDate": sub.get_value::<String, _>("DriverDate").unwrap_or_default(),
            "vram": sub.get_value::<u64, _>("HardwareInformation.qwMemorySize").ok(),
        }));
    }
    out
}

pub fn machine() -> Value {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let nt = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let bios = r"HARDWARE\DESCRIPTION\System\BIOS";
    let (total, avail, load) = memory();
    let install = hklm.open_subkey(nt).ok().and_then(|k| k.get_value::<u32, _>("InstallDate").ok()).unwrap_or(0);
    let mut product = reg_str(&hklm, nt, "ProductName").unwrap_or_default();
    let build: u32 = reg_str(&hklm, nt, "CurrentBuild").and_then(|b| b.parse().ok()).unwrap_or(0);
    if build >= 22000 { product = product.replace("Windows 10", "Windows 11"); }
    json!({
        "manufacturer": reg_str(&hklm, bios, "SystemManufacturer"),
        "model": reg_str(&hklm, bios, "SystemVersion").filter(|s| !s.is_empty() && s != "Default string")
            .or_else(|| reg_str(&hklm, bios, "SystemProductName")),
        "modelCode": reg_str(&hklm, bios, "SystemProductName"),
        "cpu": reg_str(&hklm, r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString").map(|s| s.trim().to_string()),
        "cores": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        "ram": total, "ramAvail": avail, "ramLoad": load,
        "gpus": gpus(),
        "os": product,
        "osVersion": reg_str(&hklm, nt, "DisplayVersion"),
        "build": build,
        "installDate": install,
        "computerName": std::env::var("COMPUTERNAME").ok(),
    })
}

pub fn theme() -> Value {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let accent = hkcu
        .open_subkey(r"Software\Microsoft\Windows\DWM")
        .ok()
        .and_then(|k| k.get_value::<u32, _>("AccentColor").ok())
        .map(|abgr| format!("#{:02x}{:02x}{:02x}", abgr & 0xff, (abgr >> 8) & 0xff, (abgr >> 16) & 0xff));
    let pers = hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize").ok();
    let light = pers.as_ref().and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme").ok()).map(|v| v == 1);
    let transparency = pers.as_ref().and_then(|k| k.get_value::<u32, _>("EnableTransparency").ok()).unwrap_or(1) == 1;
    let build: u32 = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion").ok()
        .and_then(|k| k.get_value::<String, _>("CurrentBuild").ok()).and_then(|b| b.parse().ok()).unwrap_or(0);
    // Mica needs Windows 11 (22000+) with transparency effects on; otherwise draw a solid window.
    json!({"accent": accent, "light": light, "mica": build >= 22000 && transparency})
}
