//! The project configuration, read and applied as uvmcheck does
//! (docs/guide/project_config.md): which files are library or excluded code,
//! per-rule `enabled`, `severity`, `exclude_paths` and `only_paths`,
//! waivers (the config's and `.uvmcheck/waivers.json`), and the compile
//! settings a config may carry (`sources`, `filelists`, `include_dirs`,
//! `defines`).
//!
//! A Python config (`.uvmcheck/config.py`) is evaluated by the uvmcheck
//! Python SDK (`python3 -m uvmcheck.config_runner`), as the C++ engine does,
//! into the same JSON a `.json` config holds. Integer and boolean rule
//! options and the `style` and `naming` blocks go to [`crate::options`];
//! what the native rules cannot apply (other option types, a header or
//! naming pattern that is a regex) is named in [`Config::unapplied`] so the
//! caller can say so.

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
pub(crate) struct Pattern(Option<String>);

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

    pub(crate) fn matches(&self, path: &str) -> bool {
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
pub(crate) fn normalized(path: &str) -> String {
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
}

/// A waiver from the config (`waivers`) or a waiver file (`--waivers`,
/// `.uvmcheck/waivers.json`), as uvmcheck's ExternalWaiver.
#[derive(Debug)]
pub(crate) struct Waiver {
    pub(crate) rule: String,
    /// Config waivers: a path pattern. Waiver files: text the path must
    /// contain (leading and trailing `*` dropped), as uvmcheck reads them.
    pub(crate) path: Option<Pattern>,
    pub(crate) contains: Option<String>,
    pub(crate) line: u32,
    /// The `message` pattern as written, and compiled (none when empty).
    pub(crate) message: String,
    pub(crate) message_re: Option<crate::pattern::Pattern>,
    pub(crate) severity: Option<Severity>,
    /// What a re-graded finding's message names (`waiver #1 (SV-STY-002)`).
    pub(crate) label: String,
    /// Who owns it (`owner`), for the waiver report.
    pub(crate) owner: String,
    /// The waiver as written: one config entry expands to a waiver per rule
    /// and path, all in one group (the waiver report counts groups).
    pub(crate) group: usize,
}

impl Waiver {
    pub(crate) fn matches(&self, rule: &str, path: &str, raw_path: &str, line: u32, message: &str) -> bool {
        (self.rule == "*" || self.rule == rule)
            && (self.line == 0 || self.line == line)
            && self.path.as_ref().is_none_or(|p| p.matches(path))
            && self.contains.as_ref().is_none_or(|s| s.is_empty() || raw_path.contains(s.as_str()))
            // The message pattern is searched for, as std::regex_search.
            && self.message_re.as_ref().is_none_or(|re| re.is_match_anywhere(message))
    }
}

/// A waiver's `message` compiled; an error names the waiver as uvmcheck
/// does (`waiver LABEL: invalid message regex 'P': WHY`).
fn message_regex(label: &str, message: &str) -> Result<Option<crate::pattern::Pattern>, String> {
    if message.is_empty() {
        return Ok(None);
    }
    crate::pattern::Pattern::new(message, false)
        .map(Some)
        .map_err(|e| format!("waiver {label}: invalid message regex '{message}': {e}"))
}

/// Reads a waiver file: a list of waivers, or an object with `waivers`;
/// each with `rule` (or `id`), `path` (or `file`), `line`, `reason`,
/// `owner`, `message`, `severity`.
pub(crate) fn load_waiver_file(file: &Path) -> Result<Vec<Waiver>, String> {
    let root = load_json(file)?;
    let list = match &root {
        Json::Arr(_) => &root,
        Json::Obj(_) => root.get("waivers"),
        _ => &Json::Null,
    };
    let Json::Arr(items) = list else {
        return Err(format!("{}: expected a list of waivers, or an object with \"waivers\"", file.display()));
    };
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut out = Vec::new();
    for w in items.iter().filter(|w| matches!(w, Json::Obj(_))) {
        let rule = w.get("rule").as_str().or(w.get("id").as_str()).unwrap_or("").to_string();
        if rule.is_empty() {
            continue;
        }
        let path =
            w.get("path").as_str().filter(|s| !s.is_empty()).or(w.get("file").as_str()).unwrap_or("").to_string();
        let sev = match w.get("severity") {
            Json::Null => None,
            v => Some(v.as_str().and_then(severity).ok_or_else(|| {
                format!("{}: waiver for {rule}: severity must be error, warning or info", file.display())
            })?),
        };
        let label = format!(
            "{name} #{} ({rule}{})",
            out.len() + 1,
            if path.is_empty() { String::new() } else { format!(" in {path}") }
        );
        let contains = path.trim_matches('*').to_string();
        let group = out.len();
        let message = w.get("message").as_str().unwrap_or("").to_string();
        let message_re = message_regex(&label, &message).map_err(|e| format!("{}: {e}", file.display()))?;
        out.push(Waiver {
            rule,
            path: None,
            contains: Some(contains),
            line: w.get("line").as_u32().unwrap_or(0),
            message,
            message_re,
            severity: sev,
            label,
            owner: w.get("owner").as_str().unwrap_or("").to_string(),
            group,
        });
    }
    Ok(out)
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
    /// Integer rule options: (rule ID or name, key, value).
    pub options: Vec<(String, String, i64)>,
    /// The `style` block.
    pub style: Vec<(String, crate::options::StyleValue)>,
    /// The `naming` block.
    pub naming: Vec<(String, crate::options::NamingOverride)>,
    /// The project root: the folder holding `.uvmcheck/` (or the config).
    pub root: PathBuf,
    /// Waivers past their `expires` date (`label expired DATE`), for the
    /// waiver report.
    pub expired: Vec<String>,
    /// Layered Python configs: this file and the parents it builds on.
    pub layers: Vec<String>,
    /// Each rule's `options` object as written (rule key, JSON text), for
    /// Python rules.
    pub rule_options: Vec<(String, String)>,
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
pub(crate) fn python_sdk() -> Option<PathBuf> {
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
                        if let Json::Obj(_) = entry.get("options") {
                            c.rule_options.push((key.clone(), entry.get("options").to_string()));
                        }
                        if let Json::Obj(opts) = entry.get("options") {
                            for (k, v) in opts {
                                match v {
                                    Json::Num(n) if n.fract() == 0.0 => {
                                        c.options.push((key.clone(), k.clone(), *n as i64))
                                    }
                                    Json::Bool(b) => c.options.push((key.clone(), k.clone(), i64::from(*b))),
                                    _ => c.unapplied.push(format!("rules.{key}.options.{k}")),
                                }
                            }
                        }
                    }
                    _ => return Err(format!("rules.{key}: expected an object, true/false, or \"off\"")),
                }
                c.rules.push((key.clone(), rs));
            }
        }
        // As uvmcheck: a rules key that names no rule is a warning.
        let all = crate::rules();
        for (key, _) in &c.rules {
            let known =
                all.iter().any(|r| r.id == key || r.name == key || crate::sub_rules(r.id).iter().any(|s| s.0 == key));
            if !known {
                c.warnings.push(format!("rules: '{key}' does not match any rule ID or name"));
            }
        }
        if let Json::Obj(style) = root.get("style") {
            for (k, v) in style {
                let val = match v {
                    Json::Bool(b) => crate::options::StyleValue::Bool(*b),
                    Json::Num(n) => crate::options::StyleValue::Int(*n as i64),
                    Json::Str(s) => crate::options::StyleValue::Str(s.clone()),
                    _ => continue,
                };
                match k.as_str() {
                    // uvmcheck reads it but no rule acts on it.
                    "one_statement_per_line" => {}
                    "max_line_length"
                    | "allow_tabs"
                    | "allow_trailing_whitespace"
                    | "mandatory_matching_end_label"
                    | "require_file_header"
                    | "file_matches_class_name" => {}
                    "file_header_pattern" => {
                        let why = v.as_str().filter(|p| !p.is_empty()).and_then(crate::header_pattern_error);
                        let required =
                            style.iter().any(|(k, v)| k == "require_file_header" && v.as_bool() == Some(true));
                        if let Some(why) = why.filter(|_| required) {
                            c.unapplied.push(format!("style.file_header_pattern ({why}; the header check is off)"));
                        }
                    }
                    other => c.warnings.push(format!("unknown style setting '{other}' ignored")),
                }
                c.style.push((k.clone(), val));
            }
        }
        if let Json::Obj(naming) = root.get("naming") {
            for (target, block) in naming {
                let s = |k: &str| block.get(k).as_str().map(str::to_string);
                let o = crate::options::NamingOverride {
                    prefix: s("prefix"),
                    suffix: s("suffix"),
                    pattern: s("pattern"),
                    casing: s("casing"),
                    severity: s("severity"),
                    enabled: block.get("enabled").as_bool(),
                };
                if o.enabled != Some(false)
                    && !crate::naming_pattern_supported(target, o.pattern.as_deref().unwrap_or(""))
                {
                    c.unapplied.push(format!("naming.{target} (a target or pattern the native engine cannot check)"));
                }
                c.naming.push((target.clone(), o));
            }
        }
        c.add_waivers(root.get("waivers"), &base)?;
        // From the Python config loader: waivers it dropped as expired, its
        // warnings, and the layers it read.
        c.expired.extend(strings(root.get("expired_waivers")));
        c.warnings.extend(strings(root.get("warnings")));
        c.layers.extend(strings(root.get("layers")));
        c.root = project;
        // Compile settings.
        for s in strings(root.get("filelists")) {
            // Paths in the list are relative to it, as uvmcheck reads config file
            // lists (slang `processCommandFiles(makeRelative = true)`, i.e. -F).
            c.compile_args.extend(["-F".to_string(), anchor(&s, &base)]);
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
        for (i, w) in list.as_arr().iter().enumerate() {
            let mut rules = strings(w.get("rules"));
            rules.extend(strings(w.get("rule")));
            if rules.is_empty() {
                return Err("every waiver needs \"rules\" (a rule ID, name, or \"*\")".into());
            }
            if w.get("reason").as_str().is_none_or(str::is_empty) {
                return Err(format!("waiver for {} needs a \"reason\"", rules[0]));
            }
            let label =
                w.get("label").as_str().map_or_else(|| format!("waiver #{} ({})", i + 1, rules[0]), str::to_string);
            if let Some(e) = w.get("expires").as_str() {
                if e < today.as_str() {
                    self.warnings.push(format!("waiver for {} expired on {e}; it no longer applies", rules[0]));
                    self.expired.push(format!("{label} expired {e}"));
                    continue;
                }
            }
            let owner = w.get("owner").as_str().unwrap_or("").to_string();
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
            let message_re = message_regex(&label, &message)?;
            let sev = w.get("severity").as_str().and_then(severity);
            let globs: Vec<Option<Pattern>> =
                if globs.is_empty() { vec![None] } else { globs.into_iter().map(Some).collect() };
            let all = crate::rules();
            for r in &rules {
                // A rule's name names its ID, as uvmcheck resolves it.
                let id = all.iter().find(|x| x.name == r.as_str()).map_or(r.clone(), |x| x.id.to_string());
                for g in &globs {
                    self.waivers.push(Waiver {
                        rule: id.clone(),
                        path: g.clone(),
                        contains: None,
                        line,
                        message: message.clone(),
                        message_re: message_re.clone(),
                        severity: sev,
                        label: label.clone(),
                        owner: owner.clone(),
                        group: i,
                    });
                }
            }
        }
        Ok(())
    }

    fn rule(&self, id: &str, name: &str) -> Option<&RuleSettings> {
        self.rules.iter().find(|(k, _)| k == id || (!name.is_empty() && k == name)).map(|(_, r)| r)
    }

    /// The rule runs under this config (`enabled` not false).
    pub fn enabled(&self, id: &str, name: &str) -> bool {
        self.rule(id, name).and_then(|r| r.enabled) != Some(false)
    }

    /// Applies the config to `found`: drops findings of rules it turns off
    /// and in library and excluded files, and regrades severities (waivers
    /// come after, in [`crate::suppress`]).
    pub fn apply(&self, found: &mut Vec<Finding>, files: &[String]) {
        // Config keys and waivers name a rule by ID or by a rule's name, as
        // uvmcheck resolves them: a sub-rule (SV-STY-102 ...) by its ID only.
        let all = crate::rules();
        let names: std::collections::HashMap<&str, &str> = all.iter().map(|r| (r.id, r.name)).collect();
        let mut cache: std::collections::HashMap<u32, String> = Default::default();
        let mut norm = |f: u32| -> String {
            cache
                .entry(f)
                .or_insert_with(|| normalized(files.get(f as usize).map(String::as_str).unwrap_or("")))
                .clone()
        };
        found.retain_mut(|x| {
            let name = names.get(x.rule).copied().unwrap_or("");
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
            true
        });
    }

    /// Nothing is reported in `file`: a library or excluded path.
    pub fn hides_file(&self, file: &str) -> bool {
        let path = normalized(file);
        self.library.iter().any(|p| p.matches(&path)) || self.exclude.iter().any(|p| p.matches(&path))
    }

    /// The config's waivers, in order.
    pub(crate) fn waivers(&self) -> &[Waiver] {
        &self.waivers
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
