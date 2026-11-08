//! Inline waivers, as uvmcheck's WaiverEngine reads them:
//!
//! - on the finding's own line: `uvmcheck:waive RULE`, `uvmcheck:disable`,
//!   `uvmcheck-disable`, `lint:waive`, `lint:off`, `uvmcheck: waive`;
//! - on the line above: `uvmcheck-disable-next-line RULE`,
//!   `uvmcheck:waive-next-line`, `uvmcheck:disable-next-line`.
//!
//! `*` waives any rule. A quoted pattern after the rule narrows the waiver to
//! findings whose message matches it; patterns without regex syntax match as
//! text, and the rest (no regex engine here yet) waive nothing, with a
//! warning: an unevaluated pattern must not hide findings.

use std::collections::{HashMap, HashSet};

use crate::Finding;

const SAME_LINE: [&str; 6] =
    ["uvmcheck:waive", "uvmcheck:disable", "uvmcheck-disable", "lint:waive", "lint:off", "uvmcheck: waive"];
const NEXT_LINE: [&str; 4] =
    ["uvmcheck-disable-next-line", "uvmcheck:waive-next-line", "uvmcheck:disable-next-line", "uvmcheck: waive"];

/// The quoted pattern after a keyword, if any (`None` when unterminated).
fn message_pattern(rest: &str) -> Option<Option<String>> {
    let Some(q) = rest.find('"') else { return Some(None) };
    let mut out = String::new();
    let mut it = rest[q + 1..].chars();
    while let Some(ch) = it.next() {
        match ch {
            '\\' => out.extend(it.next()),
            '"' => return Some(Some(out)),
            _ => out.push(ch),
        }
    }
    None
}

/// A pattern with regex syntax cannot be evaluated here (no regex engine
/// yet): it waives nothing, so a finding the comment did not mean to waive is
/// never hidden, and each such pattern is named once on stderr.
fn pattern_matches(pattern: &str, message: &str) -> bool {
    let regex_syntax = pattern.chars().any(|c| "\\.^$*+?()[]{}|".contains(c));
    if regex_syntax {
        static WARNED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
        if let Ok(mut w) = WARNED.lock() {
            if !w.iter().any(|p| p == pattern) {
                eprintln!(
                    "Warning: inline waiver pattern \"{pattern}\" uses regex syntax, which the native engine cannot evaluate yet; it waives nothing (uvmcheck-slang evaluates it)"
                );
                w.push(pattern.to_string());
            }
        }
        return false;
    }
    message.contains(pattern)
}

fn waives(text: &str, f: &Finding, same_line: bool) -> bool {
    let keywords: &[&str] = if same_line { &SAME_LINE } else { &NEXT_LINE };
    for kw in keywords {
        let mut from = 0;
        while let Some(p) = text[from..].find(kw) {
            let pos = from + p;
            from = pos + 1;
            let rest = &text[pos + kw.len()..];
            // `uvmcheck:waive` is a prefix of `uvmcheck:waive-next-line`.
            if rest.starts_with('-') || rest.starts_with(':') {
                continue;
            }
            if !rest.contains(f.rule) && !rest.contains('*') {
                continue;
            }
            match message_pattern(rest) {
                None => return false,
                Some(Some(p)) if !pattern_matches(&p, &f.message) => continue,
                _ => return true,
            }
        }
    }
    false
}

