//! Waivers and the baseline, applied after the project configuration as
//! uvmcheck's WaiverEngine applies them: the config's waivers, then a
//! waiver file's (`--waivers FILE`, else `.uvmcheck/waivers.json` under the
//! project root), the first match deciding (a waiver with a `severity`
//! keeps the finding, re-graded); then the baseline (`--baseline FILE`,
//! else `.uvmcheck/baseline.json`), whose entries are fingerprints of known
//! findings.
//!
//! A fingerprint is FNV-1a 64 of the rule, the file relative to the project
//! root, the flagged line's text with whitespace collapsed, and the
//! occurrence index among findings with those three equal, so it survives
//! edits that move lines. Version 1.0 baselines hold `RULE@file:line`.
//! `--update-baseline FILE` writes the current findings as a version 2.0
//! baseline.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::project::Waiver;
use crate::Finding;

/// The waivers and baseline of a run.
#[derive(Default)]
pub struct Suppress {
    waivers: Vec<Waiver>,
    baseline: HashSet<String>,
    legacy: HashSet<String>,
    root: PathBuf,
    /// Notes for stderr (a file that could not be read ...).
    pub warnings: Vec<String>,
    /// What [`Suppress::apply`] matched, for [`Suppress::hygiene`].
    stats: std::sync::Mutex<Stats>,
}

#[derive(Default)]
struct Stats {
    group_matches: HashMap<usize, usize>,
    matched_fingerprints: HashSet<String>,
    by_rule: HashMap<String, usize>,
    by_owner: HashMap<String, usize>,
    regraded: usize,
}

/// Waiver and baseline hygiene (`--waiver-report`), as uvmcheck's
/// WaiverHygiene.
#[derive(Default, Debug)]
pub struct Hygiene {
    /// Waivers written (config entries and waiver file entries).
    pub written: usize,
    pub without_owner: usize,
    /// Labels of waivers that matched nothing.
    pub unused: Vec<String>,
    /// Waiver comments in the sources.
    pub inline_comments: usize,
    /// Waiver comments that waived nothing: (file, line, text).
    pub unused_inline: Vec<crate::waivers::Unused>,
    pub baseline_entries: usize,
    pub stale_baseline: usize,
    /// Waived findings per rule and per owner, most first.
    pub by_rule: Vec<(String, usize)>,
    pub by_owner: Vec<(String, usize)>,
}

fn collapse(s: &str) -> String {
    let mut out = String::new();
    let mut space = false;
    for ch in s.chars() {
        if matches!(ch, ' ' | '\t' | '\r' | '\n') {
            space = !out.is_empty();
            continue;
        }
        if space {
            out.push(' ');
        }
        space = false;
        out.push(ch);
    }
    out
}

fn fnv(s: &str) -> String {
    let mut h: u64 = 1_469_598_103_934_665_603;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(1_099_511_628_211);
    }
    format!("{h:016x}")
}

impl Suppress {
    /// `config`'s waivers and root (else the working folder), the waiver file
    /// and the baseline. With `no_config`, only files named explicitly.
    pub fn load(
        config: Option<&crate::project::Config>,
        waivers: Option<&str>,
        baseline: Option<&str>,
        no_config: bool,
    ) -> Result<Suppress, String> {
        let root = config
            .map(|c| c.root.clone())
            .filter(|r| !r.as_os_str().is_empty())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        Self::load_in(&root, config, waivers, baseline, no_config)
    }

