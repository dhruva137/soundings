#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod apps;
mod dev;
mod downloads;
mod games;
mod mcp;
mod scan;
mod stability;
mod system;
mod telemetry;
mod util;

use scan::{FileRec, Progress, ScanResult};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, State};

#[derive(Default)]
struct Inner {
    scan: Mutex<Option<Arc<ScanResult>>>,
    scanning: AtomicBool,
    dups: Mutex<Option<Value>>,
    dup_progress: AtomicU64,
    tried_saved: AtomicBool,
    telemetry: telemetry::Telemetry,
}
#[derive(Default, Clone)]
struct AppState(Arc<Inner>);

type R = Result<Value, String>;

/// Background reads run on a small pool so the PC (and battery) barely notice them.
fn light_pool() -> &'static rayon::ThreadPool {
    static POOL: std::sync::OnceLock<rayon::ThreadPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| rayon::ThreadPoolBuilder::new().num_threads(3).build().unwrap())
}

async fn blocking<F: FnOnce() -> Value + Send + 'static>(f: F) -> R {
    tauri::async_runtime::spawn_blocking(move || light_pool().install(f)).await.map_err(|e| e.to_string())
}

/// Results of slow, rarely-changing reads are kept on disk for `max_age` seconds.
fn disk_cached(name: &str, max_age: u64, fresh: bool, f: impl FnOnce() -> Value) -> Value {
    let dir = util::data_dir().join("cache");
    let _ = std::fs::create_dir_all(&dir);
    let p = dir.join(format!("{name}.json"));
    if !fresh {
        if let Ok(m) = std::fs::metadata(&p) {
            let age = m.modified().ok().and_then(|t| t.elapsed().ok()).map(|d| d.as_secs()).unwrap_or(u64::MAX);
            if age < max_age {
                if let Some(v) = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok()) { return v; }
            }
        }
    }
    let v = f();
    let _ = std::fs::write(&p, v.to_string());
    v
}

// ---------------- settings ----------------
fn settings_path() -> std::path::PathBuf { util::data_dir().join("settings.json") }
fn load_settings() -> Value {
    std::fs::read_to_string(settings_path()).ok().and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_else(|| json!({"excluded": [], "largeMb": 250, "oldDays": 365}))
}

#[tauri::command]
fn get_settings() -> Value { load_settings() }

#[tauri::command]
fn set_settings(settings: Value) -> R {
    std::fs::write(settings_path(), serde_json::to_string_pretty(&settings).unwrap()).map_err(|e| e.to_string())?;
    Ok(settings)
}

// ---------------- system ----------------
#[tauri::command]
async fn overview() -> R {
    blocking(|| json!({"machine": system::machine(), "drives": system::drives(), "theme": system::theme(), "home": util::home(), "dataDir": util::data_dir()})).await
}

