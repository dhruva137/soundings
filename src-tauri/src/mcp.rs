//! `soundings.exe --mcp`: a read-only Model Context Protocol server over stdio, so Claude (Desktop or
//! Code) can answer questions about this PC. It only reads; there is no tool that changes, moves or
//! deletes anything. It stays off until the user turns on "Connect Claude" in Soundings' Settings,
//! and it uses the map Soundings already saved instead of re-scanning the disk.

use crate::scan::{FileRec, ScanResult};
use crate::{apps, dev, disk_cached, downloads, games, stability, system, util};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const OFF: &str = "Claude access is turned off. Open Soundings > Settings > Connect Claude and turn it on.";
const NO_SCAN: &str = "Soundings has no saved map yet. Open Soundings > Storage and press Map, then ask again.";

fn allowed() -> bool { crate::load_settings()["claude"].as_bool().unwrap_or(false) }

fn tools() -> Value {
    let t = |name: &str, desc: &str, props: Value| json!({
        "name": name, "description": desc,
        "inputSchema": {"type": "object", "properties": props, "additionalProperties": false},
        "annotations": {"readOnlyHint": true, "destructiveHint": false, "openWorldHint": false}
    });
    let int = |d: &str| json!({"type": "integer", "description": d});
    json!([
        t("pc_overview", "The PC's hardware (CPU, RAM, GPU, Windows version) and each drive's size and free space.", json!({})),
        t("storage_map", "What takes up space inside a folder, from Soundings' saved disk map: sub-folders and big files with sizes. Omit `folder` for the top of the mapped drive.",
          json!({"folder": {"type": "string", "description": "Full path, for example C:\\Users"}, "depth": int("Levels to include, 1-3 (default 1)")})),
        t("large_files", "The biggest files in the user's folder (outside AppData and project build folders).", json!({"min_mb": int("Minimum size in MB (default 250)"), "limit": int("Max results (default 40)")})),
        t("old_files", "Files over 10 MB in the user's folder that have not been changed or opened in a long time.", json!({"days": int("Unused for at least this many days (default 365)"), "limit": int("Max results (default 40)")})),
        t("duplicate_files", "Groups of identical files (same content) in the user's folder, biggest waste first. Needs the Duplicates tab to have been opened once in Soundings.", json!({"limit": int("Max groups (default 25)")})),
        t("project_build_folders", "node_modules, Python venvs, Rust/Java target and similar rebuildable folders inside projects, with sizes.", json!({"limit": int("Max results (default 40)")})),
        t("installed_apps", "Installed programs with publisher, version, size, install date and origin (Windows / Manufacturer / Came with PC / You installed).",
          json!({"search": {"type": "string", "description": "Filter by text in name or publisher"}, "origin": {"type": "string", "description": "Windows, Manufacturer, Came with PC or You installed"}})),
        t("app_usage", "Which apps the user actually uses: launches and time in focus, from Windows' own records.", json!({"limit": int("Max results (default 25)")})),
        t("startup_items", "Programs that start with Windows and whether each is enabled.", json!({})),
        t("developer_tools", "Every copy of Python, Node, Java, Git, .NET and other developer tools, versions, and which copy runs first on PATH.", json!({})),
        t("developer_caches", "Sizes of package caches, model downloads and temp folders (pip, npm, Hugging Face, Ollama, Temp and more), with each tool's own clean-up command.", json!({})),
        t("games", "Installed games from Steam, Epic, GOG, Xbox, Ubisoft, Rockstar and others, with sizes.", json!({})),
        t("crash_history", "Blue screens, unexpected shutdowns, GPU driver resets and app crashes from the Windows event logs.", json!({"days": int("How far back (default 90)")})),
        t("downloads_summary", "Downloads folder activity: per month, by kind, and which sites files came from.", json!({})),
    ])
}

fn arg_u(a: &Value, k: &str, d: u64) -> u64 { a[k].as_u64().unwrap_or(d) }

fn saved() -> Result<ScanResult, String> { ScanResult::load_saved().ok_or_else(|| NO_SCAN.to_string()) }

fn file_list(files: Vec<&FileRec>) -> Value {
    json!(files.iter().map(|f| json!({"path": f.path, "size": util_size(f.size), "bytes": f.size,
        "modified": day(f.modified), "inGitRepo": f.repo})).collect::<Vec<_>>())
}

fn util_size(b: u64) -> String {
    let u = ["B", "KB", "MB", "GB", "TB"];
    let (mut v, mut i) = (b as f64, 0);
    while v >= 1024.0 && i < 4 { v /= 1024.0; i += 1; }
    format!("{v:.1} {}", u[i])
}
fn day(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{m:02}-{d:02}", yoe + era * 400 + if m <= 2 { 1 } else { 0 })
}

