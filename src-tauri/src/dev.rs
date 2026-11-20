//! Developer runtimes (every copy, which one wins on PATH) and heavy caches. Read-only:
//! the only processes run are `<tool> --version` with a timeout and no window.

use crate::util::{dir_size, dir_size_skip, glob, home, local_appdata, roaming_appdata, run_hidden};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

struct Tool {
    name: &'static str,
    exe: &'static str,
    args: &'static [&'static str],
    known: Vec<String>,
}

fn tools() -> Vec<Tool> {
    let la = local_appdata().display().to_string();
    let ra = roaming_appdata().display().to_string();
    let h = home().display().to_string();
    vec![
        Tool { name: "Python", exe: "python.exe", args: &["--version"], known: vec![
            format!(r"{la}\Programs\Python\Python*\python.exe"), r"C:\Python*\python.exe".into(),
            r"C:\Program Files\Python*\python.exe".into(), format!(r"{h}\miniconda3\python.exe"),
            format!(r"{h}\anaconda3\python.exe"), r"C:\ProgramData\miniconda3\python.exe".into(),
            r"C:\ProgramData\anaconda3\python.exe".into(), format!(r"{la}\uv\python\*\python.exe"),
            format!(r"{h}\.pyenv\pyenv-win\versions\*\python.exe"),
        ]},
        Tool { name: "Node.js", exe: "node.exe", args: &["--version"], known: vec![
            r"C:\Program Files\nodejs\node.exe".into(), format!(r"{ra}\nvm\v*\node.exe"),
            format!(r"{la}\Volta\tools\image\node\*\node.exe"), format!(r"{la}\fnm\node-versions\*\installation\node.exe"),
        ]},
        Tool { name: "Java", exe: "java.exe", args: &["-version"], known: vec![
            r"C:\Program Files\Java\*\bin\java.exe".into(), r"C:\Program Files\Eclipse Adoptium\*\bin\java.exe".into(),
            r"C:\Program Files\Microsoft\jdk-*\bin\java.exe".into(), r"C:\Program Files\Zulu\*\bin\java.exe".into(),
            r"C:\Program Files\Android\Android Studio\jbr\bin\java.exe".into(),
        ]},
        Tool { name: "Git", exe: "git.exe", args: &["--version"], known: vec![
            r"C:\Program Files\Git\cmd\git.exe".into(), format!(r"{la}\Programs\Git\cmd\git.exe"),
            format!(r"{la}\GitHubDesktop\app-*\resources\app\git\cmd\git.exe"),
        ]},
        Tool { name: "Rust", exe: "rustc.exe", args: &["--version"], known: vec![format!(r"{h}\.cargo\bin\rustc.exe")] },
        Tool { name: "Go", exe: "go.exe", args: &["version"], known: vec![r"C:\Program Files\Go\bin\go.exe".into()] },
        Tool { name: ".NET", exe: "dotnet.exe", args: &["--version"], known: vec![r"C:\Program Files\dotnet\dotnet.exe".into()] },
        Tool { name: "Conda", exe: "conda.exe", args: &["--version"], known: vec![
            format!(r"{h}\miniconda3\Scripts\conda.exe"), format!(r"{h}\anaconda3\Scripts\conda.exe"),
        ]},
        Tool { name: "uv", exe: "uv.exe", args: &["--version"], known: vec![format!(r"{h}\.local\bin\uv.exe")] },
        Tool { name: "Bun", exe: "bun.exe", args: &["--version"], known: vec![format!(r"{h}\.bun\bin\bun.exe")] },
        Tool { name: "Deno", exe: "deno.exe", args: &["--version"], known: vec![format!(r"{h}\.deno\bin\deno.exe")] },
    ]
}

/// Folder that "is" this install, for sizing.
fn install_root(tool: &str, exe: &Path) -> PathBuf {
    let dir = exe.parent().unwrap_or(exe).to_path_buf();
    let up = |p: &Path, n: usize| { let mut q = p.to_path_buf(); for _ in 0..n { if let Some(x) = q.parent() { q = x.to_path_buf(); } } q };
    match tool {
        "Java" => up(&dir, 1),
        "Git" => up(&dir, 1),
        "Rust" => up(&dir, 1),
        "Go" => up(&dir, 1),
        "Conda" => up(&dir, 1),
        _ => dir,
    }
}

/// conda's own version, read from its package record (running conda.exe can fail if it was moved).
fn conda_version(root: &Path) -> String {
    for p in glob(&format!("{}\\conda-meta\\conda-*.json", root.display())) {
        let n = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let parts: Vec<&str> = n.split('-').collect();
        if parts.len() >= 3 && parts[0] == "conda" && parts[1].chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            return format!("conda {}", parts[1]);
        }
    }
    "conda (version unknown)".into()
}

fn dotnet_runtimes(exe: &Path) -> String {
    let out = run_hidden(exe, &["--list-runtimes"], Duration::from_secs(6)).unwrap_or_default();
    let mut v: Vec<String> = out.lines().filter_map(|l| {
        let mut it = l.split_whitespace();
        let name = it.next()?; let ver = it.next()?;
        if name == "Microsoft.NETCore.App" { Some(ver.to_string()) } else { None }
    }).collect();
    v.dedup();
    if v.is_empty() { "Runtime only".into() } else { format!("Runtime {} (no SDK)", v.join(", ")) }
}

fn first_line(s: &str) -> String {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().chars().take(80).collect()
}