// ---------------- storage ----------------
#[tauri::command]
fn start_scan(app: tauri::AppHandle, st: State<'_, AppState>, root: String) -> R {
    let st = st.inner().clone();
    if st.0.scanning.swap(true, Ordering::SeqCst) { return Err("A scan is already running".into()); }
    let excluded: Vec<String> = load_settings()["excluded"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
    std::thread::spawn(move || {
        let prog = Arc::new(Progress::new());
        let done = Arc::new(AtomicBool::new(false));
        {
            let (prog, done, app) = (prog.clone(), done.clone(), app.clone());
            std::thread::spawn(move || {
                while !done.load(Ordering::Relaxed) {
                    let _ = app.emit("scan-progress", json!({
                        "files": prog.files.load(Ordering::Relaxed), "dirs": prog.dirs.load(Ordering::Relaxed),
                        "bytes": prog.bytes.load(Ordering::Relaxed)}));
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
            });
        }
        // a dedicated, lower-priority pool so the PC stays responsive during the scan
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).saturating_sub(2).clamp(2, 12);
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        let res = pool.install(|| scan::scan(&root, &excluded, &prog));
        done.store(true, Ordering::Relaxed);
        let summary = json!({"root": res.root, "files": res.nodes[0].files, "size": res.nodes[0].size,
                             "dirs": res.nodes.len(), "denied": res.denied, "ms": res.elapsed_ms as u64});
        let res = Arc::new(res);
        *st.0.scan.lock().unwrap() = Some(res.clone());
        *st.0.dups.lock().unwrap() = None;
        st.0.scanning.store(false, Ordering::SeqCst);
        let _ = app.emit("scan-done", summary);
        // keep it for next launch (in the background, after the UI already has it)
        let _ = std::fs::remove_file(dups_path());
        res.save();
    });
    Ok(json!({"started": true}))
}

/// The last map: in memory, or loaded once from %LOCALAPPDATA%\Soundings\cache\scan.bin.
fn ensure_loaded(st: &AppState) -> Option<Arc<ScanResult>> {
    if let Some(s) = st.0.scan.lock().unwrap().clone() { return Some(s); }
    if st.0.scanning.load(Ordering::SeqCst) || st.0.tried_saved.swap(true, Ordering::SeqCst) { return None; }
    let s = Arc::new(ScanResult::load_saved()?);
    let mut slot = st.0.scan.lock().unwrap();
    if slot.is_none() { *slot = Some(s.clone()); }
    slot.clone()
}

#[tauri::command]
async fn scan_status(st: State<'_, AppState>) -> R {
    let st = st.inner().clone();
    let s = tauri::async_runtime::spawn_blocking({ let st = st.clone(); move || ensure_loaded(&st) }).await.map_err(|e| e.to_string())?;
    Ok(json!({"scanning": st.0.scanning.load(Ordering::SeqCst), "has": s.is_some(),
           "root": s.as_ref().map(|r| r.root.clone()), "size": s.as_ref().map(|r| r.nodes[0].size),
           "files": s.as_ref().map(|r| r.nodes[0].files), "ms": s.as_ref().map(|r| r.elapsed_ms as u64),
           "taken": s.as_ref().map(|r| r.taken)}))
}

fn current(st: &AppState) -> Result<Arc<ScanResult>, String> {
    ensure_loaded(st).ok_or_else(|| "No scan yet".to_string())
}

fn dups_path() -> std::path::PathBuf { util::data_dir().join("cache").join("dups.json") }

/// Saved duplicate groups for this exact scan, with files you've since removed left out.
fn saved_dups(taken: i64) -> Option<Value> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(dups_path()).ok()?).ok()?;
    (v["taken"].as_i64() == Some(taken)).then(|| prune_dups(v))
}

fn prune_dups(mut v: Value) -> Value {
    if let Some(gs) = v["groups"].as_array_mut() {
        for g in gs.iter_mut() {
            if let Some(fs) = g["files"].as_array_mut() {
                fs.retain(|f| f["path"].as_str().map(|p| std::path::Path::new(p).exists()).unwrap_or(false));
                let (n, size) = (fs.len() as u64, g["size"].as_u64().unwrap_or(0));
                g["wasted"] = json!(size * n.saturating_sub(1));
            }
        }
        gs.retain(|g| g["files"].as_array().map(|f| f.len() > 1).unwrap_or(false));
    }
    v
}

#[tauri::command]
fn tree(st: State<'_, AppState>, id: Option<u32>, depth: Option<u32>) -> R {
    let s = current(&st)?;
    let id = id.unwrap_or(0).min(s.nodes.len() as u32 - 1);
    Ok(s.view(id, depth.unwrap_or(5)))
}

#[tauri::command]
fn node_path(st: State<'_, AppState>, id: u32) -> R {
    let s = current(&st)?;
    Ok(json!(s.path_of(id)))
}

