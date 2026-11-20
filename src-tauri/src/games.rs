//! Game library discovery. Pass 1: launcher manifests. Pass 2: engine fingerprints.
//! Read-only; never reads launcher credential/token files.

use crate::util::{dir_size, glob, home, local_appdata, modified_unix, read_text_lossy, roaming_appdata};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use winreg::enums::*;
use winreg::RegKey;

#[derive(Clone)]
struct Game {
    name: String,
    source: &'static str,
    path: PathBuf,
    size: u64,
    last_played: i64,
    confidence: f32,
    signals: Vec<String>,
}

/// Minimal Valve KeyValues reader: returns the first "key" "value" pair for `key` (case-insensitive).
fn vdf_get(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key.to_lowercase());
    let lower = text.to_lowercase();
    let i = lower.find(&needle)? + needle.len();
    let rest = &text[i..];
    let a = rest.find('"')? + 1;
    let b = a + rest[a..].find('"')?;
    Some(rest[a..b].replace("\\\\", "\\"))
}

fn vdf_all(text: &str, key: &str) -> Vec<String> {
    let mut out = vec![];
    let needle = format!("\"{}\"", key);
    let mut pos = 0;
    while let Some(i) = text[pos..].find(&needle) {
        let start = pos + i + needle.len();
        let rest = &text[start..];
        if let Some(a) = rest.find('"') {
            if let Some(b) = rest[a + 1..].find('"') {
                out.push(rest[a + 1..a + 1 + b].replace("\\\\", "\\"));
            }
        }
        pos = start;
    }
    out
}

const STEAM_TOOLS: &[&str] = &["228980", "250820", "1070560", "1391110", "1628350", "431960"];

fn steam(out: &mut Vec<Game>, leftovers: &mut Vec<Value>) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let root = hkcu
        .open_subkey(r"Software\Valve\Steam")
        .ok()
        .and_then(|k| k.get_value::<String, _>("SteamPath").ok())
        .map(|s| PathBuf::from(s.replace('/', "\\")))
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam"));
    let mut libs = vec![root.clone()];
    if let Some(t) = read_text_lossy(&root.join(r"steamapps\libraryfolders.vdf")) {
        for p in vdf_all(&t, "path") {
            let p = PathBuf::from(p.replace('/', "\\"));
            let key = p.to_string_lossy().to_lowercase();
            if !libs.iter().any(|l| l.to_string_lossy().to_lowercase() == key) { libs.push(p); }
        }
    }
    for lib in libs {
        let apps = lib.join("steamapps");
        let mut installdirs = HashSet::new();
        let Ok(rd) = fs::read_dir(&apps) else { continue };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if !(n.starts_with("appmanifest_") && n.ends_with(".acf")) { continue; }
            let Some(t) = read_text_lossy(&e.path()) else { continue };
            let id = vdf_get(&t, "appid").unwrap_or_default();
            let dir = vdf_get(&t, "installdir").unwrap_or_default();
            installdirs.insert(dir.to_lowercase());
            if STEAM_TOOLS.contains(&id.as_str()) { continue; }
            out.push(Game {
                name: vdf_get(&t, "name").unwrap_or(dir.clone()),
                source: "Steam",
                path: apps.join("common").join(&dir),
                size: vdf_get(&t, "SizeOnDisk").and_then(|s| s.parse().ok()).unwrap_or(0),
                last_played: vdf_get(&t, "LastPlayed").and_then(|s| s.parse().ok()).unwrap_or(0),
                confidence: 1.0,
                signals: vec![format!("Steam app {id}")],
            });
        }
        if let Ok(rd) = fs::read_dir(apps.join("common")) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if !installdirs.contains(&n.to_lowercase()) && e.path().is_dir() {
                    let (s, _) = dir_size(&e.path());
                    leftovers.push(json!({"name": n, "path": e.path().to_string_lossy(), "size": s}));
                }
            }
        }
    }
}