    /// [`Suppress::load`] with the project folder given (a workspace that is
    /// not the working folder).
    pub fn load_in(
        root: &Path,
        config: Option<&crate::project::Config>,
        waivers: Option<&str>,
        baseline: Option<&str>,
        no_config: bool,
    ) -> Result<Suppress, String> {
        let root = root.to_path_buf();
        let mut s = Suppress { root: root.clone(), ..Suppress::default() };
        if let Some(c) = config {
            s.waivers.extend(c.waivers().iter().map(|w| Waiver {
                rule: w.rule.clone(),
                path: w.path.clone(),
                contains: w.contains.clone(),
                line: w.line,
                message: w.message.clone(),
                message_re: w.message_re.clone(),
                severity: w.severity,
                label: w.label.clone(),
                owner: w.owner.clone(),
                group: w.group,
            }));
        }
        // Waiver file entries are groups of their own, after the config's.
        let first_group = s.waivers.iter().map(|w| w.group + 1).max().unwrap_or(0);
        let dot = root.join(".uvmcheck");
        let pick = |given: Option<&str>, name: &str| -> Option<PathBuf> {
            match given {
                Some(g) => Some(PathBuf::from(g)),
                None if !no_config => Some(dot.join(name)).filter(|p| p.is_file()),
                None => None,
            }
        };
        if let Some(w) = pick(waivers, "waivers.json") {
            match crate::project::load_waiver_file(&w) {
                Ok(list) => s.waivers.extend(list.into_iter().map(|w| Waiver { group: first_group + w.group, ..w })),
                // A pattern that does not compile stops the run, as in uvmcheck;
                // a file that does not load is only a warning.
                Err(e) if e.contains("invalid message regex") => return Err(e),
                Err(e) => s.warnings.push(format!("Could not load waiver file: {e}")),
            }
        }
        if let Some(b) = pick(baseline, "baseline.json") {
            match std::fs::read_to_string(&b).map_err(|e| e.to_string()).and_then(|t| crate::json::parse(&t)) {
                Ok(j) => {
                    let legacy = j.get("version").as_str() == Some("1.0");
                    if !matches!(j.get("baseline"), crate::json::Json::Arr(_)) {
                        s.warnings.push(format!(
                            "Could not load baseline file: {}: expected an object with a \"baseline\" list",
                            b.display()
                        ));
                    }
                    for e in j.get("baseline").as_arr() {
                        if let Some(fp) = e.get("fingerprint").as_str().filter(|f| !f.is_empty()) {
                            if legacy { &mut s.legacy } else { &mut s.baseline }.insert(fp.to_string());
                        }
                    }
                }
                Err(e) => s.warnings.push(format!("Could not load baseline file: {}: {e}", b.display())),
            }
        }
        Ok(s)
    }

    /// The file as a baseline names it: relative to the project root when
    /// under it, else absolute.
    fn baseline_path(&self, file: &str) -> String {
        let abs = crate::project::normalized(file);
        let root = crate::project::normalized(&self.root.to_string_lossy());
        match abs.strip_prefix(&format!("{root}/")) {
            Some(rel) => rel.to_string(),
            None => abs,
        }
    }

    /// Each finding's fingerprint.
    pub fn fingerprints(&self, found: &[Finding], files: &[String]) -> Vec<String> {
        let mut texts: HashMap<String, Vec<String>> = HashMap::new();
        let mut order: Vec<usize> = (0..found.len()).collect();
        order.sort_by_key(|&i| {
            let (f, l, c) = found[i].position_in(files);
            (f.to_string(), l, c)
        });
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut out = vec![String::new(); found.len()];
        for i in order {
            let (file, line, _) = found[i].position_in(files);
            let lines = texts.entry(file.to_string()).or_insert_with(|| {
                svlang::syntax::overlay::read(std::path::Path::new(file))
                    .map(|t| t.lines().map(str::to_string).collect())
                    .unwrap_or_default()
            });
            let text = if line == 0 { "" } else { lines.get(line as usize - 1).map_or("", String::as_str) };
            let key = format!("{}\n{}\n{}", found[i].rule, self.baseline_path(file), collapse(text));
            let n = seen.entry(key.clone()).or_insert(0);
            out[i] = fnv(&format!("{key}\n{n}"));
            *n += 1;
        }
        out
    }

    /// Drops waived and baselined findings and re-grades those a waiver with
    /// a severity names; returns how many were dropped.
    pub fn apply(&self, found: &mut Vec<Finding>, files: &[String]) -> usize {
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        // Findings an inline comment waived: a waiver that matches one too is
        // used, as uvmcheck evaluates both.
        let inline = crate::waivers::USED.lock().ok().and_then(|u| u.as_ref().map(|u| u.waived.clone()));
        for (file, line, rule, message) in inline.unwrap_or_default() {
            let path = crate::project::normalized(&file);
            if let Some(w) = self.waivers.iter().find(|w| w.matches(rule, &path, &file, line, &message)) {
                *stats.group_matches.entry(w.group).or_default() += 1;
            }
            *stats.by_owner.entry("(inline comment)".into()).or_default() += 1;
        }
        let use_baseline = !self.baseline.is_empty() || !self.legacy.is_empty();
        let fps = if use_baseline { self.fingerprints(found, files) } else { Vec::new() };
        let mut norm: HashMap<String, String> = HashMap::new();
        let mut keep = Vec::with_capacity(found.len());
        let mut waived = 0;
        for (i, mut f) in std::mem::take(found).into_iter().enumerate() {
            let (file, line, _) = f.position_in(files);
            let path = norm.entry(file.to_string()).or_insert_with(|| crate::project::normalized(file)).clone();
            let ext = self.waivers.iter().find(|w| w.matches(f.rule, &path, file, line, &f.message));
            let mut dropped = false;
            if let Some(w) = ext {
                *stats.group_matches.entry(w.group).or_default() += 1;
            }
            match ext {
                Some(w) if w.severity.is_some() => {
                    f.severity = w.severity.unwrap_or(f.severity);
                    f.message.push_str(&format!(" [re-graded by {}]", w.label));
                    stats.regraded += 1;
                }
                Some(w) => {
                    dropped = true;
                    *stats.by_rule.entry(f.rule.to_string()).or_default() += 1;
                    let owner = if w.owner.is_empty() { "(no owner)" } else { w.owner.as_str() };
                    *stats.by_owner.entry(owner.to_string()).or_default() += 1;
                }
                None => {}
            }
            if !dropped && use_baseline {
                let base = file.rsplit(['/', '\\']).next().unwrap_or(file);
                let legacy = format!("{}@{base}:{line}", f.rule);
                if self.baseline.contains(&fps[i]) {
                    stats.matched_fingerprints.insert(fps[i].clone());
                    dropped = true;
                } else if self.legacy.contains(&legacy) {
                    stats.matched_fingerprints.insert(legacy);
                    dropped = true;
                }
            }
            if dropped {
                waived += 1;
            } else {
                keep.push(f);
            }
        }
        *found = keep;
        waived
    }