#[tauri::command]
fn large_old(st: State<'_, AppState>, min_mb: u64, old_days: u64) -> R {
    let s = current(&st)?;
    let now = util::now_unix();
    let mut large: Vec<&FileRec> = s.user_files.iter().filter(|f| f.size >= min_mb << 20).collect();
    large.sort_by(|a, b| b.size.cmp(&a.size));
    large.truncate(400);
    let cutoff = now - (old_days as i64) * 86_400;
    let mut old: Vec<&FileRec> = s.user_files.iter().filter(|f| f.size >= 10 << 20 && f.modified.max(f.accessed) < cutoff).collect();
    old.sort_by(|a, b| b.size.cmp(&a.size));
    old.truncate(400);
    Ok(json!({"large": large, "old": old}))
}

#[tauri::command]
async fn duplicates(app: tauri::AppHandle, st: State<'_, AppState>) -> R {
    let mem = st.0.dups.lock().unwrap().clone();
    if let Some(d) = mem {
        return blocking(move || prune_dups(d)).await;
    }
    let s = current(&st)?;
    let st2 = st.inner().clone();
    let taken = s.taken;
    if let Ok(Some(d)) = tauri::async_runtime::spawn_blocking(move || saved_dups(taken)).await {
        *st.0.dups.lock().unwrap() = Some(d.clone());
        return Ok(d);
    }
    blocking(move || {
        st2.0.dup_progress.store(0, Ordering::Relaxed);
        let done = Arc::new(AtomicBool::new(false));
        {
            let (done, st3, app) = (done.clone(), st2.clone(), app.clone());
            std::thread::spawn(move || while !done.load(Ordering::Relaxed) {
                let _ = app.emit("dup-progress", st3.0.dup_progress.load(Ordering::Relaxed));
                std::thread::sleep(std::time::Duration::from_millis(400));
            });
        }
        let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
        let groups = pool.install(|| scan::duplicates(&s.user_files, &st2.0.dup_progress));
        done.store(true, Ordering::Relaxed);
        let v = json!({"groups": groups, "taken": taken});
        let _ = std::fs::write(dups_path(), v.to_string());
        *st2.0.dups.lock().unwrap() = Some(v.clone());
        v
    }).await
}

#[tauri::command]
fn artifacts(st: State<'_, AppState>) -> R {
    let s = current(&st)?;
    let mut a = s.artifacts.clone();
    a.sort_by(|x, y| y.size.cmp(&x.size));
    Ok(json!(a))
}

#[tauri::command]
async fn downloads_insights() -> R { blocking(downloads::insights).await }

// ---------------- actions ----------------
// Soundings has no delete, recycle or move command at all. The only action on a file is showing it
// in File Explorer, where the user decides what to do with it.

/// Which of these paths no longer exist (so the shortlist can drop items the user removed).
#[tauri::command]
fn missing_paths(paths: Vec<String>) -> Value {
    json!(paths.into_iter().filter(|p| !std::path::Path::new(p).exists()).collect::<Vec<_>>())
}

#[tauri::command]
fn reveal(path: String) -> R {
    use std::os::windows::process::CommandExt;
    let p = std::path::PathBuf::from(&path);
    let mut c = std::process::Command::new("explorer.exe");
    if p.is_file() { c.raw_arg(format!("/select,\"{}\"", path)); } else { c.arg(&path); }
    c.spawn().map_err(|e| e.to_string())?;
    Ok(json!(true))
}

