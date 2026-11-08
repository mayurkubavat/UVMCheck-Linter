//! Methodology guide MG-9.2, change history: SV-EVO-001 (hotspots) and
//! SV-EVO-002 (change coupling). Both read git and do nothing unless
//! `--history` is given ([`set_history`]).

use crate::syntax::Cst;
use crate::{Rule, Severity, Sink};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-EVO-001", name: "Hotspot", check: crate::Check::Mixed(hotspot) },
        Rule { id: "SV-EVO-002", name: "ChangeCoupling", check: crate::Check::Mixed(change_coupling) },
    ]
}

static HISTORY: AtomicBool = AtomicBool::new(false);

/// Turns on the git-history rules (uvmcheck's `--history`).
pub fn set_history(on: bool) {
    HISTORY.store(on, Ordering::Relaxed);
}

/// The commits of a repository, newest first.
struct Log {
    /// File (canonical path) -> indices of the commits that touch it.
    files: HashMap<String, Vec<u32>>,
    /// Files each commit touches.
    sizes: Vec<usize>,
}

#[derive(Default)]
struct Cache {
    /// Directory -> repository root (None: not in a repository).
    roots: HashMap<String, Option<String>>,
    logs: HashMap<(String, usize), Option<Arc<Log>>>,
}

fn cache() -> &'static Mutex<Cache> {
    static C: OnceLock<Mutex<Cache>> = OnceLock::new();
    C.get_or_init(Default::default)
}

fn canonical(p: &str) -> String {
    std::fs::canonicalize(p).map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|_| p.to_string())
}

