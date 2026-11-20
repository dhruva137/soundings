//! Installed programs (registry uninstall keys) and startup entries. Read-only.

use serde_json::{json, Value};
use std::collections::HashSet;
use winreg::enums::*;
use winreg::RegKey;

const OEM: &[&str] = &[
    "lenovo", "dell", "hp inc", "hewlett", "asus", "acer", "msi", "micro-star", "samsung", "razer", "gigabyte",
    "intel", "advanced micro devices", "amd", "nvidia", "realtek", "dolby", "dts", "synaptics", "elan",
    "qualcomm", "mediatek", "killer", "rivet", "waves", "mcafee", "norton", "conexant", "smart sound",
];

fn origin(publisher: &str, name: &str, installed: i64, os_install: i64, sys: bool) -> (&'static str, String) {
    let p = publisher.to_lowercase();
    let n = name.to_lowercase();
    if p.contains("microsoft") && (sys || n.contains("visual c++") || n.contains("runtime") || n.contains("update") || n.contains("edge") || n.contains("webview")) {
        return ("Windows", "Published by Microsoft as part of Windows or a shared runtime".into());
    }
    if let Some(o) = OEM.iter().find(|o| p.contains(*o)) {
        let near = installed > 0 && os_install > 0 && installed <= os_install + 86_400 * 2;
        return ("Manufacturer", if near {
            format!("{} software, present when Windows was first set up", title(o))
        } else {
            format!("Driver or utility from {}", title(o))
        });
    }
    if installed > 0 && os_install > 0 && installed <= os_install + 3600 * 6 {
        return ("Came with PC", "Installed within hours of Windows being set up".into());
    }
    ("You installed", if installed > 0 { "Installed after setup".into() } else { "No install date recorded".into() })
}

fn title(s: &str) -> String {
    s.split(' ').map(|w| {
        let mut c = w.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    }).collect::<Vec<_>>().join(" ")
}

fn parse_date(s: &str) -> i64 {
    // YYYYMMDD -> unix (midnight UTC)
    if s.len() != 8 { return 0; }
    let (y, m, d) = (s[0..4].parse::<i64>().unwrap_or(0), s[4..6].parse::<i64>().unwrap_or(0), s[6..8].parse::<i64>().unwrap_or(0));
    if y < 1990 || !(1..=12).contains(&m) || !(1..=31).contains(&d) { return 0; }
    days_from_civil(y, m, d) * 86_400
}

pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn os_install_date() -> i64 {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .ok()
        .and_then(|k| k.get_value::<u32, _>("InstallDate").ok())
        .unwrap_or(0) as i64
}

pub struct RawApp {
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub location: String,
    pub size: u64,
    pub installed: i64,
    pub system: bool,
    pub scope: &'static str,
}

pub fn raw_apps() -> Vec<RawApp> {
    let mut out = vec![];
    let mut seen = HashSet::new();
    let sources = [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall", "All users"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall", "All users"),
        (HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Uninstall", "Just you"),
    ];
    for (hive, path, scope) in sources {
        let Ok(root) = RegKey::predef(hive).open_subkey(path) else { continue };
        for k in root.enum_keys().flatten() {
            let Ok(sub) = root.open_subkey(&k) else { continue };
            let Ok(name) = sub.get_value::<String, _>("DisplayName") else { continue };
            if sub.get_value::<String, _>("ParentKeyName").is_ok() { continue; } // updates/patches
            let system = sub.get_value::<u32, _>("SystemComponent").unwrap_or(0) == 1;
            let version = sub.get_value::<String, _>("DisplayVersion").unwrap_or_default();
            if !seen.insert((name.to_lowercase(), version.clone())) { continue; }
            out.push(RawApp {
                publisher: sub.get_value::<String, _>("Publisher").unwrap_or_default(),
                location: sub.get_value::<String, _>("InstallLocation").unwrap_or_default().trim_matches('"').to_string(),
                size: sub.get_value::<u32, _>("EstimatedSize").map(|kb| kb as u64 * 1024).unwrap_or(0),
                installed: sub.get_value::<String, _>("InstallDate").map(|s| parse_date(&s)).unwrap_or(0),
                name,
                version,
                system,
                scope,
            });
        }
    }
    out
}

pub fn apps() -> Value {
    let os = os_install_date();
    let list: Vec<Value> = raw_apps()
        .into_iter()
        .map(|a| {
            let (o, why) = origin(&a.publisher, &a.name, a.installed, os, a.system);
            json!({"name": a.name, "publisher": a.publisher, "version": a.version, "location": a.location,
                   "size": a.size, "installed": a.installed, "origin": o, "why": why, "scope": a.scope, "hidden": a.system})
        })
        .collect();
    json!({"apps": list, "osInstall": os})
}

fn rot13(s: &str) -> String {
    s.chars().map(|c| match c {
        'a'..='z' => (((c as u8 - b'a' + 13) % 26) + b'a') as char,
        'A'..='Z' => (((c as u8 - b'A' + 13) % 26) + b'A') as char,
        _ => c,
    }).collect()
}