pub fn runtimes() -> Value {
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    let tools = tools();
    let results: Vec<Value> = tools
        .par_iter()
        .map(|t| {
            let mut seen = HashSet::new();
            let mut copies: Vec<(PathBuf, Option<usize>)> = vec![];
            for (i, d) in path_dirs.iter().enumerate() {
                let p = d.join(t.exe);
                if p.is_file() && seen.insert(p.to_string_lossy().to_lowercase()) { copies.push((p, Some(i))); }
            }
            for pat in &t.known {
                for p in glob(pat) {
                    if p.is_file() && seen.insert(p.to_string_lossy().to_lowercase()) { copies.push((p, None)); }
                }
            }
            let active = copies.iter().filter_map(|(_, i)| *i).min();
            let rows: Vec<Value> = copies
                .par_iter()
                .map(|(p, idx)| {
                    let low = p.to_string_lossy().to_lowercase();
                    let alias = low.contains("\\windowsapps\\");
                    let shim = low.contains("\\.cargo\\bin") || low.contains("\\shims\\") || low.contains("\\volta\\bin");
                    let root = install_root(t.name, p);
                    let version = if alias { "Store alias (not installed)".into() }
                        else if t.name == "Conda" { conda_version(&root) }
                        else {
                            let v = run_hidden(p, t.args, Duration::from_secs(6)).map(|s| first_line(&s)).unwrap_or_else(|| "?".into());
                            if t.name == ".NET" && !v.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                                dotnet_runtimes(p)
                            } else { v }
                        };
                    let (size, _) = if alias || shim || t.name == "Conda" { (0, 0) }
                        else if low.contains("conda3") { dir_size_skip(&root, &["envs", "pkgs"]) }
                        else { dir_size(&root) };
                    json!({"exe": p.to_string_lossy(), "root": root.to_string_lossy(), "version": version, "size": size,
                           "onPath": idx.is_some(), "active": idx.is_some() && *idx == active, "alias": alias})
                })
                .collect();
            let real = rows.iter().filter(|r| !r["alias"].as_bool().unwrap_or(false)).count();
            json!({"tool": t.name, "copies": rows, "count": real})
        })
        .filter(|v| v["copies"].as_array().map(|a| !a.is_empty()).unwrap_or(false))
        .collect();

    // conda environments
    let h = home();
    let mut envs = vec![];
    for base in [h.join("miniconda3"), h.join("anaconda3"), h.join(".conda"), PathBuf::from(r"C:\ProgramData\miniconda3")] {
        for e in glob(&format!("{}\\envs\\*", base.display())) {
            if e.is_dir() { envs.push(e); }
        }
    }
    let envs: Vec<Value> = envs.par_iter().map(|e| {
        let (s, f) = dir_size(e);
        json!({"name": e.file_name().map(|n| n.to_string_lossy().to_string()), "path": e.to_string_lossy(), "size": s, "files": f})
    }).collect();
    json!({"runtimes": results, "condaEnvs": envs})
}

pub fn caches() -> Value {
    let la = local_appdata();
    let ra = roaming_appdata();
    let h = home();
    let tmp = std::env::temp_dir();
    let list: Vec<(&str, &str, PathBuf)> = vec![
        ("Temporary files", "System", tmp.clone()),
        ("npm cache", "JavaScript", la.join("npm-cache")),
        ("pnpm store", "JavaScript", la.join("pnpm").join("store")),
        ("Yarn cache", "JavaScript", la.join("Yarn").join("Cache")),
        ("pip cache", "Python", la.join("pip").join("cache")),
        ("uv cache", "Python", la.join("uv").join("cache")),
        ("Conda packages", "Python", h.join("miniconda3").join("pkgs")),
        ("Conda packages (Anaconda)", "Python", h.join("anaconda3").join("pkgs")),
        ("Hugging Face models", "AI models", h.join(".cache").join("huggingface")),
        ("PyTorch hub", "AI models", h.join(".cache").join("torch")),
        ("Ollama models", "AI models", h.join(".ollama").join("models")),
        ("LM Studio models", "AI models", h.join(".lmstudio").join("models")),
        ("LM Studio models (old)", "AI models", h.join(".cache").join("lm-studio")),
        ("Cargo registry", "Rust", h.join(".cargo").join("registry")),
        ("Rust toolchains", "Rust", h.join(".rustup").join("toolchains")),
        ("NuGet packages", ".NET", h.join(".nuget").join("packages")),
        ("Gradle caches", "Java", h.join(".gradle").join("caches")),
        ("Maven repository", "Java", h.join(".m2").join("repository")),
        ("Docker disk image", "Containers", la.join("Docker").join("wsl")),
        ("Android SDK", "Mobile", la.join("Android").join("Sdk")),
        ("VS Code extensions", "Editors", h.join(".vscode").join("extensions")),
        ("Cursor extensions", "Editors", h.join(".cursor").join("extensions")),
        ("NVIDIA shader cache", "Graphics", la.join("NVIDIA").join("DXCache")),
        ("NVIDIA GL cache", "Graphics", la.join("NVIDIA").join("GLCache")),
        ("AMD shader cache", "Graphics", la.join("AMD").join("DxcCache")),
        ("Chrome cache", "Browsers", la.join(r"Google\Chrome\User Data\Default\Cache")),
        ("Edge cache", "Browsers", la.join(r"Microsoft\Edge\User Data\Default\Cache")),
        ("Electron/Squirrel temp", "Apps", ra.join("SquirrelTemp")),
    ];
    let rows: Vec<Value> = list
        .par_iter()
        .filter(|(_, _, p)| p.is_dir())
        .map(|(name, group, p)| {
            let (s, f) = dir_size(p);
            json!({"name": name, "group": group, "path": p.to_string_lossy(), "size": s, "files": f})
        })
        .filter(|v| v["size"].as_u64().unwrap_or(0) > 0)
        .collect();
    json!(rows)
}
