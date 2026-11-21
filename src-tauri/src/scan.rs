//! Parallel, read-only disk scanner. Builds a folder tree (sizes only, files are never opened),
//! and collects user files >= 1 MB for the Large/Old/Duplicates views.

use crate::util::{ft_to_unix, home};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// One directory entry, straight from the Windows directory listing (no extra calls per file).
struct Ent {
    name: String,
    dir: bool,
    size: u64,
    mtime: u64,
    atime: u64,
    ctime: u64,
}

/// Lists a directory with FindFirstFileExW in "basic" mode with large fetches: no 8.3 short names,
/// bigger batches from the file system, sizes and dates included. Symlinks and junctions are left out
/// (never followed); other reparse points such as OneDrive placeholders are kept.
fn list_dir(path: &Path) -> Option<Vec<Ent>> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::*;
    let s = path.as_os_str().to_string_lossy();
    let mut pat = if s.starts_with(r"\\") { s.to_string() } else { format!(r"\\?\{s}") };
    if !pat.ends_with('\\') { pat.push('\\'); }
    pat.push('*');
    let wide: Vec<u16> = std::ffi::OsStr::new(&pat).encode_wide().chain(Some(0)).collect();
    let mut d: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let h = unsafe {
        FindFirstFileExW(wide.as_ptr(), FindExInfoBasic, &mut d as *mut _ as *mut _, FindExSearchNameMatch,
                         std::ptr::null(), FIND_FIRST_EX_LARGE_FETCH)
    };
    if h == INVALID_HANDLE_VALUE { return None; }
    let mut out = Vec::new();
    loop {
        let len = d.cFileName.iter().position(|&c| c == 0).unwrap_or(d.cFileName.len());
        let n = &d.cFileName[..len];
        let dot = n == [46] || n == [46, 46];
        // IO_REPARSE_TAG name-surrogate bit: symlinks and mount points / junctions
        let link = d.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 && d.dwReserved0 & 0x2000_0000 != 0;
        if !dot && !link {
            let ft = |f: windows_sys::Win32::Foundation::FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
            out.push(Ent {
                name: std::ffi::OsString::from_wide(n).to_string_lossy().into_owned(),
                dir: d.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0,
                size: ((d.nFileSizeHigh as u64) << 32) | d.nFileSizeLow as u64,
                mtime: ft(d.ftLastWriteTime),
                atime: ft(d.ftLastAccessTime),
                ctime: ft(d.ftCreationTime),
            });
        }
        if unsafe { FindNextFileW(h, &mut d) } == 0 { break; }
    }
    unsafe { FindClose(h) };
    Some(out)
}

#[derive(Serialize, Deserialize)]
pub struct Node {
    pub name: Box<str>,
    pub parent: u32,
    pub size: u64,
    pub files: u64,
    pub children: Vec<u32>,
    pub top_files: Vec<(Box<str>, u64)>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FileRec {
    pub path: String,
    pub size: u64,
    pub modified: i64,
    pub accessed: i64,
    pub created: i64,
    /// Inside a git repository or worktree: never auto-selected for removal.
    pub repo: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Artifact {
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub files: u64,
    pub modified: i64,
}

#[derive(Serialize, Deserialize)]
pub struct ScanResult {
    pub root: String,
    pub nodes: Vec<Node>,
    pub user_files: Vec<FileRec>,
    pub artifacts: Vec<Artifact>,
    pub denied: u64,
    pub elapsed_ms: u128,
    /// When the scan finished (unix seconds).
    pub taken: i64,
}

const SAVED_VERSION: u32 = 1;
fn saved_path() -> PathBuf { crate::util::data_dir().join("cache").join("scan.bin") }

impl ScanResult {
    /// Keep the last map on this PC so the app opens with it instantly. Local only, never uploaded.
    pub fn save(&self) {
        let p = saved_path();
        let _ = fs::create_dir_all(p.parent().unwrap());
        let tmp = p.with_extension("tmp");
        if let Ok(f) = fs::File::create(&tmp) {
            let mut w = std::io::BufWriter::new(f);
            if bincode::serialize_into(&mut w, &(SAVED_VERSION, self)).is_ok() {
                drop(w);
                let _ = fs::rename(&tmp, &p);
            }
        }
    }
    pub fn load_saved() -> Option<ScanResult> {
        let f = fs::File::open(saved_path()).ok()?;
        let (v, s): (u32, ScanResult) = bincode::deserialize_from(std::io::BufReader::new(f)).ok()?;
        (v == SAVED_VERSION).then_some(s)
    }
}

pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub denied: AtomicU64,
}
impl Progress {
    pub fn new() -> Self {
        Progress { files: AtomicU64::new(0), dirs: AtomicU64::new(0), bytes: AtomicU64::new(0), denied: AtomicU64::new(0) }
    }
}

struct Ctx<'a> {
    prog: &'a Progress,
    user_root: String,     // lowercase "c:\users\name\"
    user_skip: String,     // lowercase "c:\users\name\appdata\"
    excluded: Vec<String>, // lowercase full paths
    files_out: Mutex<Vec<FileRec>>,
    artifacts_out: Mutex<Vec<Artifact>>,
}

