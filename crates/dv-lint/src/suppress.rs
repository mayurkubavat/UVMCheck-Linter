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
        let mut s = Suppress { root: root.clone(), ..Suppress::default() };
        if let Some(c) = config {
            s.waivers.extend(c.waivers().iter().map(|w| Waiver {
                rule: w.rule.clone(),
                path: w.path.clone(),
                contains: w.contains.clone(),
                line: w.line,
                message: w.message.clone(),
                severity: w.severity,
                label: w.label.clone(),
            }));
        }
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
                Ok(list) => s.waivers.extend(list),
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
                std::fs::read_to_string(file).map(|t| t.lines().map(str::to_string).collect()).unwrap_or_default()
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
            match ext {
                Some(w) if w.severity.is_some() => {
                    f.severity = w.severity.unwrap_or(f.severity);
                    f.message.push_str(&format!(" [re-graded by {}]", w.label));
                }
                Some(_) => dropped = true,
                None => {}
            }
            if !dropped && use_baseline {
                let base = file.rsplit(['/', '\\']).next().unwrap_or(file);
                dropped = self.baseline.contains(&fps[i]) || self.legacy.contains(&format!("{}@{base}:{line}", f.rule));
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