fn epic(out: &mut Vec<Game>) {
    let dir = PathBuf::from(r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(rd) = fs::read_dir(&dir) else { return };
    for e in rd.flatten() {
        if e.path().extension().map(|x| x != "item").unwrap_or(true) { continue; }
        let Some(t) = read_text_lossy(&e.path()) else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
        let cats = v["AppCategories"].as_array().map(|a| a.iter().any(|c| c == "games")).unwrap_or(false);
        if !cats { continue; }
        out.push(Game {
            name: v["DisplayName"].as_str().unwrap_or("?").into(),
            source: "Epic",
            path: PathBuf::from(v["InstallLocation"].as_str().unwrap_or("")),
            size: v["InstallSize"].as_u64().unwrap_or(0),
            last_played: 0,
            confidence: 1.0,
            signals: vec!["Epic manifest".into()],
        });
    }
}

fn reg_games(out: &mut Vec<Game>) {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(k) = hklm.open_subkey(r"SOFTWARE\WOW6432Node\GOG.com\Games") {
        for id in k.enum_keys().flatten() {
            if let Ok(g) = k.open_subkey(&id) {
                let name: String = g.get_value("gameName").unwrap_or_default();
                let path: String = g.get_value("path").unwrap_or_default();
                if !name.is_empty() {
                    out.push(Game { name, source: "GOG", path: path.into(), size: 0, last_played: 0, confidence: 1.0, signals: vec!["GOG registry".into()] });
                }
            }
        }
    }
    if let Ok(k) = hklm.open_subkey(r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs") {
        for id in k.enum_keys().flatten() {
            if let Ok(g) = k.open_subkey(&id) {
                let path: String = g.get_value("InstallDir").unwrap_or_default();
                let p = PathBuf::from(path.replace('/', "\\"));
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(id.clone());
                out.push(Game { name, source: "Ubisoft", path: p, size: 0, last_played: 0, confidence: 1.0, signals: vec!["Ubisoft registry".into()] });
            }
        }
    }
    if let Ok(k) = hklm.open_subkey(r"SOFTWARE\WOW6432Node\Rockstar Games") {
        for t in k.enum_keys().flatten() {
            let l = t.to_lowercase();
            if l.contains("launcher") || l.contains("social club") || l.contains("sdk") { continue; }
            if let Ok(g) = k.open_subkey(&t) {
                if let Ok(path) = g.get_value::<String, _>("InstallFolder") {
                    out.push(Game { name: t, source: "Rockstar", path: path.into(), size: 0, last_played: 0, confidence: 1.0, signals: vec!["Rockstar registry".into()] });
                }
            }
        }
    }
    // Generic: uninstall entries from game publishers
    let pubs = ["riot games", "electronic arts", "blizzard", "activision", "bethesda", "cd projekt", "bandai namco",
                "square enix", "capcom", "sega", "2k", "krafton", "hoyoverse", "mihoyo", "cognosphere", "garena", "valve",
                "ubisoft", "rockstar games", "mojang", "paradox", "devolver", "team cherry", "re-logic", "innersloth"];
    let bad = ["launcher", "client", "social club", "sdk", "online services", "redist", "anti-cheat", "anticheat", "updater", "overlay", "studio", "app"];
    let launchers = ["steam", "epic games launcher", "ea app", "ea", "origin", "ubisoft connect", "uplay", "battle.net",
                     "gog galaxy", "riot vanguard", "minecraft launcher", "xbox", "rockstar games launcher"];
    for a in crate::apps::raw_apps() {
        let p = a.publisher.to_lowercase();
        let n = a.name.to_lowercase();
        if launchers.contains(&n.trim()) { continue; }
        if pubs.iter().any(|x| p.contains(x)) && !bad.iter().any(|b| n.contains(b)) {
            out.push(Game { name: a.name, source: "Installed program", path: a.location.into(), size: a.size, last_played: 0, confidence: 0.7, signals: vec![format!("Publisher: {}", a.publisher)] });
        }
    }
}

fn store_and_misc(out: &mut Vec<Game>) {
    // Xbox app installs
    for d in crate::system::drives() {
        let root = PathBuf::from(d["root"].as_str().unwrap_or("C:\\")).join("XboxGames");
        if let Ok(rd) = fs::read_dir(&root) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n == "GameSave" || n.starts_with('.') || !e.path().is_dir() { continue; }
                out.push(Game { name: n, source: "Xbox", path: e.path(), size: 0, last_played: 0, confidence: 0.95, signals: vec!["XboxGames folder".into()] });
            }
        }
    }
    let mc = roaming_appdata().join(".minecraft");
    let versions = mc.join("versions");
    if versions.is_dir() {
        let n = fs::read_dir(&versions).map(|r| r.count()).unwrap_or(0);
        out.push(Game { name: "Minecraft: Java Edition".into(), source: "Minecraft", path: mc.clone(), size: 0, last_played: modified_unix(&mc.join("launcher_profiles.json")),
                        confidence: if n > 0 { 0.9 } else { 0.5 }, signals: vec![format!("{n} versions installed")] });
    }
    let rbx = local_appdata().join("Roblox");
    if !glob(&format!("{}\\Versions\\*\\RobloxPlayerBeta.exe", rbx.display())).is_empty() {
        out.push(Game { name: "Roblox".into(), source: "Roblox", path: rbx.clone(), size: 0, last_played: modified_unix(&rbx.join("logs")), confidence: 0.95, signals: vec!["RobloxPlayerBeta.exe".into()] });
    }
}