/// What inline comments waived so far, for the waiver report: the comment
/// lines used (file, line) and the waived findings per rule.
#[derive(Default)]
pub(crate) struct Used {
    pub(crate) lines: HashSet<(String, u32)>,
    pub(crate) by_rule: HashMap<String, usize>,
    /// The waived findings, with their file: a waiver file entry that also
    /// matches one counts as used, as in uvmcheck.
    pub(crate) waived: Vec<(String, u32, &'static str, String)>,
}

pub(crate) static USED: std::sync::Mutex<Option<Used>> = std::sync::Mutex::new(None);

/// Drops findings an inline comment waives; returns how many.
pub(crate) fn apply(findings: &mut Vec<Finding>, files: &[String]) -> usize {
    let before = findings.len();
    let mut used = Used::default();
    let mut texts: HashMap<String, Option<Vec<String>>> = HashMap::new();
    findings.retain(|f| {
        let (path, line, _) = f.position_in(files);
        if path.is_empty() || line == 0 {
            return true;
        }
        let lines = texts
            .entry(path.to_string())
            .or_insert_with(|| std::fs::read_to_string(path).ok().map(|t| t.lines().map(str::to_string).collect()));
        let Some(lines) = lines else { return true };
        let at = |l: u32| lines.get(l as usize - 1).map(String::as_str).unwrap_or("");
        let by = if waives(at(line), f, true) {
            Some(line)
        } else if line > 1 && waives(at(line - 1), f, false) {
            Some(line - 1)
        } else {
            None
        };
        if let Some(l) = by {
            used.lines.insert((path.to_string(), l));
            *used.by_rule.entry(f.rule.to_string()).or_default() += 1;
            used.waived.push((path.to_string(), line, f.rule, f.message.clone()));
        }
        by.is_none()
    });
    if let Ok(mut u) = USED.lock() {
        let all = u.get_or_insert_with(Used::default);
        all.lines.extend(used.lines);
        all.waived.extend(used.waived);
        for (r, n) in used.by_rule {
            *all.by_rule.entry(r).or_default() += n;
        }
    }
    before - findings.len()
}

/// A waiver comment that waived nothing: (file, line, comment text).
pub type Unused = (String, u32, String);

/// The waiver comments in `files`: how many there are, and those that waived
/// nothing while naming a rule that ran (`active`) or `*`, as uvmcheck's
/// waiver hygiene finds them (a keyword inside a `//` or `/*` comment).
pub(crate) fn unused_comments(files: &[String], active: &HashSet<&str>) -> (usize, Vec<Unused>) {
    const KEYWORDS: [&str; 8] = [
        "uvmcheck-disable-next-line",
        "uvmcheck:waive-next-line",
        "uvmcheck:disable-next-line",
        "uvmcheck:waive",
        "uvmcheck:disable",
        "uvmcheck-disable",
        "lint:waive",
        "lint:off",
    ];
    let used = USED.lock().ok().and_then(|u| u.as_ref().map(|u| u.lines.clone())).unwrap_or_default();
    let mut count = 0;
    let mut unused = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(file) else { continue };
        for (i, l) in text.lines().enumerate() {
            let Some(at) = KEYWORDS.iter().filter_map(|k| l.find(k)).min() else { continue };
            let comment = l[..at].rfind("//");
            if comment.is_none() && !l[..at].contains("/*") {
                continue;
            }
            count += 1;
            let line = i as u32 + 1;
            if used.contains(&(file.clone(), line)) {
                continue;
            }
            if rule_ids(&l[at..]).any(|r| r == "*" || active.contains(r)) {
                unused.push((file.clone(), line, l[comment.unwrap_or(at)..].to_string()));
            }
        }
    }
    (count, unused)
}

/// The rule IDs (`SV-STY-002`, `UVM-NAM-101.x`) and `*` in `text`, as
/// uvmcheck's `[A-Z][A-Z0-9]*-[A-Z][A-Z0-9]*-[0-9]+[A-Za-z0-9.]*|\*`.
fn rule_ids(text: &str) -> impl Iterator<Item = &str> {
    let b = text.as_bytes();
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < b.len() {
            if b[i] == b'*' {
                i += 1;
                return Some(&text[i - 1..i]);
            }
            if let Some(n) = id_at(&b[i..]) {
                let s = &text[i..i + n];
                i += n;
                return Some(s);
            }
            i += 1;
        }
        None
    })
}

/// The length of a rule ID at the start of `b`, if one starts there.
fn id_at(b: &[u8]) -> Option<usize> {
    let up = |c: u8| c.is_ascii_uppercase();
    let upnum = |c: u8| c.is_ascii_uppercase() || c.is_ascii_digit();
    let mut i = 0;
    for _ in 0..2 {
        if !b.get(i).copied().is_some_and(up) {
            return None;
        }
        i += 1;
        while b.get(i).copied().is_some_and(upnum) {
            i += 1;
        }
        if b.get(i) != Some(&b'-') {
            return None;
        }
        i += 1;
    }
    let digits = i;
    while b.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    if i == digits {
        return None;
    }
    while b.get(i).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'.') {
        i += 1;
    }
    Some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_rule_ids_as_uvmcheck_does() {
        let ids: Vec<&str> = rule_ids(" SV-STY-002 -- x, UVM-NAM-101.a and * not sv-sty-1 or A-B-").collect();
        assert_eq!(ids, ["SV-STY-002", "UVM-NAM-101.a", "*"]);
    }
}