// ---------------- apps / dev / games / stability ----------------
#[tauri::command]
async fn apps_list(fresh: Option<bool>) -> R { blocking(move || disk_cached("apps", 6 * 3600, fresh.unwrap_or(false), apps::apps)).await }
#[tauri::command]
async fn startup_list() -> R { blocking(apps::startup).await }
#[tauri::command]
async fn app_usage() -> R { blocking(apps::usage).await }
#[tauri::command]
async fn dev_runtimes(fresh: Option<bool>) -> R { blocking(move || disk_cached("runtimes", 24 * 3600, fresh.unwrap_or(false), dev::runtimes)).await }
#[tauri::command]
async fn dev_caches(fresh: Option<bool>, cached_only: Option<bool>) -> R {
    if cached_only.unwrap_or(false) {
        let p = util::data_dir().join("cache").join("caches.json");
        return Ok(std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null));
    }
    blocking(move || disk_cached("caches", 12 * 3600, fresh.unwrap_or(false), dev::caches)).await
}
#[tauri::command]
async fn games_list(fresh: Option<bool>) -> R { blocking(move || disk_cached("games", 24 * 3600, fresh.unwrap_or(false), games::games)).await }
#[tauri::command]
async fn stability_history(days: u32) -> R { blocking(move || json!(stability::history(days))).await }
#[tauri::command]
async fn monitor_status() -> R { blocking(stability::monitor_status).await }
#[tauri::command]
fn telemetry_start(st: State<'_, AppState>) -> R { st.0.telemetry.start() }
#[tauri::command]
fn telemetry_stop(st: State<'_, AppState>) -> Value { st.0.telemetry.stop() }
#[tauri::command]
fn telemetry_status(st: State<'_, AppState>) -> Value { st.0.telemetry.status() }
#[tauri::command]
async fn monitor_enable(on: bool) -> R {
    tauri::async_runtime::spawn_blocking(move || stability::monitor_enable(on)).await.map_err(|e| e.to_string())?
}

// ---------------- Claude connection (read-only MCP server, off by default) ----------------
fn claude_desktop_configs() -> Vec<std::path::PathBuf> {
    let mut v = vec![];
    if let Some(a) = std::env::var_os("APPDATA") { v.push(std::path::PathBuf::from(a).join("Claude")); }
    // Microsoft Store / MSIX installs keep it in the package's virtualised AppData
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        for p in util::glob(&format!("{}\\Packages\\Claude_*\\LocalCache\\Roaming\\Claude", l.to_string_lossy())) { v.push(p); }
    }
    v.into_iter().filter(|d| d.is_dir()).map(|d| d.join("claude_desktop_config.json")).collect()
}

#[tauri::command]
fn claude_info() -> Value {
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    let cfgs = claude_desktop_configs();
    let connected = cfgs.iter().any(|p| std::fs::read_to_string(p).ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok()).map(|v| v["mcpServers"]["soundings"].is_object()).unwrap_or(false));
    json!({"enabled": load_settings()["claude"].as_bool().unwrap_or(false), "exe": exe,
           "desktopFound": !cfgs.is_empty(), "desktopConnected": connected})
}

/// Adds (or removes) Soundings in Claude Desktop's own config file, keeping a backup of it first.
#[tauri::command]
fn claude_desktop_connect(on: bool) -> R {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().to_string();
    let cfgs = claude_desktop_configs();
    if cfgs.is_empty() { return Err("Claude Desktop doesn't seem to be installed.".into()); }
    for p in cfgs {
        let text = std::fs::read_to_string(&p).unwrap_or_else(|_| "{}".into());
        let mut v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|_| format!("Couldn't read {} (it isn't valid JSON), so it was left alone.", p.display()))?;
        if p.exists() { let _ = std::fs::write(p.with_extension("json.before-soundings"), &text); }
        if !v.is_object() { v = json!({}); }
        if !v["mcpServers"].is_object() { v["mcpServers"] = json!({}); }
        let servers = v["mcpServers"].as_object_mut().unwrap();
        if on { servers.insert("soundings".into(), json!({"command": exe, "args": ["--mcp"]})); } else { servers.remove("soundings"); }
        std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap()).map_err(|e| e.to_string())?;
    }
    Ok(claude_info())
}

