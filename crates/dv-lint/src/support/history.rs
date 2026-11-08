//! Git history for the change-history rules (SV-EVO-001 hotspots, SV-EVO-002
//! change coupling). Nothing is read unless `--history` is given ([`set_history`]).

use crate::syntax::Cst;

use std::collections::HashMap;

use std::path::{Path, PathBuf};

use std::sync::atomic::{AtomicBool, Ordering};

use std::sync::{Arc, Mutex, OnceLock};

pub(crate) static HISTORY: AtomicBool = AtomicBool::new(false);

/// Turns on the git-history rules (uvmcheck's `--history`).
pub fn set_history(on: bool) {
    HISTORY.store(on, Ordering::Relaxed);
}

/// The commits of a repository, newest first.
pub(crate) struct Log {
    /// File (canonical path) -> indices of the commits that touch it.
    pub(crate) files: HashMap<String, Vec<u32>>,
    /// Files each commit touches.
    pub(crate) sizes: Vec<usize>,
}

#[derive(Default)]
pub(crate) struct Cache {
    /// Directory -> repository root (None: not in a repository).
    pub(crate) roots: HashMap<String, Option<String>>,
    pub(crate) logs: HashMap<(String, usize), Option<Arc<Log>>>,
}

pub(crate) fn cache() -> &'static Mutex<Cache> {
    static C: OnceLock<Mutex<Cache>> = OnceLock::new();
    C.get_or_init(Default::default)
}

pub(crate) fn canonical(p: &str) -> String {
    std::fs::canonicalize(p).map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|_| p.to_string())
}

pub(crate) fn git(dir: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-c")
        .arg("core.quotepath=off")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub(crate) fn read_log(root: &str, max: usize) -> Option<Log> {
    let n = max.to_string();
    let out = git(root, &["log", "--no-merges", "--no-renames", "--format=@@%H", "--name-only", "-n", &n])?;
    let mut log = Log { files: HashMap::new(), sizes: Vec::new() };
    for line in out.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with("@@") {
            log.sizes.push(0);
            continue;
        }
        if line.is_empty() || log.sizes.is_empty() {
            continue;
        }
        let ix = (log.sizes.len() - 1) as u32;
        *log.sizes.last_mut().unwrap() += 1;
        let p = normalize(&Path::new(root).join(line));
        log.files.entry(p).or_default().push(ix);
    }
    Some(log)
}

/// `a/./b/../c` -> `a/c`, without touching the file system.
pub(crate) fn normalize(p: &Path) -> String {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out.to_string_lossy().into_owned()
}

/// The log of the repository holding `path` (canonical), if any.
pub(crate) fn log_for(path: &str, max: usize) -> Option<Arc<Log>> {
    if !HISTORY.load(Ordering::Relaxed) {
        return None;
    }
    let dir = Path::new(path).parent()?.to_string_lossy().into_owned();
    let mut c = cache().lock().ok()?;
    let root = match c.roots.get(&dir) {
        Some(r) => r.clone(),
        None => {
            let r = git(&dir, &["rev-parse", "--show-toplevel"]).map(|s| canonical(s.trim_end()));
            c.roots.insert(dir, r.clone());
            r
        }
    }?;
    c.logs.entry((root.clone(), max)).or_insert_with(|| read_log(&root, max).map(Arc::new)).clone()
}

pub(crate) const MAX_COMMITS: usize = 1000;

pub(crate) fn base_name(p: &str) -> &str {
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}

/// The canonical path of the file a token was written in.
pub(crate) fn file_of(c: &Cst, tok: u32) -> String {
    canonical(&crate::support::statements::file_of(c, tok))
}