    /// How many findings waivers with a severity re-graded.
    pub fn regraded(&self) -> usize {
        self.stats.lock().map_or(0, |s| s.regraded)
    }

    /// Baseline entries no finding matched (after [`Suppress::apply`]).
    pub fn stale_baseline(&self) -> usize {
        let total = self.baseline.len() + self.legacy.len();
        total.saturating_sub(self.stats.lock().map_or(0, |s| s.matched_fingerprints.len()))
    }

    /// Waiver and baseline hygiene after [`Suppress::apply`]: waivers and
    /// waiver comments in `files` (user files) that matched nothing, where
    /// the comment names a rule in `active` or `*`.
    pub fn hygiene(&self, files: &[String], active: &HashSet<&str>) -> Hygiene {
        let stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        let mut h = Hygiene::default();
        let mut seen = HashSet::new();
        for w in &self.waivers {
            if !seen.insert(w.group) {
                continue;
            }
            h.written += 1;
            if w.owner.is_empty() {
                h.without_owner += 1;
            }
            if stats.group_matches.get(&w.group).copied().unwrap_or(0) == 0 {
                h.unused.push(w.label.clone());
            }
        }
        let mut by_rule: HashMap<String, usize> = stats.by_rule.clone();
        if let Ok(u) = crate::waivers::USED.lock() {
            for (r, n) in u.as_ref().map(|u| u.by_rule.clone()).unwrap_or_default() {
                *by_rule.entry(r).or_default() += n;
            }
        }
        let most_first = |m: HashMap<String, usize>| {
            let mut v: Vec<(String, usize)> = m.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            v
        };
        h.by_rule = most_first(by_rule);
        h.by_owner = most_first(stats.by_owner.clone());
        drop(stats);
        (h.inline_comments, h.unused_inline) = crate::waivers::unused_comments(files, active);
        h.baseline_entries = self.baseline.len() + self.legacy.len();
        h.stale_baseline = self.stale_baseline();
        h
    }

    /// Writes `found` as a version 2.0 baseline.
    pub fn save_baseline(&self, path: &Path, found: &[Finding], files: &[String]) -> Result<(), String> {
        let fps = self.fingerprints(found, files);
        let mut order: Vec<usize> = (0..found.len()).collect();
        order.sort_by(|&a, &b| {
            let (fa, la, ca) = found[a].position_in(files);
            let (fb, lb, cb) = found[b].position_in(files);
            (fa, la, ca, found[a].rule).cmp(&(fb, lb, cb, found[b].rule))
        });
        let esc = |s: &str| crate::json::Json::Str(s.to_string()).to_string();
        let mut out = String::from(
            "{\n  \"version\": \"2.0\",\n  \"note\": \"Known findings, matched by fingerprint (rule, file relative to the project root, text of the flagged line). Line numbers are for reading only.\",\n  \"baseline\": [\n",
        );
        for (k, &i) in order.iter().enumerate() {
            let (file, line, _) = found[i].position_in(files);
            out.push_str(&format!(
                "    {{\"rule\": {}, \"file\": {}, \"line\": {line}, \"fingerprint\": \"{}\",\n     \"message\": {}}}{}\n",
                esc(found[i].rule),
                esc(&self.baseline_path(file)),
                fps[i],
                esc(&found[i].message),
                if k + 1 < order.len() { "," } else { "" }
            ));
        }
        out.push_str("  ]\n}\n");
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
    }
}