struct Tmp {
    name: Box<str>,
    size: u64,
    files: u64,
    children: Vec<Tmp>,
    top_files: Vec<(Box<str>, u64)>,
}

const ARTIFACT_DIRS: &[(&str, &str)] = &[
    ("node_modules", "node_modules"),
    (".venv", "Python venv"),
    ("venv", "Python venv"),
    ("target", "Rust/Java build"),
    (".next", "Next.js cache"),
    (".gradle", "Gradle cache"),
    (".turbo", "Turborepo cache"),
    (".parcel-cache", "Parcel cache"),
];

fn walk(path: &Path, name: Box<str>, ctx: &Ctx, in_artifact: bool, in_repo: bool) -> Tmp {
    let mut t = Tmp { name, size: 0, files: 0, children: vec![], top_files: vec![] };
    ctx.prog.dirs.fetch_add(1, Ordering::Relaxed);
    let Some(rd) = list_dir(path) else {
        ctx.prog.denied.fetch_add(1, Ordering::Relaxed);
        return t;
    };
    let in_repo = in_repo || rd.iter().any(|e| e.name == ".git");
    let lower = path.to_string_lossy().to_lowercase();
    let lower = if lower.ends_with('\\') { lower } else { format!("{lower}\\") };
    let in_user = lower.starts_with(&ctx.user_root) && !lower.starts_with(&ctx.user_skip);
    let mut subdirs: Vec<(PathBuf, Box<str>)> = vec![];
    let mut my_files: Vec<FileRec> = vec![];
    for e in rd {
        if e.dir {
            if !ctx.excluded.is_empty() {
                let child_lower = format!("{lower}{}", e.name.to_lowercase());
                if ctx.excluded.iter().any(|x| *x == child_lower) { continue; }
            }
            subdirs.push((path.join(&e.name), e.name.into_boxed_str()));
        } else {
            let sz = e.size;
            t.size += sz;
            t.files += 1;
            if sz >= 1 << 20 {
                if in_user && !in_artifact {
                    my_files.push(FileRec {
                        path: path.join(&e.name).to_string_lossy().into_owned(),
                        size: sz,
                        modified: ft_to_unix(e.mtime),
                        accessed: ft_to_unix(e.atime),
                        created: ft_to_unix(e.ctime),
                        repo: in_repo,
                    });
                }
                t.top_files.push((e.name.into_boxed_str(), sz));
            }
        }
    }
    ctx.prog.files.fetch_add(t.files, Ordering::Relaxed);
    ctx.prog.bytes.fetch_add(t.size, Ordering::Relaxed);
    if !my_files.is_empty() {
        ctx.files_out.lock().unwrap().extend(my_files);
    }
    if t.top_files.len() > 24 {
        t.top_files.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        t.top_files.truncate(24);
    }

    let kids: Vec<(Tmp, Option<&str>)> = subdirs
        .into_par_iter()
        .map(|(p, n)| {
            let lname = n.to_lowercase();
            let art = if in_user && !in_artifact {
                ARTIFACT_DIRS.iter().find(|(d, _)| *d == lname).map(|(_, k)| *k)
            } else {
                None
            };
            let sub = walk(&p, n, ctx, in_artifact || art.is_some(), in_repo);
            if let Some(kind) = art {
                if sub.size > 0 {
                    ctx.artifacts_out.lock().unwrap().push(Artifact {
                        path: p.to_string_lossy().to_string(),
                        kind: kind.to_string(),
                        size: sub.size,
                        files: sub.files,
                        modified: crate::util::modified_unix(p.parent().unwrap_or(&p)),
                    });
                }
            }
            (sub, art)
        })
        .collect();
    for (k, _) in kids {
        t.size += k.size;
        t.files += k.files;
        t.children.push(k);
    }
    t.children.sort_unstable_by(|a, b| b.size.cmp(&a.size));
    t
}