fn git(dir: &str, args: &[&str]) -> Option<String> {
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

fn read_log(root: &str, max: usize) -> Option<Log> {
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
fn normalize(p: &Path) -> String {
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
fn log_for(path: &str, max: usize) -> Option<Arc<Log>> {
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
    c.logs
        .entry((root.clone(), max))
        .or_insert_with(|| read_log(&root, max).map(Arc::new))
        .clone()
}

const MAX_COMMITS: usize = 1000;

fn base_name(p: &str) -> &str {
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}

/// The canonical path of the file a token was written in.
fn file_of(c: &Cst, tok: u32) -> String {
    canonical(&crate::g_design::file_of(c, tok))
}

/// SV-EVO-001: the five files with the largest complexity times churn.
fn hotspot(_: &dv_model::Design, csts: &[Cst], out: &mut Sink) {
    if !HISTORY.load(Ordering::Relaxed) {
        return;
    }
    let (top_files, min_commits, min_complexity) = (5usize, 3usize, 15i64);
    struct Stats {
        complexity: i64,
        worst: i64,
        /// The most complex routine: its tree and name token, and name.
        at: Option<(usize, u32, String)>,
        churn: usize,
        log_size: usize,
    }
    let mut files: BTreeMap<String, Stats> = BTreeMap::new();
    for (k, c) in csts.iter().enumerate() {
        let classes = crate::g_design_class::Classes::new(c);
        for r in crate::g_design_metrics::user_routines(c, &classes) {
            let file = file_of(c, r.tok);
            if file.is_empty() {
                continue;
            }
            let cc = crate::g_design_metrics::routine_metrics(c, r.body).cyclomatic;
            let f = files.entry(file).or_insert(Stats { complexity: 0, worst: 0, at: None, churn: 0, log_size: 0 });
            // Each module instance lists its routines again.
            f.complexity += cc * r.times as i64;
            if f.at.is_none() || cc > f.worst {
                f.worst = cc;
                f.at = Some((k, r.tok, r.name.clone()));
            }
        }
    }
    let mut ranked: Vec<(i64, String)> = Vec::new();
    for (file, f) in files.iter_mut() {
        let Some(log) = log_for(file, MAX_COMMITS) else { continue };
        f.churn = log.files.get(file).map_or(0, Vec::len);
        f.log_size = log.sizes.len();
        if f.churn < min_commits || f.complexity < min_complexity {
            continue;
        }
        ranked.push((f.churn as i64 * f.complexity, file.clone()));
    }
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (i, (_, file)) in ranked.iter().take(top_files).enumerate() {
        let f = &files[file];
        let Some((k, tok, name)) = &f.at else { continue };
        out.add(
            "SV-EVO-001",
            "Hotspot",
            Severity::Info,
            format!(
                "Hotspot #{}: '{}' changed in {} of the last {} commits and its routines add up to cyclomatic \
                 complexity {}; '{}' ({}) is the most complex. Complex code that keeps changing is where defects \
                 collect; simplify or split it before adding to it.",
                i + 1,
                base_name(file),
                f.churn,
                f.log_size,
                f.complexity,
                name,
                f.worst
            ),
            csts[*k].loc(*tok),
        );
    }
}

/// SV-EVO-002: two files in different packages that keep changing together.
fn change_coupling(_: &dv_model::Design, csts: &[Cst], out: &mut Sink) {
    if !HISTORY.load(Ordering::Relaxed) {
        return;
    }
    let (min_shared, min_degree, max_files) = (3usize, 50usize, 30usize);
    struct Info {
        packages: BTreeSet<String>,
        /// The file's first class in a package: its tree and name token.
        first: (usize, u32),
    }
    let mut files: BTreeMap<String, Info> = BTreeMap::new();
    for (k, c) in csts.iter().enumerate() {
        let classes = crate::g_design_class::Classes::new(c);
        for i in classes.user() {
            let cls = &classes.list[i];
            let Some(p) = c.parent(cls.node).filter(|&p| c.kind(p) == PackageDecl) else { continue };
            let pkg = crate::g_c_support::decl_name(c, p);
            let file = file_of(c, cls.tok);
            if file.is_empty() {
                continue;
            }
            files.entry(file).or_insert(Info { packages: BTreeSet::new(), first: (k, cls.tok) }).packages.insert(pkg);
        }
    }
    // Commits per file, leaving out sweeping commits (reformatting, renames,
    // licence headers) that touch many files at once.
    let mut revs: Vec<(&String, Arc<Log>, Vec<u32>)> = Vec::new();
    for file in files.keys() {
        let Some(log) = log_for(file, MAX_COMMITS) else { continue };
        let Some(cs) = log.files.get(file) else { continue };
        let kept: Vec<u32> = cs.iter().copied().filter(|&c| log.sizes[c as usize] <= max_files).collect();
        if kept.len() >= min_shared {
            revs.push((file, log.clone(), kept));
        }
    }
    for i in 0..revs.len() {
        for j in i + 1..revs.len() {
            let (fa, la, ra) = &revs[i];
            let (fb, lb, rb) = &revs[j];
            if !Arc::ptr_eq(la, lb) {
                continue;
            }
            let (pa, pb) = (&files[*fa].packages, &files[*fb].packages);
            if pa.intersection(pb).next().is_some() {
                continue;
            }
            // Both lists are ascending (commit indices, newest first).
            let shared = ra.iter().filter(|c| rb.binary_search(c).is_ok()).count();
            let total = ra.len() + rb.len();
            if shared < min_shared || shared * 200 < min_degree * total {
                continue;
            }
            let (k, tok) = files[*fb].first;
            out.add(
                "SV-EVO-002",
                "ChangeCoupling",
                Severity::Info,
                format!(
                    "'{}' (package {}) changed together with '{}' (package {}) in {} commits, {}% of the commits \
                     touching either. Packages that keep changing together share knowledge their imports do not \
                     show; move it to one place or make the dependency explicit.",
                    base_name(fb),
                    pb.iter().next().map_or("", String::as_str),
                    base_name(fa),
                    pa.iter().next().map_or("", String::as_str),
                    shared,
                    shared * 200 / total
                ),
                csts[k].loc(tok),
            );
        }
    }
}