/// Developer aid: `soundings.exe --dump <out-dir> [scan-root]` writes read-only JSON snapshots
/// so the UI can be previewed in a browser (src/index.html?mock). Never modifies anything.
fn dump(out: &str, root: Option<String>) {
    let out = std::path::PathBuf::from(out);
    let _ = std::fs::create_dir_all(&out);
    let w = |name: &str, v: Value| { let _ = std::fs::write(out.join(format!("{name}.json")), v.to_string()); };
    w("overview", json!({"machine": system::machine(), "drives": system::drives(), "theme": system::theme(), "home": util::home(), "dataDir": util::data_dir()}));
    w("apps_list", apps::apps());
    w("startup_list", apps::startup());
    w("app_usage", apps::usage());
    w("dev_runtimes", dev::runtimes());
    w("dev_caches", dev::caches());
    w("games_list", games::games());
    w("stability_history", json!(stability::history(365)));
    w("downloads_insights", downloads::insights());
    w("monitor_status", stability::monitor_status());
    w("get_settings", load_settings());
    if let Some(root) = root {
        let excluded: Vec<String> = load_settings()["excluded"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
        let prog = Progress::new();
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).saturating_sub(2).clamp(2, 12);
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        let s = pool.install(|| scan::scan(&root, &excluded, &prog));
        w("scan_status", json!({"scanning": false, "has": true, "root": s.root, "size": s.nodes[0].size, "files": s.nodes[0].files, "ms": s.elapsed_ms as u64}));
        // pre-render the root view plus each big top-level child
        let mut views = serde_json::Map::new();
        views.insert("0".into(), s.view(0, 5));
        for &c in s.nodes[0].children.iter().take(12) {
            views.insert(c.to_string(), s.view(c, 5));
            for &g in s.nodes[c as usize].children.iter().take(6) { views.insert(g.to_string(), s.view(g, 5)); }
        }
        w("tree", Value::Object(views));
        let mut large: Vec<&FileRec> = s.user_files.iter().filter(|f| f.size >= 250 << 20).collect();
        large.sort_by(|a, b| b.size.cmp(&a.size)); large.truncate(400);
        let cutoff = util::now_unix() - 365 * 86_400;
        let mut old: Vec<&FileRec> = s.user_files.iter().filter(|f| f.size >= 10 << 20 && f.modified.max(f.accessed) < cutoff).collect();
        old.sort_by(|a, b| b.size.cmp(&a.size)); old.truncate(400);
        w("large_old", json!({"large": large, "old": old}));
        let mut a = s.artifacts.clone();
        a.sort_by(|x, y| y.size.cmp(&x.size));
        w("artifacts", json!(a));
        let p = AtomicU64::new(0);
        let pool4 = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
        w("duplicates", json!({"groups": pool4.install(|| scan::duplicates(&s.user_files, &p))}));
        eprintln!("scan: {} files, {} bytes, {} ms, {} denied", s.nodes[0].files, s.nodes[0].size, s.elapsed_ms, s.denied);
    }
}

fn main() {
    if std::env::args().any(|a| a == "--record-events") {
        stability::record();
        return;
    }
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--mcp") {
        mcp::serve();
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--bench-scan") {
        let root = args.get(i + 1).cloned().unwrap_or_else(|| "C:\\".into());
        let excluded: Vec<String> = load_settings()["excluded"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).saturating_sub(2).clamp(2, 12);
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        let s = pool.install(|| scan::scan(&root, &excluded, &Progress::new()));
        let t = std::time::Instant::now();
        s.save();
        let save_ms = t.elapsed().as_millis();
        let t = std::time::Instant::now();
        let ok = ScanResult::load_saved().is_some();
        println!("scan {} files, {} dirs in {} ms; save {} ms; load {} ms (ok={ok})", s.nodes[0].files, s.nodes.len(), s.elapsed_ms, save_ms, t.elapsed().as_millis());
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--dump") {
        dump(args.get(i + 1).map(String::as_str).unwrap_or("dump"), args.get(i + 2).cloned());
        return;
    }
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_settings, set_settings, overview, start_scan, scan_status, tree, node_path, large_old, duplicates,
            artifacts, downloads_insights, missing_paths, reveal, apps_list, startup_list, app_usage, dev_runtimes,
            dev_caches, games_list, stability_history, monitor_status, monitor_enable,
            telemetry_start, telemetry_stop, telemetry_status, claude_info, claude_desktop_connect
        ])
        .build(tauri::generate_context!())
        .expect("error while building Soundings")
        .run(|app, event| {
            // never leave nvidia-smi running after the window closes
            if let tauri::RunEvent::Exit = event {
                if let Some(st) = tauri::Manager::try_state::<AppState>(app) { st.0.telemetry.stop(); }
            }
        });
}
