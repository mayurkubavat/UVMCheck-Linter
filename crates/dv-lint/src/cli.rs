//! The uvmcheck options the dv-lint binary takes beyond rule selection and
//! reports: `--explain`, `--changed-since` / `--strict-new-files`, the
//! waiver-hygiene reports (`--waiver-report`, `--report-unused-waivers`,
//! `--strict-waivers`), the result cache (`--no-cache`, `--cache-dir`,
//! `--clear-cache`), `--map`, `--init` and `--python-rules`, with what they
//! do after the rules ran. main.rs parses its own options and hands the rest
//! here.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::{Finding, Severity};

/// The options, as given.
#[derive(Default, Debug, Clone)]
pub struct Extra {
    pub explain: Option<String>,
    pub changed_since: Option<String>,
    pub strict_new_files: bool,
    pub waiver_report: bool,
    pub report_unused_waivers: bool,
    pub strict_waivers: bool,
    pub no_cache: bool,
    pub cache_dir: Option<String>,
    pub clear_cache: bool,
    pub map: Option<String>,
    pub init: bool,
    pub python_rules: Option<String>,
}

/// What [`Extra::after_suppress`] decided for the exit status.
#[derive(Default, Debug, Clone, Copy)]
pub struct Outcome {
    /// `--strict-new-files`: a file new since the revision has a finding.
    pub strict_new_failed: bool,
    /// `--strict-waivers`: a waiver, waiver comment or baseline entry
    /// matched nothing, or a waiver expired.
    pub hygiene_failed: bool,
}

const VALUED: [&str; 5] = ["--explain", "--changed-since", "--cache-dir", "--map", "--python-rules"];

impl Extra {
    /// Takes `a` (with its value from `rest`) when it is one of these
    /// options; Err when its value is missing.
    pub fn take(&mut self, a: &str, rest: &mut dyn Iterator<Item = String>) -> Result<bool, String> {
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) if VALUED.contains(&f) => (f, Some(v.to_string())),
            _ => (a, None),
        };
        let mut value = || inline.clone().or_else(|| rest.next()).ok_or_else(|| format!("{flag} needs a value"));
        match flag {
            "--explain" => self.explain = Some(value()?),
            "--changed-since" => self.changed_since = Some(value()?),
            "--cache-dir" => self.cache_dir = Some(value()?),
            "--map" => self.map = Some(value()?),
            "--python-rules" => self.python_rules = Some(value()?),
            "--strict-new-files" => self.strict_new_files = true,
            "--waiver-report" => self.waiver_report = true,
            "--report-unused-waivers" => self.report_unused_waivers = true,
            "--strict-waivers" => self.strict_waivers = true,
            "--no-cache" => self.no_cache = true,
            "--clear-cache" => self.clear_cache = true,
            "--init" => self.init = true,
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Any waiver-hygiene option.
    pub fn hygiene(&self) -> bool {
        self.waiver_report || self.report_unused_waivers || self.strict_waivers
    }

    /// Checks option combinations before any work.
    pub fn validate(&self) -> Result<(), String> {
        if self.strict_new_files && self.changed_since.is_none() {
            return Err("--strict-new-files requires --changed-since <git-ref>".into());
        }
        Ok(())
    }

    /// The cache folder: `--cache-dir`, else `.uvmcheck_cache` in the
    /// project root.
    pub fn cache_folder(&self, root: &Path) -> PathBuf {
        self.cache_dir.as_ref().map_or_else(|| root.join(".uvmcheck_cache"), PathBuf::from)
    }

    /// After the waivers and baseline: `--changed-since` keeps findings on
    /// changed lines; the hygiene options report waivers and comments that
    /// matched nothing (`--report-unused-waivers` and `--strict-waivers` as
    /// UVM-WV-001 warnings). `user_files` are the files whose waiver
    /// comments count, `active` the rules that ran. Err for a git failure.
    #[allow(clippy::too_many_arguments)]
    pub fn after_suppress(
        &self,
        found: &mut Vec<Finding>,
        files: &mut Vec<String>,
        root: &Path,
        suppress: Option<&crate::suppress::Suppress>,
        config: Option<&crate::project::Config>,
        user_files: &[String],
        active: &HashSet<&str>,
    ) -> Result<Outcome, String> {
        let mut out = Outcome::default();
        if let Some(rev) = &self.changed_since {
            let changed = crate::changed::ChangedLines::load(rev, root).map_err(|e| format!("--changed-since: {e}"))?;
            let before = found.len();
            found.retain(|f| {
                let (file, line, _) = f.position_in(files);
                !file.is_empty() && changed.contains(file, line)
            });
            eprintln!(
                "[dv-lint] --changed-since {rev}: {} finding(s) on changed lines in {} changed file(s); {} elsewhere not shown",
                found.len(),
                changed.file_count(),
                before - found.len()
            );
            if self.strict_new_files {
                let fresh = found.iter().filter(|f| changed.is_new_file(f.position_in(files).0)).count();
                if fresh > 0 {
                    out.strict_new_failed = true;
                    eprintln!("[dv-lint] --strict-new-files: {fresh} finding(s) in files that did not exist at {rev}");
                }
            }
        }
        if !self.hygiene() {
            return Ok(out);
        }
        let Some(s) = suppress else { return Ok(out) };
        let h = s.hygiene(user_files, active);
        let expired: &[String] = config.map_or(&[], |c| &c.expired);
        out.hygiene_failed =
            !h.unused.is_empty() || !h.unused_inline.is_empty() || h.stale_baseline > 0 || !expired.is_empty();
        if self.report_unused_waivers || self.strict_waivers {
            // Off by default: a waiver file shared by runs over different
            // files can match nothing in one of them without being wrong.
            let where_ = config.map(|c| c.file.display().to_string()).unwrap_or_default();
            let mut add = |file: &str, line: u32, message: String| {
                let idx = match files.iter().position(|f| f == file) {
                    Some(i) => i,
                    None => {
                        files.push(file.to_string());
                        files.len() - 1
                    }
                };
                found.push(Finding {
                    rule: "UVM-WV-001",
                    name: "UnusedWaiver",
                    severity: Severity::Warning,
                    message,
                    loc: dv_model::Loc { file: idx as u32, line, column: 1, expansion: None },
                    related: Vec::new(),
                    fixes: Vec::new(),
                });
            };
            for label in &h.unused {
                add(&where_, 1, format!("waiver {label} matched no diagnostic"));
            }
            for e in expired {
                add(&where_, 1, format!("expired waiver {e}"));
            }
            for (file, line, text) in &h.unused_inline {
                add(file, *line, format!("waiver comment matched no diagnostic: {text}"));
            }
        }
        if self.waiver_report || self.strict_waivers {
            print!("{}", report(&h, expired, self.strict_waivers && out.hygiene_failed));
        }
        Ok(out)
    }
}

