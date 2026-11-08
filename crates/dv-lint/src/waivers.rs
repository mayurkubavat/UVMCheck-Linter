//! Inline waivers, as uvmcheck's WaiverEngine reads them:
//!
//! - on the finding's own line: `uvmcheck:waive RULE`, `uvmcheck:disable`,
//!   `uvmcheck-disable`, `lint:waive`, `lint:off`, `uvmcheck: waive`;
//! - on the line above: `uvmcheck-disable-next-line RULE`,
//!   `uvmcheck:waive-next-line`, `uvmcheck:disable-next-line`.
//!
//! `*` waives any rule. A quoted pattern after the rule narrows the waiver to
//! findings whose message matches it; patterns without regex syntax match as
//! text, and the rest (no regex engine here) waive like no pattern.

use std::collections::HashMap;

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

fn pattern_matches(pattern: &str, message: &str) -> bool {
    let regex_syntax = pattern.chars().any(|c| "\\.^$*+?()[]{}|".contains(c));
    regex_syntax || message.contains(pattern)
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

/// Drops findings an inline comment waives; returns how many.
pub(crate) fn apply(findings: &mut Vec<Finding>, files: &[String]) -> usize {
    let before = findings.len();
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
        let waived = waives(at(line), f, true) || (line > 1 && waives(at(line - 1), f, false));
        !waived
    });
    before - findings.len()
}