fn run(name: &str, a: &Value) -> Result<Value, String> {
    Ok(match name {
        "pc_overview" => json!({"machine": system::machine(), "drives": system::drives()}),
        "storage_map" => {
            let s = saved()?;
            let id = match a["folder"].as_str() { Some(f) if !f.is_empty() => s.find(f).ok_or("That folder is not in the saved map (it may be skipped, or outside the mapped drive).")?, _ => 0 };
            let mut v = s.view(id, arg_u(a, "depth", 1).clamp(1, 3) as u32);
            v["mappedOn"] = json!(day(s.taken));
            v
        }
        "large_files" => {
            let s = saved()?;
            let min = arg_u(a, "min_mb", 250) << 20;
            let mut l: Vec<&FileRec> = s.user_files.iter().filter(|f| f.size >= min).collect();
            l.sort_by(|x, y| y.size.cmp(&x.size));
            l.truncate(arg_u(a, "limit", 40).min(200) as usize);
            json!({"mappedOn": day(s.taken), "files": file_list(l)})
        }
        "old_files" => {
            let s = saved()?;
            let cutoff = util::now_unix() - arg_u(a, "days", 365) as i64 * 86_400;
            let mut l: Vec<&FileRec> = s.user_files.iter().filter(|f| f.size >= 10 << 20 && f.modified.max(f.accessed) < cutoff).collect();
            l.sort_by(|x, y| y.size.cmp(&x.size));
            l.truncate(arg_u(a, "limit", 40).min(200) as usize);
            json!({"mappedOn": day(s.taken), "note": "Windows does not always record when a file was last opened.", "files": file_list(l)})
        }
        "duplicate_files" => {
            let s = saved()?;
            let mut d = crate::saved_dups(s.taken).ok_or("No duplicate check saved for the current map. Open Soundings > Storage > Duplicates once, then ask again.")?;
            if let Some(g) = d["groups"].as_array_mut() { g.truncate(arg_u(a, "limit", 25).min(200) as usize); }
            d
        }
        "project_build_folders" => {
            let s = saved()?;
            let mut v = s.artifacts.clone();
            v.sort_by(|x, y| y.size.cmp(&x.size));
            v.truncate(arg_u(a, "limit", 40).min(300) as usize);
            json!(v)
        }
        "installed_apps" => {
            let all = disk_cached("apps", 6 * 3600, false, apps::apps);
            let q = a["search"].as_str().unwrap_or("").to_lowercase();
            let o = a["origin"].as_str().unwrap_or("").to_lowercase();
            json!(all["apps"].as_array().cloned().unwrap_or_default().into_iter().filter(|x| {
                (q.is_empty() || format!("{} {}", x["name"].as_str().unwrap_or(""), x["publisher"].as_str().unwrap_or("")).to_lowercase().contains(&q))
                    && (o.is_empty() || x["origin"].as_str().unwrap_or("").to_lowercase() == o)
            }).collect::<Vec<_>>())
        }
        "app_usage" => {
            let mut v = apps::usage();
            if let Some(l) = v.as_array_mut() { l.truncate(arg_u(a, "limit", 25) as usize); }
            v
        }
        "startup_items" => apps::startup(),
        "developer_tools" => disk_cached("runtimes", 24 * 3600, false, dev::runtimes),
        "developer_caches" => disk_cached("caches", 12 * 3600, false, dev::caches),
        "games" => disk_cached("games", 24 * 3600, false, games::games),
        "crash_history" => json!(stability::history(arg_u(a, "days", 90).min(3650) as u32)),
        "downloads_summary" => downloads::insights(),
        _ => return Err(format!("Unknown tool {name}")),
    })
}

fn call(p: &Value) -> Value {
    let name = p["name"].as_str().unwrap_or("");
    let args = p.get("arguments").cloned().unwrap_or(json!({}));
    // same small 3-thread pool as the app, so a question from Claude never hogs the PC
    let r = if !allowed() { Err(OFF.to_string()) } else { crate::light_pool().install(|| run(name, &args)) };
    let (text, err) = match r {
        Ok(v) => {
            let mut s = v.to_string();
            if s.len() > 120_000 { s.truncate(120_000); s.push_str(" …(truncated; ask for fewer results)"); }
            (s, false)
        }
        Err(e) => (e, true),
    };
    json!({"content": [{"type": "text", "text": text}], "isError": err})
}

pub fn serve() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() { continue; }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            let _ = writeln!(out, "{}", json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "Parse error"}}));
            let _ = out.flush();
            continue;
        };
        let Some(id) = msg.get("id").cloned() else { continue }; // notifications need no reply
        let res = match msg["method"].as_str().unwrap_or("") {
            "initialize" => Ok(json!({
                "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "soundings", "title": "Soundings", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Read-only view of this Windows PC from the Soundings app: storage map, large/old/duplicate files, apps, app usage, developer tools and caches, games, crash history and downloads. Nothing here can change or delete files; if the user wants to remove something, tell them which file and let them do it in File Explorer."
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": tools()})),
            "tools/call" => Ok(call(&msg["params"])),
            _ => Err(json!({"code": -32601, "message": "Method not found"})),
        };
        let reply = match res {
            Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
            Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
        };
        let _ = writeln!(out, "{reply}");
        let _ = out.flush();
    }
}
