//! Crash & GPU-error history from the Windows event logs, plus the opt-in background recorder.
//!
//! Recorder design: a Task Scheduler task with event triggers (zero idle cost) runs
//! `soundings.exe --record-events` only when a matching event is written, and at logon.
//! The recorder reads everything newer than its bookmark, appends JSON lines to
//! %LOCALAPPDATA%\Soundings\stability\events.jsonl and shows one toast per new incident.

use crate::util::{data_dir, run_hidden, CREATE_NO_WINDOW};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::time::Duration;

const SYSTEM_SELECTS: &[&str] = &[
    "*[System[Provider[@Name='Microsoft-Windows-Kernel-Power'] and (EventID=41)]]",
    "*[System[Provider[@Name='EventLog'] and (EventID=6008)]]",
    "*[System[Provider[@Name='Microsoft-Windows-WER-SystemErrorReporting'] and (EventID=1001)]]",
    "*[System[Provider[@Name='Display'] and (EventID=4101)]]",
    "*[System[Provider[@Name='nvlddmkm' or @Name='amdkmdag' or @Name='amdkmdap' or @Name='igfx' or @Name='igfxn' or @Name='igfxnd']]]",
    "*[System[Provider[@Name='Microsoft-Windows-DxgKrnl'] and (Level=1 or Level=2)]]",
    "*[System[Provider[@Name='Microsoft-Windows-WHEA-Logger']]]",
    "*[System[Provider[@Name='Microsoft-Windows-Resource-Exhaustion-Detector'] and (EventID=2004)]]",
    "*[System[Provider[@Name='disk'] and (EventID=7 or EventID=11 or EventID=51 or EventID=153)]]",
    "*[System[Provider[@Name='Microsoft-Windows-Ntfs' or @Name='Ntfs'] and (EventID=55 or EventID=98) and (Level=1 or Level=2 or Level=3)]]",
    "*[System[Provider[@Name='Microsoft-Windows-Kernel-Processor-Power'] and (EventID=37)]]",
    "*[System[Provider[@Name='Microsoft-Windows-UserPnp'] and (EventID=20001 or EventID=20003)]]",
    "*[System[Provider[@Name='Microsoft-Windows-WindowsUpdateClient'] and (EventID=19)]]",
];
const APP_SELECTS: &[&str] = &[
    "*[System[Provider[@Name='Application Error'] and (EventID=1000)]]",
    "*[System[Provider[@Name='Application Hang'] and (EventID=1002)]]",
    "*[System[Provider[@Name='Windows Error Reporting'] and (EventID=1001)]]",
];

fn with_time(sel: &str, ms: Option<u64>) -> String {
    match ms {
        Some(ms) => sel.replacen("[System[", &format!("[System[TimeCreated[timediff(@SystemTime) <= {ms}] and "), 1),
        None => sel.to_string(),
    }
}

fn esc(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }

pub fn query_xml(channel: &str, sels: &[&str], ms: Option<u64>) -> String {
    let mut q = format!("<QueryList><Query Id=\"0\" Path=\"{channel}\">");
    for s in sels {
        q.push_str(&format!("<Select Path=\"{channel}\">{}</Select>", esc(&with_time(s, ms))));
    }
    q.push_str("</Query></QueryList>");
    q
}

// ---------------- event log access (wevtapi) ----------------
fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }

pub fn read_events(query: &str, limit: usize) -> Vec<String> {
    use windows_sys::Win32::System::EventLog::*;
    let mut out = vec![];
    let q = wide(query);
    unsafe {
        let h = EvtQuery(0 as _, std::ptr::null(), q.as_ptr(), (EvtQueryChannelPath | EvtQueryReverseDirection) as u32);
        if h as isize == 0 { return out; }
        let mut buf: Vec<u16> = vec![0; 32 * 1024];
        loop {
            let mut evs: [EVT_HANDLE; 64] = std::mem::zeroed();
            let mut got = 0u32;
            if EvtNext(h, 64, evs.as_mut_ptr(), 2000, 0, &mut got) == 0 { break; }
            for ev in evs.iter().take(got as usize) {
                let mut used = 0u32;
                let mut props = 0u32;
                let bytes = (buf.len() * 2) as u32;
                let mut ok = EvtRender(0 as _, *ev, EvtRenderEventXml as u32, bytes, buf.as_mut_ptr() as _, &mut used, &mut props);
                if ok == 0 && used > bytes {
                    buf = vec![0; used as usize / 2 + 1];
                    ok = EvtRender(0 as _, *ev, EvtRenderEventXml as u32, used, buf.as_mut_ptr() as _, &mut used, &mut props);
                }
                if ok != 0 {
                    let n = (used as usize / 2).saturating_sub(1);
                    out.push(String::from_utf16_lossy(&buf[..n]));
                }
                EvtClose(*ev);
            }
            if out.len() >= limit { break; }
        }
        EvtClose(h);
    }
    out
}