fn flatten(root: Tmp) -> Vec<Node> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut stack: Vec<(Tmp, u32)> = vec![(root, u32::MAX)];
    while let Some((t, parent)) = stack.pop() {
        let id = nodes.len() as u32;
        nodes.push(Node { name: t.name, parent, size: t.size, files: t.files, children: vec![], top_files: t.top_files });
        if parent != u32::MAX {
            nodes[parent as usize].children.push(id);
        }
        // push in reverse so children end up in size order
        for c in t.children.into_iter().rev() {
            stack.push((c, id));
        }
    }
    // children were pushed in reverse pop order; re-sort by size
    let sizes: Vec<u64> = nodes.iter().map(|n| n.size).collect();
    for n in nodes.iter_mut() {
        n.children.sort_unstable_by(|a, b| sizes[*b as usize].cmp(&sizes[*a as usize]));
    }
    nodes
}

pub fn scan(root: &str, excluded: &[String], prog: &Progress) -> ScanResult {
    let start = std::time::Instant::now();
    let h = home().to_string_lossy().to_lowercase();
    let ctx = Ctx {
        prog,
        user_root: format!("{h}\\"),
        user_skip: format!("{h}\\appdata\\"),
        excluded: excluded.iter().map(|s| s.trim_end_matches('\\').to_lowercase()).collect(),
        files_out: Mutex::new(vec![]),
        artifacts_out: Mutex::new(vec![]),
    };
    let rootp = PathBuf::from(root);
    let t = walk(&rootp, root.to_string().into_boxed_str(), &ctx, false, false);
    let nodes = flatten(t);
    ScanResult {
        root: root.to_string(),
        nodes,
        user_files: ctx.files_out.into_inner().unwrap(),
        artifacts: ctx.artifacts_out.into_inner().unwrap(),
        denied: prog.denied.load(Ordering::Relaxed),
        elapsed_ms: start.elapsed().as_millis(),
        taken: crate::util::now_unix(),
    }
}

impl ScanResult {
    /// Node id for a full folder path inside the map (case-insensitive).
    pub fn find(&self, path: &str) -> Option<u32> {
        let root = self.root.trim_end_matches('\\').to_lowercase();
        let want = path.trim().trim_end_matches('\\').replace('/', "\\").to_lowercase();
        let rest = want.strip_prefix(&root)?;
        let mut cur = 0u32;
        for part in rest.split('\\').filter(|p| !p.is_empty()) {
            cur = *self.nodes[cur as usize].children.iter().find(|&&c| self.nodes[c as usize].name.to_lowercase() == part)?;
        }
        Some(cur)
    }

    pub fn path_of(&self, id: u32) -> String {
        let mut parts = vec![];
        let mut cur = id;
        while cur != u32::MAX {
            let n = &self.nodes[cur as usize];
            parts.push(n.name.to_string());
            cur = n.parent;
        }
        parts.reverse();
        let mut p = PathBuf::from(&parts[0]);
        for s in &parts[1..] {
            p.push(s);
        }
        p.to_string_lossy().to_string()
    }

    /// Subtree for the sunburst: `depth` levels, children below 0.15% of the focus node are merged.
    pub fn view(&self, id: u32, depth: u32) -> Value {
        let focus = &self.nodes[id as usize];
        let min = (focus.size as f64 * 0.0015) as u64;
        let mut crumbs = vec![];
        let mut cur = id;
        while cur != u32::MAX {
            let n = &self.nodes[cur as usize];
            crumbs.push(json!({"id": cur, "name": n.name.as_ref(), "size": n.size}));
            cur = n.parent;
        }
        crumbs.reverse();
        json!({
            "root": self.subtree(id, depth, min),
            "path": self.path_of(id),
            "crumbs": crumbs,
        })
    }