/// The `=== Waiver hygiene ===` report, as uvmcheck prints it.
pub fn report(h: &crate::suppress::Hygiene, expired: &[String], strict_failed: bool) -> String {
    let mut o = String::from("\n=== Waiver hygiene ===\n");
    o.push_str(&format!(
        "Waivers: {} written, {} matched nothing, {} expired, {} without an owner\n",
        h.written,
        h.unused.len(),
        expired.len(),
        h.without_owner
    ));
    for u in &h.unused {
        o.push_str(&format!("  unused:  {u}\n"));
    }
    for e in expired {
        o.push_str(&format!("  expired: {e}\n"));
    }
    o.push_str(&format!("Waiver comments: {} in source, {} waive nothing\n", h.inline_comments, h.unused_inline.len()));
    for (file, line, text) in &h.unused_inline {
        o.push_str(&format!("  {file}:{line}: {text}\n"));
    }
    o.push_str(&format!(
        "Baseline: {} entr{}, {} no longer match (fixed or changed)\n",
        h.baseline_entries,
        if h.baseline_entries == 1 { "y" } else { "ies" },
        h.stale_baseline
    ));
    if !h.by_rule.is_empty() {
        let rules: Vec<String> = h.by_rule.iter().take(10).map(|(r, n)| format!("{r} {n}")).collect();
        o.push_str(&format!(
            "Waived findings by rule: {}{}\n",
            rules.join(", "),
            if h.by_rule.len() > 10 { ", ..." } else { "" }
        ));
        let owners: Vec<String> = h.by_owner.iter().map(|(r, n)| format!("{r} {n}")).collect();
        o.push_str(&format!("Waived findings by owner: {}\n", owners.join(", ")));
    }
    if strict_failed {
        o.push_str("[dv-lint] --strict-waivers: remove or fix the entries above (exit 4)\n");
    }
    o
}

/// The exit status: 1 for an ERROR finding or a failed
/// `--strict-new-files`, else 4 for failed `--strict-waivers`, else 0.
pub fn exit_status(found: &[Finding], outcome: Outcome, strict_waivers: bool) -> i32 {
    if found.iter().any(|f| f.severity == Severity::Error) || outcome.strict_new_failed {
        1
    } else if strict_waivers && outcome.hygiene_failed {
        4
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_its_options_with_values() {
        let mut x = Extra::default();
        let mut rest = vec!["HEAD".to_string()].into_iter();
        assert!(x.take("--changed-since", &mut rest).unwrap());
        assert!(x.take("--map=m.json", &mut std::iter::empty()).unwrap());
        assert!(x.take("--strict-waivers", &mut std::iter::empty()).unwrap());
        assert!(!x.take("--json", &mut std::iter::empty()).unwrap());
        assert!(x.take("--explain", &mut std::iter::empty()).is_err());
        assert_eq!(x.changed_since.as_deref(), Some("HEAD"));
        assert_eq!(x.map.as_deref(), Some("m.json"));
        assert!(x.hygiene());
        x.strict_new_files = true;
        x.changed_since = None;
        assert!(x.validate().is_err());
    }
}