// ---------------- tiny XML field extraction ----------------
fn attr(xml: &str, tag: &str, name: &str) -> Option<String> {
    let i = xml.find(&format!("<{tag} "))?;
    let seg = &xml[i..i + xml[i..].find('>')?];
    let k = seg.find(&format!("{name}="))? + name.len() + 1;
    let q = seg[k..].chars().next()?;
    let rest = &seg[k + 1..];
    Some(unescape(&rest[..rest.find(q)?]))
}
fn elem(xml: &str, tag: &str) -> Option<String> {
    let i = xml.find(&format!("<{tag}"))?;
    let s = i + xml[i..].find('>')? + 1;
    if xml[..s].ends_with("/>") { return Some(String::new()); }
    let e = s + xml[s..].find(&format!("</{tag}>"))?;
    Some(unescape(&xml[s..e]))
}
fn data(xml: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut pos = 0;
    while let Some(i) = xml[pos..].find("<Data") {
        let start = pos + i;
        let Some(gt) = xml[start..].find('>') else { break };
        let head = &xml[start..start + gt];
        let name = head.find("Name='").or_else(|| head.find("Name=\"")).map(|k| {
            let r = &head[k + 6..];
            r[..r.find(['\'', '"']).unwrap_or(r.len())].to_string()
        }).unwrap_or_default();
        if head.ends_with('/') {
            out.push((name, String::new()));
            pos = start + gt + 1;
            continue;
        }
        let vs = start + gt + 1;
        let Some(ve) = xml[vs..].find("</Data>") else { break };
        out.push((name, unescape(&xml[vs..vs + ve])));
        pos = vs + ve;
    }
    out
}
fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&#xD;", "").replace("&#xA;", " ").replace("&amp;", "&")
}

fn bugcheck_name(code: u32) -> &'static str {
    match code {
        0x116 => "VIDEO_TDR_FAILURE", 0x117 => "VIDEO_TDR_TIMEOUT_DETECTED", 0x10E => "VIDEO_MEMORY_MANAGEMENT_INTERNAL",
        0x141 => "VIDEO_ENGINE_TIMEOUT_DETECTED", 0x1A => "MEMORY_MANAGEMENT", 0x3B => "SYSTEM_SERVICE_EXCEPTION",
        0xD1 => "DRIVER_IRQL_NOT_LESS_OR_EQUAL", 0x133 => "DPC_WATCHDOG_VIOLATION", 0x124 => "WHEA_UNCORRECTABLE_ERROR",
        0x9F => "DRIVER_POWER_STATE_FAILURE", 0x7E => "SYSTEM_THREAD_EXCEPTION_NOT_HANDLED", 0x50 => "PAGE_FAULT_IN_NONPAGED_AREA",
        0xEF => "CRITICAL_PROCESS_DIED", 0x139 => "KERNEL_SECURITY_CHECK_FAILURE", 0xA => "IRQL_NOT_LESS_OR_EQUAL",
        0x1E => "KMODE_EXCEPTION_NOT_HANDLED", 0x19 => "BAD_POOL_HEADER", 0x154 => "UNEXPECTED_STORE_EXCEPTION",
        _ => "",
    }
}

fn first_hex(s: &str) -> Option<u32> {
    let i = s.find("0x")?;
    let h: String = s[i + 2..].chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    u32::from_str_radix(&h, 16).ok()
}

const GPU_MODULES: &[&str] = &["nvwgf2um", "nvoglv", "nvcuda", "nvlddmkm", "cudnn", "cublas", "torch", "amdxx", "atiumd", "amdvlk", "igd", "d3d", "dxgi", "nvapi"];

