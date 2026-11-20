use std::fs;
use std::os::windows::fs::MetadataExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn env_path(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(fallback))
}
pub fn home() -> PathBuf { env_path("USERPROFILE", "C:\\Users\\Default") }
pub fn local_appdata() -> PathBuf { env_path("LOCALAPPDATA", "C:\\") }
pub fn roaming_appdata() -> PathBuf { env_path("APPDATA", "C:\\") }

/// Where Soundings keeps its own small files (settings, crash log).
pub fn data_dir() -> PathBuf {
    let d = local_appdata().join("Soundings");
    let _ = fs::create_dir_all(&d);
    d
}

/// Windows FILETIME (100ns since 1601) -> unix seconds.
pub fn ft_to_unix(ft: u64) -> i64 {
    if ft == 0 { return 0; }
    (ft as i64 - 116_444_736_000_000_000) / 10_000_000
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Total size and file count of a folder. Never follows junctions/symlinks, never opens files.
pub fn dir_size(p: &Path) -> (u64, u64) {
    dir_size_skip(p, &[])
}

pub fn dir_size_skip(p: &Path, skip_names: &[&str]) -> (u64, u64) {
    let mut size = 0u64;
    let mut files = 0u64;
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() { continue; }
            if ft.is_dir() {
                if d == p {
                    let n = e.file_name().to_string_lossy().to_lowercase();
                    if skip_names.iter().any(|s| *s == n) { continue; }
                }
                stack.push(e.path());
            } else if let Ok(md) = e.metadata() {
                size += md.file_size();
                files += 1;
            }
        }
    }
    (size, files)
}

pub fn modified_unix(p: &Path) -> i64 {
    fs::metadata(p).map(|m| ft_to_unix(m.last_write_time())).unwrap_or(0)
}

/// Run a program with no console window and a hard timeout. Returns stdout+stderr.
pub fn run_hidden(exe: &Path, args: &[&str], timeout: Duration) -> Option<String> {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(30)),
            Err(_) => return None,
        }
    }
    let out = child.wait_with_output().ok()?;
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    Some(s.trim().to_string())
}

/// Expand a path pattern where any component may contain one '*' (e.g. C:\Program Files\Java\*\bin\java.exe).
pub fn glob(pattern: &str) -> Vec<PathBuf> {
    let parts: Vec<&str> = pattern.split('\\').collect();
    let mut acc = vec![PathBuf::from(format!("{}\\", parts[0]))];
    for part in &parts[1..] {
        if part.is_empty() { continue; }
        let mut next = vec![];
        for base in &acc {
            if let Some(star) = part.find('*') {
                let (pre, suf) = (part[..star].to_lowercase(), part[star + 1..].to_lowercase());
                if let Ok(rd) = fs::read_dir(base) {
                    for e in rd.flatten() {
                        let n = e.file_name().to_string_lossy().to_lowercase();
                        if n.starts_with(&pre) && n.ends_with(&suf) && n.len() >= pre.len() + suf.len() {
                            next.push(e.path());
                        }
                    }
                }
            } else {
                let p = base.join(part);
                if p.exists() { next.push(p); }
            }
        }
        acc = next;
        if acc.is_empty() { break; }
    }
    acc
}

pub fn read_text_lossy(p: &Path) -> Option<String> {
    fs::read(p).ok().map(|b| String::from_utf8_lossy(&b).to_string())
}