fn known_folder(p: &str) -> String {
    let map = [
        ("{6D809377-6AF0-444B-8957-A3773F02200E}", r"C:\Program Files"),
        ("{7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E}", r"C:\Program Files (x86)"),
        ("{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}", r"C:\Windows\System32"),
        ("{F38BF404-1D43-42F2-9305-67DE0B28FC23}", r"C:\Windows"),
        ("{D65231B0-B2F1-4857-A4CE-A8E7C6EA7D27}", r"C:\Windows\SysWOW64"),
        ("{0139D44E-6AFE-49F2-8690-3DAFCAE6FFB8}", r"Start menu"),
        ("{A77F5D77-2E2B-44C3-A6A2-ABA601054A51}", r"Start menu"),
        ("{9E3995AB-1F9C-4F13-B827-48B24B6C7174}", r"Pinned"),
    ];
    for (g, r) in map {
        if let Some(rest) = p.strip_prefix(g) { return format!("{r}{rest}"); }
    }
    p.to_string()
}

/// Turn a UserAssist entry (path, shortcut or app ID) into a name people recognise.
fn friendly(path: &str) -> String {
    let last = path.rsplit('\\').next().unwrap_or(path);
    let low = last.to_lowercase();
    let office = [("winword", "Word"), ("excel", "Excel"), ("powerpnt", "PowerPoint"), ("outlook", "Outlook"), ("onenote", "OneNote"), ("msaccess", "Access")];
    for (k, v) in office { if low.contains(&format!("office.{k}")) || low == format!("{k}.exe") { return v.into(); } }
    if low.starts_with("chrome") { return "Google Chrome".into(); }
    if low.starts_with("msedge") || low.contains("microsoftedge") { return "Microsoft Edge".into(); }
    if let Some((pkg, app)) = last.split_once('!') {
        if !app.eq_ignore_ascii_case("app") { return app.to_string(); }
        let p = pkg.split('_').next().unwrap_or(pkg);
        return p.rsplit('.').next().unwrap_or(p).to_string();
    }
    let s = last.trim_end_matches(".exe").trim_end_matches(".EXE").trim_end_matches(".lnk");
    s.to_string()
}

/// Per-user program usage from Explorer's UserAssist counters (launch count, focus time, last run).
pub fn usage() -> Value {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let mut out: Vec<Value> = vec![];
    let base = r"Software\Microsoft\Windows\CurrentVersion\Explorer\UserAssist";
    let Ok(ua) = hkcu.open_subkey(base) else { return json!([]) };
    for g in ua.enum_keys().flatten() {
        let Ok(cnt) = ua.open_subkey(format!(r"{g}\Count")) else { continue };
        for (name, val) in cnt.enum_values().flatten() {
            let b = &val.bytes;
            if b.len() < 68 { continue; }
            let runs = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            let focus_ms = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
            let last = u64::from_le_bytes([b[60], b[61], b[62], b[63], b[64], b[65], b[66], b[67]]);
            let path = known_folder(&rot13(&name));
            if runs == 0 && focus_ms == 0 { continue; }
            if path.starts_with("UEME_") || path.contains("Microsoft.Windows.Explorer") { continue; }
            let disp = friendly(&path);
            out.push(json!({"name": disp, "path": path, "runs": runs, "focusMs": focus_ms, "lastRun": crate::util::ft_to_unix(last)}));
        }
    }
    // the same program can appear as an exe and a shortcut: merge by display name
    let mut merged: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for v in out {
        let k = v["name"].as_str().unwrap_or("").to_lowercase();
        merged.entry(k).and_modify(|e| {
            e["runs"] = json!(e["runs"].as_u64().unwrap_or(0) + v["runs"].as_u64().unwrap_or(0));
            e["focusMs"] = json!(e["focusMs"].as_u64().unwrap_or(0) + v["focusMs"].as_u64().unwrap_or(0));
            if v["lastRun"].as_i64() > e["lastRun"].as_i64() { e["lastRun"] = v["lastRun"].clone(); }
        }).or_insert(v);
    }
    let mut list: Vec<Value> = merged.into_values().collect();
    list.sort_by(|a, b| b["focusMs"].as_u64().cmp(&a["focusMs"].as_u64()));
    json!(list)
}

pub fn startup() -> Value {
    let mut out = vec![];
    let keys = [
        (HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Run", "Just you"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run", "All users"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run", "All users"),
    ];
    // Windows records disabled items here; byte 0 == 2 means enabled
    let approved = |hive, name: &str| -> bool {
        let base = if hive == HKEY_CURRENT_USER { RegKey::predef(HKEY_CURRENT_USER) } else { RegKey::predef(HKEY_LOCAL_MACHINE) };
        for p in [r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run", r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32"] {
            if let Ok(k) = base.open_subkey(p) {
                if let Ok(v) = k.get_raw_value(name) {
                    return v.bytes.first().map(|b| b % 2 == 0).unwrap_or(true);
                }
            }
        }
        true
    };
    for (hive, path, scope) in keys {
        let Ok(k) = RegKey::predef(hive).open_subkey(path) else { continue };
        for (name, val) in k.enum_values().flatten() {
            out.push(json!({"name": name, "command": val.to_string(), "scope": scope, "source": "Registry", "enabled": approved(hive, &name)}));
        }
    }
    let folders = [
        (crate::util::roaming_appdata().join(r"Microsoft\Windows\Start Menu\Programs\Startup"), "Just you"),
        (std::path::PathBuf::from(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\StartUp"), "All users"),
    ];
    for (dir, scope) in folders {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n.eq_ignore_ascii_case("desktop.ini") { continue; }
                out.push(json!({"name": n.trim_end_matches(".lnk"), "command": e.path().to_string_lossy(), "scope": scope, "source": "Startup folder", "enabled": true}));
            }
        }
    }
    json!(out)
}