// ---------- Pass 2: engine fingerprints ----------
const REJECT: &[&str] = &[
    "node_modules", "site-packages", "\\.git", "unity\\hub", "unity hub", "editor\\data", "epic games\\launcher",
    "epic online services", "android", "jetbrains", "microsoft visual studio", "windows kits", "nvidia corporation",
    "lenovo", "steamworks shared", "_commonredist", "directxredist", "redist", "rockstar games\\launcher", "riot client",
    "cursor", "microsoft vs code", "antigravity", "obsidian", "lm studio", "ollama", "windowsapps", "docker", "miniconda3",
    "anaconda3", "msys64", "flutter", ".cargo", ".rustup", "cisco packet tracer", "blender foundation", "obs-studio",
];

fn score_dir(p: &Path) -> (f32, Vec<String>) {
    let Ok(rd) = fs::read_dir(p) else { return (0.0, vec![]) };
    let names: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().to_lowercase()).collect();
    let has = |n: &str| names.iter().any(|x| x == n);
    let mut s = 0.0f32;
    let mut sig = vec![];
    let mut add = |w: f32, why: &str| { s += w; sig.push(why.to_string()); };
    if has("unityplayer.dll") && names.iter().any(|n| n.ends_with("_data")) { add(0.6, "Unity player"); }
    if has("gameassembly.dll") { add(0.2, "IL2CPP"); }
    if names.iter().any(|n| n.ends_with("-win64-shipping.exe") && !n.starts_with("eossdk")) { add(0.7, "Unreal shipping build"); }
    if has("steam_api64.dll") || has("steam_api.dll") || has("steam_appid.txt") { add(0.35, "Steamworks"); }
    if has("galaxy64.dll") { add(0.3, "GOG Galaxy SDK"); }
    if has("easyanticheat") || has("battleye") { add(0.3, "Anti-cheat"); }
    if names.iter().any(|n| n.ends_with(".pck")) && names.iter().any(|n| n.ends_with(".exe")) { add(0.4, "Godot pack"); }
    if has("renpy") || has("data.win") { add(0.5, "Ren'Py/GameMaker"); }
    if has("bink2w64.dll") || has("binkw64.dll") { add(0.15, "Bink video"); }
    if names.iter().any(|n| n.starts_with("fmod") || n.starts_with("physx3") || n == "discord_game_sdk.dll") { add(0.1, "Game middleware"); }
    if p.join("resources").join("app.asar").exists() { s -= 0.6; }
    if names.iter().any(|n| ["unity.exe", "unityhub.exe", "unrealeditor.exe", "ue4editor.exe", "blender.exe", "obs64.exe"].contains(&n.as_str())) { s -= 1.0; }
    (s.min(1.0), sig)
}

