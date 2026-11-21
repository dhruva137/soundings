//! Downloads insights: what arrived when, what kind, and from where (Zone.Identifier stream).

use crate::util::{ft_to_unix, home};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::PathBuf;

pub fn family(ext: &str) -> &'static str {
    match ext {
        "pdf" => "PDFs",
        "doc" | "docx" | "rtf" | "odt" | "txt" | "md" | "tex" | "epub" => "Documents",
        "ppt" | "pptx" | "key" | "odp" => "Slides",
        "xls" | "xlsx" | "csv" | "tsv" | "json" | "parquet" | "jsonl" | "xml" | "ods" => "Data",
        "ipynb" | "py" | "js" | "ts" | "c" | "cpp" | "java" | "rs" | "go" | "sh" | "pine" | "html" => "Code",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "heic" | "bmp" => "Images",
        "mp4" | "mov" | "mkv" | "webm" | "avi" => "Video",
        "mp3" | "wav" | "m4a" | "flac" | "ogg" => "Audio",
        "zip" | "rar" | "7z" | "gz" | "tar" | "tgz" | "xz" => "Archives",
        "exe" | "msi" | "iso" | "img" | "apk" | "msix" | "appx" => "Installers",
        _ => "Other",
    }
}

fn host_of(url: &str) -> Option<String> {
    let s = url.split("://").nth(1)?;
    let h = s.split(['/', '?', ':']).next()?.to_lowercase();
    let h = h.trim_start_matches("www.").to_string();
    if h.is_empty() || h == "about" { None } else { Some(h) }
}

/// Source site of a download, from the tiny Zone.Identifier stream browsers attach to it.
fn read_host(p: &str) -> Option<String> {
    let z = fs::read_to_string(format!("{p}:Zone.Identifier")).ok()?;
    let url = z.lines().find_map(|l| l.strip_prefix("HostUrl=")).or_else(|| z.lines().find_map(|l| l.strip_prefix("ReferrerUrl=")));
    url.and_then(host_of)
}

pub fn insights() -> Value {
    use rayon::prelude::*;
    let root = home().join("Downloads");
    let mut months: HashMap<String, (u64, u64)> = HashMap::new();
    let mut fams: HashMap<&str, (u64, u64)> = HashMap::new();
    let mut hosts: HashMap<String, (u64, u64)> = HashMap::new();
    let mut total = (0u64, 0u64);
    let mut recent: Vec<(i64, String, u64)> = vec![];
    // 1. list files (cheap)
    let mut files: Vec<(String, u64, i64, u64)> = vec![]; // path, size, created, write-time
    let mut stack = vec![root.clone()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() { continue; }
            if ft.is_dir() { stack.push(e.path()); continue; }
            if e.file_name().to_string_lossy().eq_ignore_ascii_case("desktop.ini") { continue; }
            let Ok(md) = e.metadata() else { continue };
            files.push((e.path().to_string_lossy().into_owned(), md.file_size(), ft_to_unix(md.creation_time()), md.last_write_time()));
        }
    }
    // 2. source sites: remembered per file, so only new or changed files are opened
    let cache_p = crate::util::data_dir().join("cache").join("download-sources.json");
    let old: HashMap<String, (u64, Option<String>)> = fs::read_to_string(&cache_p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let srcs: Vec<Option<String>> = files.par_iter().map(|(p, _, _, w)| match old.get(p) {
        Some((ow, h)) if ow == w => h.clone(),
        _ => read_host(p),
    }).collect();
    let new: HashMap<&String, (u64, &Option<String>)> = files.iter().zip(&srcs).map(|((p, _, _, w), h)| (p, (*w, h))).collect();
    let _ = fs::create_dir_all(cache_p.parent().unwrap());
    let _ = fs::write(&cache_p, serde_json::to_string(&new).unwrap_or_default());
    // 3. tally
    for ((p, size, created, _), host) in files.into_iter().zip(srcs) {
        let ext = PathBuf::from(&p).extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        let a = months.entry(month_of(created)).or_default(); a.0 += 1; a.1 += size;
        let f = fams.entry(family(&ext)).or_default(); f.0 += 1; f.1 += size;
        total.0 += 1; total.1 += size;
        if let Some(h) = host { let x = hosts.entry(h).or_default(); x.0 += 1; x.1 += size; }
        recent.push((created, p, size));
    }
    let mut months: Vec<Value> = months.into_iter().map(|(k, (n, b))| json!({"month": k, "count": n, "bytes": b})).collect();
    months.sort_by(|a, b| a["month"].as_str().cmp(&b["month"].as_str()));
    let mut fams: Vec<Value> = fams.into_iter().map(|(k, (n, b))| json!({"family": k, "count": n, "bytes": b})).collect();
    fams.sort_by(|a, b| b["bytes"].as_u64().cmp(&a["bytes"].as_u64()));
    let mut hosts: Vec<Value> = hosts.into_iter().map(|(k, (n, b))| json!({"host": k, "count": n, "bytes": b})).collect();
    hosts.sort_by(|a, b| b["count"].as_u64().cmp(&a["count"].as_u64()));
    hosts.truncate(15);
    recent.sort_by(|a, b| b.0.cmp(&a.0));
    recent.truncate(12);
    json!({
        "root": root.to_string_lossy(), "count": total.0, "bytes": total.1,
        "months": months, "families": fams, "hosts": hosts,
        "recent": recent.into_iter().map(|(t, p, s)| json!({"time": t, "path": p, "size": s})).collect::<Vec<_>>(),
    })
}

fn month_of(t: i64) -> String {
    // civil-from-days
    let z = t.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}", if m <= 2 { y + 1 } else { y }, m)
}