    fn subtree(&self, id: u32, depth: u32, min: u64) -> Value {
        let n = &self.nodes[id as usize];
        let mut children = vec![];
        let mut other = 0u64;
        let mut other_n = 0u64;
        if depth > 0 {
            for &c in &n.children {
                let cn = &self.nodes[c as usize];
                if cn.size >= min && children.len() < 80 {
                    children.push(self.subtree(c, depth - 1, min));
                } else {
                    other += cn.size;
                    other_n += 1;
                }
            }
            for (fname, fsize) in &n.top_files {
                if *fsize >= min && children.len() < 100 {
                    children.push(json!({"name": fname.as_ref(), "size": fsize, "file": true}));
                } else {
                    other += fsize;
                    other_n += 1;
                }
            }
            children.sort_by(|a, b| b["size"].as_u64().cmp(&a["size"].as_u64()));
            let listed: u64 = children.iter().map(|c| c["size"].as_u64().unwrap_or(0)).sum();
            let loose = n.size.saturating_sub(listed + other);
            if loose > 0 {
                other += loose;
            }
            if other > 0 {
                children.push(json!({"name": format!("{} smaller items", other_n.max(1)), "size": other, "other": true}));
            }
        }
        json!({"id": id, "name": n.name.as_ref(), "size": n.size, "files": n.files, "children": children})
    }
}

/// Duplicate groups among user files: same size -> same first 64 KB -> same full hash.
pub fn duplicates(files: &[FileRec], prog: &AtomicU64) -> Vec<Value> {
    let mut by_size: HashMap<u64, Vec<&FileRec>> = HashMap::new();
    for f in files {
        by_size.entry(f.size).or_default().push(f);
    }
    let cands: Vec<Vec<&FileRec>> = by_size.into_values().filter(|v| v.len() > 1).collect();
    let mut groups: Vec<Value> = cands
        .par_iter()
        .flat_map(|grp| {
            let quick = group_by(grp, |f| hash_file(&f.path, Some(64 * 1024)), prog);
            quick
                .into_iter()
                .filter(|g| g.len() > 1)
                .flat_map(|g| group_by(&g, |f| hash_file(&f.path, None), prog))
                .filter(|g| g.len() > 1)
                .map(|mut g| {
                    g.sort_by_key(|f| f.created);
                    let size = g[0].size;
                    json!({"size": size, "wasted": size * (g.len() as u64 - 1), "files": g})
                })
                .collect::<Vec<_>>()
        })
        .collect();
    groups.sort_by(|a, b| b["wasted"].as_u64().cmp(&a["wasted"].as_u64()));
    groups.truncate(1000);
    groups
}

fn group_by<'a>(items: &[&'a FileRec], key: impl Fn(&FileRec) -> Option<[u8; 32]>, prog: &AtomicU64) -> Vec<Vec<&'a FileRec>> {
    let mut m: HashMap<[u8; 32], Vec<&FileRec>> = HashMap::new();
    for f in items {
        prog.fetch_add(1, Ordering::Relaxed);
        if let Some(k) = key(f) {
            m.entry(k).or_default().push(f);
        }
    }
    m.into_values().collect()
}

fn hash_file(path: &str, limit: Option<usize>) -> Option<[u8; 32]> {
    use std::io::Read;
    thread_local! { static BUF: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(vec![0u8; 1 << 20]); }
    let mut f = fs::File::open(path).ok()?;
    let mut h = blake3::Hasher::new();
    BUF.with(|b| {
        let buf = &mut b.borrow_mut()[..];
        let mut left = limit.unwrap_or(usize::MAX);
        while left > 0 {
            let want = buf.len().min(left);
            let n = f.read(&mut buf[..want]).ok()?;
            if n == 0 { break; }
            h.update(&buf[..n]);
            left -= n;
        }
        Some(*h.finalize().as_bytes())
    })
}