/// Normalise one rendered event into our record, or None if it is noise.
pub fn classify(xml: &str) -> Option<Value> {
    let provider = attr(xml, "Provider", "Name")?;
    let id: u32 = elem(xml, "EventID")?.trim().parse().ok()?;
    let time = attr(xml, "TimeCreated", "SystemTime").unwrap_or_default();
    let channel = elem(xml, "Channel").unwrap_or_default();
    let rid = elem(xml, "EventRecordID").unwrap_or_default();
    let d = data(xml);
    let dv = |n: &str| d.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone()).unwrap_or_default();
    let di = |i: usize| d.get(i).map(|(_, v)| v.clone()).unwrap_or_default();
    let all = d.iter().map(|(_, v)| v.as_str()).collect::<Vec<_>>().join(" ");

    let (cat, sev, summary): (&str, &str, String) = match (provider.as_str(), id) {
        ("Microsoft-Windows-Kernel-Power", 41) => {
            let bc: u64 = dv("BugcheckCode").parse().unwrap_or(0);
            if bc != 0 {
                ("unexpected-shutdown", "critical", format!("Restarted after a blue screen (0x{bc:X})"))
            } else {
                ("unexpected-shutdown", "critical", "Restarted without shutting down cleanly (froze, lost power or was held off)".into())
            }
        }
        ("EventLog", 6008) => ("unexpected-shutdown", "critical", "The previous shutdown was unexpected".into()),
        ("Microsoft-Windows-WER-SystemErrorReporting", 1001) => {
            let code = first_hex(&di(0)).unwrap_or(0);
            let n = bugcheck_name(code);
            ("bugcheck", "critical", format!("Blue screen 0x{code:X}{}", if n.is_empty() { String::new() } else { format!(" {n}") }))
        }
        ("Display", 4101) => ("gpu-reset", "high", "Display driver stopped responding and recovered".into()),
        ("nvlddmkm", 153) => ("gpu-reset", "high", "NVIDIA driver reset the GPU after it stopped responding".into()),
        ("nvlddmkm", 13) | ("nvlddmkm", 14) => ("gpu-error", "high", format!("NVIDIA graphics engine error (event {id})")),
        ("nvlddmkm", _) => ("gpu-error", "medium", format!("NVIDIA driver event {id}")),
        (p, _) if p.starts_with("amdkmd") => ("gpu-error", "high", format!("AMD graphics driver event {id}")),
        (p, _) if p.starts_with("igfx") => ("gpu-error", "high", format!("Intel graphics driver event {id}")),
        ("Microsoft-Windows-DxgKrnl", _) => ("gpu-error", "medium", format!("DirectX graphics kernel error {id}")),
        ("Microsoft-Windows-WHEA-Logger", i) => {
            let s = if i == 1 || i == 18 { "critical" } else { "medium" };
            ("hardware", s, format!("Hardware error reported by the processor or a device (WHEA {i})"))
        }
        ("Microsoft-Windows-Resource-Exhaustion-Detector", 2004) => {
            let top = all.split(['(', ',']).next().unwrap_or("").trim().to_string();
            ("low-memory", "medium", if top.is_empty() { "Windows ran low on memory".into() } else { format!("Windows ran low on memory · biggest user: {top}") })
        }
        ("disk", _) | ("Microsoft-Windows-Ntfs", _) | ("Ntfs", _) => ("storage", "medium", format!("Disk or file-system error ({provider} {id})")),
        ("Microsoft-Windows-Kernel-Processor-Power", 37) => ("thermal", "low", "Processor speed was limited by firmware (often heat or power)".into()),
        ("Microsoft-Windows-UserPnp", _) => {
            let l = all.to_lowercase();
            if !(l.contains("ven_10de") || l.contains("ven_1002") || l.contains("nvlddmkm") || l.contains("amdkmdag") || l.contains("igfx") || l.contains("display")) { return None; }
            ("driver-update", "info", "Graphics driver installed or updated".into())
        }
        ("Microsoft-Windows-WindowsUpdateClient", 19) => {
            let t = dv("updateTitle");
            let l = t.to_lowercase();
            // daily antivirus definition and store app updates would drown the timeline
            let store_pkg = t.len() > 13 && t.as_bytes()[12] == b'-' && t[..12].chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
            if store_pkg || l.contains("security intelligence") || l.contains("definition update") || l.contains("antimalware") { return None; }
            ("windows-update", "info", if t.is_empty() { "Windows update installed".into() } else { t.chars().take(110).collect() })
        }
        ("Application Error", 1000) => {
            let app = di(0);
            let module = di(3);
            let m = module.to_lowercase();
            let gpu = GPU_MODULES.iter().any(|g| m.contains(g)) || app.to_lowercase().starts_with("python");
            ("app-crash", if gpu { "high" } else { "medium" }, format!("{app} crashed in {module}"))
        }
        ("Application Hang", 1002) => ("app-hang", "low", format!("{} stopped responding", di(0))),
        ("Windows Error Reporting", 1001) => {
            let ev = di(2);
            if ev != "LiveKernelEvent" { return None; }
            let p1 = di(5).to_lowercase();
            let gpu = ["117", "141", "1a1", "1a8", "1b8", "193"].contains(&p1.trim_start_matches("0x"));
            ("gpu-reset", if gpu { "high" } else { "medium" }, format!("Live kernel event {} ({})", p1, if gpu { "graphics hang captured" } else { "driver problem captured" }))
        }
        _ => return None,
    };
    Some(json!({
        "id": format!("{channel}#{rid}"), "time": time, "source": provider, "eventId": id,
        "category": cat, "severity": sev, "summary": summary,
        "detail": all.chars().take(600).collect::<String>(),
    }))
}

