//! The project configuration, read and applied as uvmcheck does
//! (docs/guide/project_config.md): which files are library or excluded code,
//! per-rule `enabled`, `severity`, `exclude_paths` and `only_paths`,
//! waivers (the config's and `.uvmcheck/waivers.json`), and the compile
//! settings a config may carry (`sources`, `filelists`, `include_dirs`,
//! `defines`).
//!
//! A Python config (`.uvmcheck/config.py`) is evaluated by the uvmcheck
//! Python SDK (`python3 -m uvmcheck.config_runner`), as the C++ engine does,
//! into the same JSON a `.json` config holds. Rule options, `style` and
//! `naming` blocks are not applied by the native rules yet: [`Config::unapplied`]
//! names them so the caller can say so.

use std::path::{Path, PathBuf};

use crate::json::Json;
use crate::{Finding, Severity};

/// Config file names, in the order uvmcheck looks for them in each folder.
const NAMES: [&str; 6] = [
    ".uvmcheck/config.py",
    ".uvmcheck/config.json",
    "uvmcheck_config.py",
    ".uvmcheck.py",
    ".uvmcheck.json",
    "uvmcheck.json",
];

/// A path pattern, anchored as uvmcheck's PathPattern: `$VAR` and `~`
/// expanded (unset: never matches), a bare name matches in any folder,
/// relative patterns are under the config's folder, `**` spans folders and a
/// pattern naming a folder covers everything under it.
#[derive(Clone, Debug)]
struct Pattern(Option<String>);

impl Pattern {
    fn new(pat: &str, base: &Path) -> Pattern {
        let Some(mut e) = expand_env(pat) else { return Pattern(None) };
        e = e.replace('\\', "/");
        while e.len() > 1 && e.ends_with('/') {
            e.pop();
        }
        if e.is_empty() {
            return Pattern(None);
        }
        if !e.contains('/') {
            return Pattern(Some(format!("**/{e}")));
        }
        let p = PathBuf::from(&e);
        let p = if p.is_relative() { base.join(p) } else { p };
        // Resolve symlinks in the wildcard-free prefix, as file paths are.
        let mut prefix = PathBuf::new();
        let mut rest = PathBuf::new();
        let mut wild = false;
        for seg in normalize(&p).iter() {
            wild |= seg.to_string_lossy().contains(['*', '?', '[']);
            if wild { &mut rest } else { &mut prefix }.push(seg);
        }
        let prefix = std::fs::canonicalize(&prefix).unwrap_or(prefix);
        let mut g = prefix.join(rest).to_string_lossy().replace('\\', "/");
        while g.len() > 1 && g.ends_with('/') {
            g.pop();
        }
        Pattern(Some(g))
    }

