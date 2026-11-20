//! Opt-in experiment telemetry: one long-lived `nvidia-smi -lms 5000` child (cheap; never polled on a timer
//! when off, because querying the NVIDIA GPU wakes it from deep sleep). Each sample also records system
//! commit charge, the suspect for "insufficient resources" GPU resets. Lines are flushed to CSV so the
//! minutes before a crash survive it.

use crate::util::{data_dir, CREATE_NO_WINDOW};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct Telemetry {
    child: Mutex<Option<Child>>,
    samples: Arc<Mutex<VecDeque<Value>>>,
    file: Mutex<Option<PathBuf>>,
}

fn nvidia_smi() -> Option<PathBuf> {
    let sys = PathBuf::from(r"C:\Windows\System32\nvidia-smi.exe");
    if sys.exists() { return Some(sys); }
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join("nvidia-smi.exe")).find(|p| p.exists()))
}

fn commit() -> (f64, u64) {
    use windows_sys::Win32::System::SystemInformation::*;
    let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    unsafe { GlobalMemoryStatusEx(&mut m) };
    let used = m.ullTotalPageFile.saturating_sub(m.ullAvailPageFile);
    let pct = if m.ullTotalPageFile > 0 { used as f64 * 100.0 / m.ullTotalPageFile as f64 } else { 0.0 };
    (pct, m.ullAvailPhys / (1 << 20))
}

impl Telemetry {
    pub fn start(&self) -> Result<Value, String> {
        let mut guard = self.child.lock().unwrap();
        if guard.is_some() { return Ok(self.status()); }
        let exe = nvidia_smi().ok_or("nvidia-smi was not found. It ships with the NVIDIA driver.")?;
        let fields = "timestamp,temperature.gpu,power.draw,utilization.gpu,memory.used,memory.total,clocks.sm,pstate";
        let mut child = Command::new(exe)
            .args([&format!("--query-gpu={fields}"), "--format=csv,noheader,nounits", "-lms", "5000"])
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn().map_err(|e| e.to_string())?;
        let dir = data_dir().join("telemetry");
        let _ = fs::create_dir_all(&dir);
        let stamp = chrono_like_stamp();
        let path = dir.join(format!("experiment-{stamp}.csv"));
        let mut f = fs::File::create(&path).map_err(|e| e.to_string())?;
        let _ = writeln!(f, "time,gpu_temp_c,gpu_power_w,gpu_util_pct,gpu_mem_used_mb,gpu_mem_total_mb,sm_clock_mhz,pstate,commit_pct,ram_free_mb");
        let out = child.stdout.take().ok_or("no output")?;
        let samples = self.samples.clone();
        samples.lock().unwrap().clear();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let c: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                if c.len() < 8 { continue; }
                let (cp, free) = commit();
                let _ = writeln!(f, "{},{:.1},{}", c.join(","), cp, free);
                let _ = f.flush();
                let num = |i: usize| c[i].parse::<f64>().ok();
                let v = json!({"t": c[0], "temp": num(1), "power": num(2), "util": num(3), "memUsed": num(4), "memTotal": num(5),
                               "clock": num(6), "pstate": c[7], "commit": (cp * 10.0).round() / 10.0, "ramFree": free});
                let mut s = samples.lock().unwrap();
                s.push_back(v);
                while s.len() > 720 { s.pop_front(); } // last hour in memory
            }
        });
        *guard = Some(child);
        *self.file.lock().unwrap() = Some(path);
        drop(guard);
        Ok(self.status())
    }

    pub fn stop(&self) -> Value {
        if let Some(mut c) = self.child.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.status()
    }

    pub fn status(&self) -> Value {
        let running = self.child.lock().unwrap().as_mut().map(|c| matches!(c.try_wait(), Ok(None))).unwrap_or(false);
        let s = self.samples.lock().unwrap();
        let tail: Vec<Value> = s.iter().rev().take(180).rev().cloned().collect();
        json!({"running": running, "available": nvidia_smi().is_some(), "file": self.file.lock().unwrap().as_ref().map(|p| p.to_string_lossy().to_string()), "samples": tail})
    }
}

fn chrono_like_stamp() -> String {
    let t = crate::util::now_unix();
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    // civil date from days (UTC)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}Z", secs / 3600, (secs / 60) % 60, secs % 60)
}