pub fn history(days: u32) -> Vec<Value> {
    let ms = Some(days as u64 * 86_400_000);
    let mut ev: Vec<Value> = vec![];
    for (ch, sels) in [("System", SYSTEM_SELECTS), ("Application", APP_SELECTS)] {
        for x in read_events(&query_xml(ch, sels, ms), 20_000) {
            if let Some(v) = classify(&x) { ev.push(v); }
        }
    }
    // merge recorder log (keeps events older than the rolling System log)
    let mut ids: std::collections::HashSet<String> = ev.iter().filter_map(|e| e["id"].as_str().map(String::from)).collect();
    if let Ok(t) = fs::read_to_string(log_path()) {
        for line in t.lines() {
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                if let Some(id) = v["id"].as_str() {
                    if ids.insert(id.to_string()) { ev.push(v); }
                }
            }
        }
    }
    ev.sort_by(|a, b| b["time"].as_str().cmp(&a["time"].as_str()));
    // Windows Error Reporting re-submits the same live-kernel report several times; keep one per 10 minutes
    let mut last: HashMap<String, i64> = HashMap::new();
    ev.retain(|e| {
        if e["source"].as_str() != Some("Windows Error Reporting") { return true; }
        let key = e["summary"].as_str().unwrap_or("").to_string();
        let t = parse_secs(e["time"].as_str().unwrap_or(""));
        match last.get(&key) {
            Some(&prev) if (prev - t).abs() < 600 => false,
            _ => { last.insert(key, t); true }
        }
    });
    ev
}

/// Seconds from an ISO-8601 UTC timestamp (enough precision for de-duplication).
fn parse_secs(s: &str) -> i64 {
    let n = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
    let days = crate::apps::days_from_civil(n(0, 4), n(5, 7), n(8, 10));
    days * 86_400 + n(11, 13) * 3600 + n(14, 16) * 60 + n(17, 19)
}

// ---------------- recorder ----------------
fn stab_dir() -> PathBuf { let d = data_dir().join("stability"); let _ = fs::create_dir_all(&d); d }
pub fn log_path() -> PathBuf { stab_dir().join("events.jsonl") }
fn state_path() -> PathBuf { stab_dir().join("state.json") }

fn rid_of(v: &Value) -> (String, u64) {
    let id = v["id"].as_str().unwrap_or("");
    let (ch, r) = id.split_once('#').unwrap_or(("", "0"));
    (ch.to_string(), r.parse().unwrap_or(0))
}