    fn matches(&self, path: &str) -> bool {
        let Some(g) = &self.0 else { return false };
        let gs: Vec<&str> = g.split('/').collect();
        let ps: Vec<&str> = path.split('/').collect();
        match_segments(&gs, 0, &ps, 0)
    }
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

fn expand_env(s: &str) -> Option<String> {
    let mut out = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    if s.starts_with('~') && (s.len() == 1 || matches!(b[1], b'/' | b'\\')) {
        out.push_str(&std::env::var("HOME").ok()?);
        i = 1;
    }
    while i < b.len() {
        if b[i] != b'$' {
            let ch = s[i..].chars().next()?;
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        let braced = b.get(i + 1) == Some(&b'{');
        let start = if braced { i + 2 } else { i + 1 };
        let end = if braced {
            start + s[start..].find('}')?
        } else {
            start + s[start..].bytes().take_while(|c| c.is_ascii_alphanumeric() || *c == b'_').count()
        };
        if end == start {
            out.push('$');
            i += 1;
            continue;
        }
        let v = std::env::var(&s[start..end]).ok().filter(|v| !v.is_empty())?;
        out.push_str(&v);
        i = if braced { end + 1 } else { end };
    }
    Some(out)
}

fn segment_match(p: &str, s: &str) -> bool {
    let (p, s) = (p.as_bytes(), s.as_bytes());
    let (mut pi, mut si) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while si < s.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            pi += 1;
            mark = si;
        } else if let Some(sp) = star {
            pi = sp + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

fn match_segments(g: &[&str], mut gi: usize, p: &[&str], mut pi: usize) -> bool {
    while gi < g.len() {
        if g[gi] == "**" {
            while gi + 1 < g.len() && g[gi + 1] == "**" {
                gi += 1;
            }
            if gi + 1 == g.len() {
                return true;
            }
            return (pi..=p.len()).any(|k| match_segments(g, gi + 1, p, k));
        }
        if pi >= p.len() || !segment_match(g[gi], p[pi]) {
            return false;
        }
        gi += 1;
        pi += 1;
    }
    true
}

/// A file path as patterns compare it: absolute, symlinks resolved.
fn normalized(path: &str) -> String {
    let p = Path::new(path);
    let abs = if p.is_relative() {
        std::env::current_dir().map(|d| d.join(p)).unwrap_or(p.to_path_buf())
    } else {
        p.to_path_buf()
    };
    let r = std::fs::canonicalize(&abs).unwrap_or_else(|_| normalize(&abs));
    let mut s = r.to_string_lossy().replace('\\', "/");
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

#[derive(Default, Debug)]
struct RuleSettings {
    enabled: Option<bool>,
    severity: Option<Severity>,
    exclude: Vec<Pattern>,
    only: Vec<Pattern>,
    options: bool,
}

#[derive(Debug)]
struct Waiver {
    rule: String,
    path: Option<Pattern>,
    line: u32,
    message: String,
    severity: Option<Severity>,
}

/// A loaded project configuration.
#[derive(Default, Debug)]
pub struct Config {
    /// The file it was read from.
    pub file: PathBuf,
    library: Vec<Pattern>,
    exclude: Vec<Pattern>,
    report_library_macros: bool,
    rules: Vec<(String, RuleSettings)>,
    waivers: Vec<Waiver>,
    /// Compile arguments from `sources`, `filelists`, `include_dirs` and
    /// `defines`.
    pub compile_args: Vec<String>,
    /// Warnings while reading it (unset variables, expired waivers ...).
    pub warnings: Vec<String>,
    /// Settings the native rules do not apply yet.
    pub unapplied: Vec<String>,
}

fn severity(s: &str) -> Option<Severity> {
    match s.to_ascii_uppercase().as_str() {
        "ERROR" => Some(Severity::Error),
        "WARNING" => Some(Severity::Warning),
        "INFO" => Some(Severity::Info),
        _ => None,
    }
}

fn strings(v: &Json) -> Vec<String> {
    match v {
        Json::Str(s) if !s.is_empty() => vec![s.clone()],
        Json::Arr(a) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    }
}

/// The config to use: `explicit` (`--config`), else the first of [`NAMES`]
/// in the working folder or a parent, else from the first source's folder
/// up.
pub fn find(explicit: Option<&str>, first_source: Option<&str>) -> Option<PathBuf> {
    if let Some(e) = explicit {
        return Some(PathBuf::from(e));
    }
    let search = |start: PathBuf| -> Option<PathBuf> {
        let mut d = Some(start.as_path());
        while let Some(dir) = d {
            for n in NAMES {
                let f = dir.join(n);
                if f.is_file() {
                    return Some(f);
                }
            }
            d = dir.parent();
        }
        None
    };
    let cwd = std::env::current_dir().ok()?;
    search(cwd.clone()).or_else(|| {
        let src = Path::new(first_source?);
        let abs = if src.is_relative() { cwd.join(src) } else { src.to_path_buf() };
        search(abs.parent()?.to_path_buf())
    })
}

/// The uvmcheck Python SDK next to this binary (`<repo>/python` from
/// `<repo>/target/release`), else `$UVMCHECK_PYTHON_SDK`.
fn python_sdk() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("UVMCHECK_PYTHON_SDK") {
        return Some(PathBuf::from(p));
    }
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.ancestors().skip(1).map(|a| a.join("python")).find(|p| p.join("uvmcheck").join("config_runner.py").is_file())
}

fn load_json(file: &Path) -> Result<Json, String> {
    let text = if file.extension().is_some_and(|e| e == "py") {
        let out = std::env::temp_dir().join(format!("dv_lint_config_{}.json", std::process::id()));
        let python = std::env::var("UVMCHECK_PYTHON").unwrap_or_else(|_| "python3".into());
        let mut cmd = std::process::Command::new(&python);
        cmd.args(["-m", "uvmcheck.config_runner", "--config"]).arg(file).arg("--out").arg(&out);
        if let Some(sdk) = python_sdk() {
            let mut pp = sdk.into_os_string();
            if let Some(old) = std::env::var_os("PYTHONPATH") {
                pp.push(":");
                pp.push(old);
            }
            cmd.env("PYTHONPATH", pp);
        }
        let res = cmd.output().map_err(|e| format!("failed to start {python} to evaluate {}: {e}", file.display()))?;
        let text = std::fs::read_to_string(&out);
        let _ = std::fs::remove_file(&out);
        match text {
            Ok(t) if res.status.success() => t,
            _ => {
                let msg = String::from_utf8_lossy(&res.stdout).to_string() + &String::from_utf8_lossy(&res.stderr);
                return Err(format!("Python config {} failed to load:\n{}", file.display(), msg.trim_end()));
            }
        }
    } else {
        std::fs::read_to_string(file).map_err(|e| format!("cannot read {}: {e}", file.display()))?
    };
    crate::json::parse(&text).map_err(|e| format!("{}: {e}", file.display()))
}

impl Config {
    /// Reads `file` and, next to its project root, `.uvmcheck/waivers.json`.
    pub fn load(file: &Path) -> Result<Config, String> {
        let root = load_json(file)?;
        let base = file.parent().map(Path::to_path_buf).unwrap_or_default();
        // `.uvmcheck/config.py` belongs to the folder above `.uvmcheck`.
        let project = if base.file_name().is_some_and(|n| n == ".uvmcheck") {
            base.parent().map(Path::to_path_buf).unwrap_or_default()
        } else {
            base.clone()
        };
        let base = if base.file_name().is_some_and(|n| n == ".uvmcheck") { project.clone() } else { base };
        let mut c = Config { file: file.to_path_buf(), ..Config::default() };
        let pats = |c: &mut Config, v: &Json, what: &str| -> Vec<Pattern> {
            strings(v)
                .iter()
                .filter_map(|s| {
                    let p = Pattern::new(s, &base);
                    if p.0.is_none() {
                        c.warnings.push(format!("{what}: '{s}' uses an unset environment variable; pattern ignored"));
                        None
                    } else {
                        Some(p)
                    }
                })
                .collect()
        };
        c.library = pats(&mut c, root.get("library_paths"), "library_paths");
        c.exclude = pats(&mut c, root.get("exclude_paths"), "exclude_paths");
        c.report_library_macros = root.get("report_library_macro_expansions").as_bool().unwrap_or(false);
        if let Json::Obj(rules) = root.get("rules") {
            for (key, entry) in rules {
                let mut rs = RuleSettings::default();
                match entry {
                    Json::Bool(b) => rs.enabled = Some(*b),
                    Json::Str(s) if matches!(s.to_ascii_lowercase().as_str(), "off" | "disabled") => {
                        rs.enabled = Some(false)
                    }
                    Json::Str(s) => rs.severity = severity(s),
                    Json::Obj(_) => {
                        rs.enabled = entry.get("enabled").as_bool();
                        rs.severity = entry.get("severity").as_str().and_then(severity);
                        rs.exclude = pats(&mut c, entry.get("exclude_paths"), &format!("rules.{key}.exclude_paths"));
                        rs.only = pats(&mut c, entry.get("only_paths"), &format!("rules.{key}.only_paths"));
                        rs.options = matches!(entry.get("options"), Json::Obj(o) if !o.is_empty());
                        if rs.options {
                            c.unapplied.push(format!("rules.{key}.options"));
                        }
                    }
                    _ => return Err(format!("rules.{key}: expected an object, true/false, or \"off\"")),
                }
                c.rules.push((key.clone(), rs));
            }
        }
        for key in ["style", "naming"] {
            if matches!(root.get(key), Json::Obj(o) if !o.is_empty()) {
                c.unapplied.push(key.to_string());
            }
        }
        c.add_waivers(root.get("waivers"), &base)?;
        let wfile = project.join(".uvmcheck").join("waivers.json");
        if wfile.is_file() {
            let w = load_json(&wfile)?;
            c.add_waivers(w.get("waivers"), &project)?;
        }
        // Compile settings.
        for s in strings(root.get("filelists")) {
            c.compile_args.extend(["-f".to_string(), anchor(&s, &base)]);
        }
        for s in strings(root.get("include_dirs")) {
            c.compile_args.push(format!("+incdir+{}", anchor(&s, &base)));
        }
        for s in strings(root.get("defines")) {
            c.compile_args.push(format!("+define+{s}"));
        }
        for s in strings(root.get("sources")) {
            c.compile_args.push(anchor(&s, &base));
        }
        Ok(c)
    }

    fn add_waivers(&mut self, list: &Json, base: &Path) -> Result<(), String> {
        let today = today();
        for w in list.as_arr() {
            let mut rules = strings(w.get("rules"));
            rules.extend(strings(w.get("rule")));
            if rules.is_empty() {
                return Err("every waiver needs \"rules\" (a rule ID, name, or \"*\")".into());
            }
            if w.get("reason").as_str().is_none_or(str::is_empty) {
                return Err(format!("waiver for {} needs a \"reason\"", rules[0]));
            }
            if let Some(e) = w.get("expires").as_str() {
                if e < today.as_str() {
                    self.warnings.push(format!("waiver for {} expired on {e}; it no longer applies", rules[0]));
                    continue;
                }
            }
            let mut paths = strings(w.get("paths"));
            paths.extend(strings(w.get("path")));
            if paths.is_empty() {
                paths.extend(strings(w.get("file")));
            }
            let globs: Vec<Pattern> = paths.iter().map(|p| Pattern::new(p, base)).filter(|p| p.0.is_some()).collect();
            if !paths.is_empty() && globs.is_empty() {
                self.warnings.push(format!("waiver for {}: its paths use unset environment variables", rules[0]));
                continue;
            }
            let line = w.get("line").as_u32().unwrap_or(0);
            let message = w.get("message").as_str().unwrap_or("").to_string();
            let sev = w.get("severity").as_str().and_then(severity);
            let globs: Vec<Option<Pattern>> =
                if globs.is_empty() { vec![None] } else { globs.into_iter().map(Some).collect() };
            for r in &rules {
                for g in &globs {
                    self.waivers.push(Waiver {
                        rule: r.clone(),
                        path: g.clone(),
                        line,
                        message: message.clone(),
                        severity: sev,
                    });
                }
            }
        }
        Ok(())
    }

    fn rule(&self, id: &str, name: &str) -> Option<&RuleSettings> {
        self.rules.iter().find(|(k, _)| k == id || k == name).map(|(_, r)| r)
    }

    /// The rule runs under this config (`enabled` not false).
    pub fn enabled(&self, id: &str, name: &str) -> bool {
        self.rule(id, name).and_then(|r| r.enabled) != Some(false)
    }

    /// Applies the config to `found`: drops findings in library and
    /// excluded files and those waived, regrades severities. Returns how
    /// many were waived.
    pub fn apply(&self, found: &mut Vec<Finding>, files: &[String]) -> usize {
        let all = crate::rules();
        let names: std::collections::HashMap<&str, &str> = all
            .iter()
            .map(|r| (r.id, r.name))
            .chain(all.iter().flat_map(|r| crate::sub_rules(r.id).iter().copied()))
            .collect();
        let mut cache: std::collections::HashMap<u32, String> = Default::default();
        let mut norm = |f: u32| -> String {
            cache
                .entry(f)
                .or_insert_with(|| normalized(files.get(f as usize).map(String::as_str).unwrap_or("")))
                .clone()
        };
        let mut waived = 0;
        found.retain_mut(|x| {
            let name = names.get(x.rule).copied().unwrap_or(x.name);
            if !self.enabled(x.rule, name) {
                return false;
            }
            // Where it is reported, and (in a macro) where the macro body is.
            let (file, origin) = match x.loc.expansion {
                Some((f, _)) => (f, Some(x.loc.file)),
                None => (x.loc.file, None),
            };
            let path = norm(file);
            if self.library.iter().any(|p| p.matches(&path)) || self.exclude.iter().any(|p| p.matches(&path)) {
                return false;
            }
            if let Some(rs) = self.rule(x.rule, name) {
                if (!rs.only.is_empty() && !rs.only.iter().any(|p| p.matches(&path)))
                    || rs.exclude.iter().any(|p| p.matches(&path))
                {
                    return false;
                }
            }
            if let Some(o) = origin.filter(|&o| o != file) {
                if !self.report_library_macros && self.library.iter().any(|p| p.matches(&norm(o))) {
                    return false;
                }
            }
            if let Some(sev) = self.rule(x.rule, name).and_then(|r| r.severity) {
                x.severity = sev;
            }
            let line = x.position_in(files).1;
            for w in &self.waivers {
                let rule_ok = w.rule == "*" || w.rule == x.rule || w.rule == name;
                let path_ok = w.path.as_ref().is_none_or(|p| p.matches(&path));
                let line_ok = w.line == 0 || w.line == line;
                // No regex engine here: a message pattern matches as text.
                let msg_ok = w.message.is_empty() || x.message.contains(&w.message);
                if rule_ok && path_ok && line_ok && msg_ok {
                    match w.severity {
                        Some(s) => x.severity = s,
                        None => {
                            waived += 1;
                            return false;
                        }
                    }
                }
            }
            true
        });
        waived
    }
}

fn anchor(s: &str, base: &Path) -> String {
    let e = expand_env(s).unwrap_or_else(|| s.to_string());
    let p = Path::new(&e);
    if p.is_relative() {
        base.join(p).to_string_lossy().into_owned()
    } else {
        e
    }
}

/// Today as YYYY-MM-DD (UTC), for waiver expiry.
fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let days = secs.div_euclid(86_400);
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_as_uvmcheck_matches_them() {
        let p = Pattern(Some("**/legacy/**".into()));
        assert!(p.matches("/a/legacy/b.sv"));
        assert!(!p.matches("/a/new/b.sv"));
        let bare = Pattern::new("*loop_test.sv*", Path::new("/x"));
        assert!(bare.matches("/proj/tb/stress_loop_test.sv"));
        let dir = Pattern(Some("/proj/vip".into()));
        assert!(dir.matches("/proj/vip/agent/a.sv"), "a folder covers what is under it");
        assert!(Pattern::new("$DV_LINT_SURELY_UNSET_VAR/x", Path::new("/")).0.is_none());
    }

    #[test]
    fn today_is_a_date() {
        let t = today();
        assert_eq!(t.len(), 10);
        assert!(t.starts_with("20"));
    }
}