fn heuristic(roots: Vec<PathBuf>, known: &[PathBuf]) -> Vec<Game> {
    let known: Vec<String> = known.iter().map(|p| p.to_string_lossy().to_lowercase()).filter(|s| s.len() > 3).collect();
    roots
        .into_par_iter()
        .flat_map(|root| {
            let mut found = vec![];
            let mut stack = vec![(root, 0u32)];
            while let Some((d, depth)) = stack.pop() {
                let low = d.to_string_lossy().to_lowercase();
                if REJECT.iter().any(|r| low.contains(r)) { continue; }
                if known.iter().any(|k| low.starts_with(k.as_str())) { continue; }
                let (s, sig) = score_dir(&d);
                if s >= 0.5 {
                    // map Unreal ...\<Proj>\Binaries\Win64 up to the game root
                    let mut gp = d.clone();
                    if low.ends_with("\\binaries\\win64") {
                        if let Some(r) = gp.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) { gp = r.to_path_buf(); }
                    }
                    let name = gp.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    found.push(Game { name, source: "Detected", path: gp, size: 0, last_played: 0, confidence: s, signals: sig });
                    continue;
                }
                if depth >= 5 { continue; }
                if let Ok(rd) = fs::read_dir(&d) {
                    for e in rd.flatten() {
                        if let Ok(ft) = e.file_type() {
                            if ft.is_dir() && !ft.is_symlink() { stack.push((e.path(), depth + 1)); }
                        }
                    }
                }
            }
            found
        })
        .collect()
}

pub fn games() -> Value {
    let mut list = vec![];
    let mut leftovers = vec![];
    steam(&mut list, &mut leftovers);
    epic(&mut list);
    reg_games(&mut list);
    store_and_misc(&mut list);

    let h = home();
    let mut roots = vec![
        PathBuf::from(r"C:\Program Files"), PathBuf::from(r"C:\Program Files (x86)"), PathBuf::from(r"C:\Games"),
        local_appdata().join("Programs"), h.join("Games"), h.join("Desktop"), h.join("Downloads"),
    ];
    for d in crate::system::drives() {
        let r = d["root"].as_str().unwrap_or("C:\\");
        if !r.starts_with('C') { roots.push(PathBuf::from(r)); }
    }
    let known: Vec<PathBuf> = list.iter().map(|g| g.path.clone()).collect();
    list.extend(heuristic(roots.into_iter().filter(|p| p.is_dir()).collect(), &known));

    // de-duplicate by name, keep highest confidence
    list.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
    let mut seen = HashSet::new();
    list.retain(|g| seen.insert(g.name.to_lowercase()));

    // fill missing sizes
    list.par_iter_mut().for_each(|g| {
        if g.size == 0 && g.path.is_dir() { g.size = dir_size(&g.path).0; }
        if g.last_played == 0 && g.path.is_dir() { g.last_played = 0; }
    });
    let (games, maybe): (Vec<_>, Vec<_>) = list.into_iter().partition(|g| g.confidence >= 0.5);
    let to_json = |g: &Game| json!({"name": g.name, "source": g.source, "path": g.path.to_string_lossy(), "size": g.size,
                                     "lastPlayed": g.last_played, "confidence": g.confidence, "signals": g.signals});
    json!({
        "games": games.iter().map(to_json).collect::<Vec<_>>(),
        "maybe": maybe.iter().map(to_json).collect::<Vec<_>>(),
        "leftovers": leftovers,
    })
}