/// Entry point for `soundings.exe --record-events`. Never shows a window.
pub fn record() {
    // single instance: a lock file held exclusively
    let lock = stab_dir().join("recorder.lock");
    let Ok(_guard) = fs::OpenOptions::new().write(true).create(true).truncate(true).open(&lock) else { return };
    let mut state: HashMap<String, u64> = fs::read_to_string(state_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let first_run = state.is_empty();
    let mut fresh: Vec<Value> = vec![];
    for (ch, sels) in [("System", SYSTEM_SELECTS), ("Application", APP_SELECTS)] {
        let last = *state.get(ch).unwrap_or(&0);
        let mut max = last;
        // 3 days covers boot-time events picked up by the logon trigger
        for x in read_events(&query_xml(ch, sels, Some(3 * 86_400_000)), 5000) {
            if let Some(v) = classify(&x) {
                let (_, r) = rid_of(&v);
                if r > last {
                    max = max.max(r);
                    if !first_run { fresh.push(v); }
                }
            }
        }
        state.insert(ch.to_string(), max);
    }
    if !fresh.is_empty() {
        fresh.sort_by(|a, b| a["time"].as_str().cmp(&b["time"].as_str()));
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(log_path()) {
            for v in &fresh {
                let mut v = v.clone();
                v["loggedAt"] = json!(crate::util::now_unix());
                let _ = writeln!(f, "{}", v);
            }
            let _ = f.sync_all();
        }
        let worst = fresh.iter().filter(|v| matches!(v["severity"].as_str(), Some("critical") | Some("high"))).last();
        if let Some(v) = worst {
            let n = fresh.iter().filter(|v| matches!(v["severity"].as_str(), Some("critical") | Some("high"))).count();
            let title = if n > 1 { format!("{n} new crash or GPU events") } else { "Crash or GPU event recorded".into() };
            toast(&title, v["summary"].as_str().unwrap_or(""));
        }
    }
    let tmp = state_path().with_extension("tmp");
    if fs::write(&tmp, serde_json::to_string(&state).unwrap_or_default()).is_ok() {
        let _ = fs::rename(&tmp, state_path());
    }
}

fn toast(title: &str, body: &str) {
    let q = |s: &str| s.replace('\'', "''").replace(['<', '>', '&'], " ");
    let script = format!(
        "[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime]>$null;\
         $x=[Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02);\
         $t=$x.GetElementsByTagName('text');$t.Item(0).AppendChild($x.CreateTextNode('{}'))>$null;$t.Item(1).AppendChild($x.CreateTextNode('{}'))>$null;\
         [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}}\\WindowsPowerShell\\v1.0\\powershell.exe').Show([Windows.UI.Notifications.ToastNotification]::new($x))",
        q(&format!("Soundings · {title}")), q(body));
    let _ = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

const TASK_NAME: &str = "Soundings Crash Recorder";

pub fn monitor_status() -> Value {
    let q = run_hidden(&PathBuf::from("schtasks.exe"), &["/Query", "/TN", TASK_NAME, "/FO", "LIST"], Duration::from_secs(8)).unwrap_or_default();
    let on = q.contains(TASK_NAME);
    let lines = fs::read_to_string(log_path()).map(|t| t.lines().count()).unwrap_or(0);
    json!({"enabled": on, "logPath": log_path().to_string_lossy(), "recorded": lines})
}

fn xml_escape(s: &str) -> String { esc(s).replace('"', "&quot;") }

pub fn monitor_enable(on: bool) -> Result<Value, String> {
    let schtasks = PathBuf::from("schtasks.exe");
    if !on {
        run_hidden(&schtasks, &["/Delete", "/TN", TASK_NAME, "/F"], Duration::from_secs(10));
        return Ok(monitor_status());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let sub = |ch: &str, sels: &[&str]| xml_escape(&query_xml(ch, sels, None));
    let user = std::env::var("USERDOMAIN").map(|d| format!("{d}\\{}", std::env::var("USERNAME").unwrap_or_default())).unwrap_or_default();
    let xml = format!(r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Records crashes and GPU errors for Soundings. Runs only when such an event is logged, and at sign-in.</Description></RegistrationInfo>
  <Triggers>
    <EventTrigger><Enabled>true</Enabled><Subscription>{sys}</Subscription></EventTrigger>
    <EventTrigger><Enabled>true</Enabled><Subscription>{app}</Subscription></EventTrigger>
    <LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId><Delay>PT30S</Delay></LogonTrigger>
  </Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT1M</ExecutionTimeLimit>
    <Priority>7</Priority>
    <Hidden>true</Hidden>
    <StartWhenAvailable>true</StartWhenAvailable>
  </Settings>
  <Actions Context="Author"><Exec><Command>{exe}</Command><Arguments>--record-events</Arguments></Exec></Actions>
</Task>"#,
        sys = sub("System", SYSTEM_SELECTS), app = sub("Application", APP_SELECTS), user = xml_escape(&user), exe = xml_escape(&exe.to_string_lossy()));
    let path = stab_dir().join("task.xml");
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() { bytes.extend_from_slice(&u.to_le_bytes()); }
    fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let out = run_hidden(&schtasks, &["/Create", "/TN", TASK_NAME, "/XML", &path.to_string_lossy(), "/F"], Duration::from_secs(15)).unwrap_or_default();
    // seed the bookmark so the first notification is for a *new* event
    record();
    let st = monitor_status();
    if !st["enabled"].as_bool().unwrap_or(false) {
        return Err(format!("Windows did not create the task: {out}"));
    }
    Ok(st)
}
