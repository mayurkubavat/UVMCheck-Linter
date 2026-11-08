//! Fix-its: the rewrites rules attach to findings, and applying them as
//! uvmcheck's FixPlanner does (`--fix`, `--fix-dry-run`).
//!
//! Edits of one file are applied in file order; exact repeats collapse to
//! one and an edit overlapping one already taken is left for the next pass,
//! when the file is linted again (a fix can also uncover a finding). Fixes
//! that would rewrite a macro call from its expansion are shown but never
//! applied, as are fixes inside macro expansions (they have no place in the
//! file) and fixes in files with compile errors.

/// How safe a fix is to apply automatically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixSafety {
    /// Applied by `--fix`: keeps simulation behaviour.
    Safe,
    /// Applied only with `--unsafe-fixes`: can change behaviour.
    Unsafe,
    /// Shown in reports, never applied.
    DisplayOnly,
}

impl FixSafety {
    /// The name uvmcheck's reports use.
    pub fn as_str(self) -> &'static str {
        match self {
            FixSafety::Safe => "safe",
            FixSafety::Unsafe => "unsafe",
            FixSafety::DisplayOnly => "display-only",
        }
    }
}

/// A replacement of bytes `start..end` of the file at `path` with `text`.
#[derive(Clone, Debug)]
pub struct Fix {
    pub path: String,
    pub start: u32,
    pub end: u32,
    pub text: String,
    pub description: String,
    pub safety: FixSafety,
}

/// One edit, for [`plan`].
#[derive(Clone, Debug)]
pub struct TextEdit {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub rule: &'static str,
}

/// The result of applying a file's edits.
#[derive(Default, Debug)]
pub struct FilePlan {
    pub updated: String,
    pub applied: usize,
    pub duplicates: usize,
    pub skipped: Vec<TextEdit>,
    /// A unified diff from the original ("" when unchanged).
    pub diff: String,
}

/// Start offset of every line, plus a sentinel at the end.
fn line_starts(text: &str) -> Vec<usize> {
    let b = text.as_bytes();
    let mut s = vec![0];
    for i in 0..b.len() {
        if b[i] == b'\n' && i + 1 < b.len() {
            s.push(i + 1);
        }
    }
    s.push(b.len());
    s
}

fn line_of(starts: &[usize], offset: usize) -> usize {
    // The last line start at or before `offset` (the sentinel excluded).
    starts[..starts.len() - 1].partition_point(|&s| s <= offset) - 1
}

fn split_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < text.len() {
        match text[pos..].find('\n') {
            Some(n) => {
                out.push(text[pos..pos + n].to_string());
                pos += n + 1;
            }
            None => {
                out.push(text[pos..].to_string());
                break;
            }
        }
    }
    out
}

struct Change {
    first: usize,
    last: usize,
    new_lines: Vec<String>,
}

const CONTEXT: usize = 3;

fn render_diff(old: &[String], changes: &[Change], path: &str) -> String {
    if changes.is_empty() {
        return String::new();
    }
    let mut out = format!("--- a/{path}\n+++ b/{path}\n");
    let mut delta: i64 = 0;
    let mut i = 0;
    while i < changes.len() {
        let mut j = i;
        while j + 1 < changes.len() && changes[j + 1].first <= changes[j].last + 1 + 2 * CONTEXT {
            j += 1;
        }
        let first = changes[i].first.saturating_sub(CONTEXT);
        let last = (old.len() - 1).min(changes[j].last + CONTEXT);
        let mut body = Vec::new();
        let (mut old_n, mut new_n) = (0usize, 0usize);
        let mut line = first;
        let mut hunk_delta: i64 = 0;
        for c in &changes[i..=j] {
            while line < c.first {
                body.push(format!(" {}", old[line]));
                line += 1;
                old_n += 1;
                new_n += 1;
            }
            for l in &old[c.first..=c.last] {
                body.push(format!("-{l}"));
                old_n += 1;
            }
            for nl in &c.new_lines {
                body.push(format!("+{nl}"));
                new_n += 1;
            }
            hunk_delta += c.new_lines.len() as i64 - (c.last - c.first + 1) as i64;
            line = c.last + 1;
        }
        while line <= last {
            body.push(format!(" {}", old[line]));
            line += 1;
            old_n += 1;
            new_n += 1;
        }
        let old_start = first as i64 + 1;
        let new_start = old_start + delta;
        out.push_str(&format!(
            "@@ -{old_start},{old_n} +{},{new_n} @@\n",
            if new_n > 0 { new_start } else { new_start - 1 }
        ));
        for b in body {
            out.push_str(&b);
            out.push('\n');
        }
        delta += hunk_delta;
        i = j + 1;
    }
    out
}

/// Applies non-overlapping `edits` to `original` (FixPlanner::planEdits):
/// identical edits collapse to one; one overlapping an edit already taken
/// (in file order), or inserting at the same point, is skipped.
pub fn plan(original: &str, mut edits: Vec<TextEdit>, display: &str) -> FilePlan {
    let mut p = FilePlan::default();
    edits.sort_by_key(|e| (e.start, e.end));
    let mut taken: Vec<TextEdit> = Vec::new();
    for e in edits {
        if e.start > e.end
            || e.end > original.len()
            || !original.is_char_boundary(e.start)
            || !original.is_char_boundary(e.end)
        {
            p.skipped.push(e);
            continue;
        }
        if let Some(prev) = taken.last() {
            if prev.start == e.start && prev.end == e.end && prev.text == e.text {
                p.duplicates += 1;
                continue;
            }
            if e.start < prev.end || e.start == prev.start {
                p.skipped.push(e);
                continue;
            }
        }
        taken.push(e);
    }
    p.applied = taken.len();
    let starts = line_starts(original);
    let old = split_lines(original);
    let mut changes = Vec::new();
    let mut cursor = 0;
    let mut k = 0;
    while k < taken.len() {
        let end_line = |e: &TextEdit| line_of(&starts, if e.end > e.start { e.end - 1 } else { e.start });
        let first = line_of(&starts, taken[k].start);
        let mut last = end_line(&taken[k]);
        let mut m = k;
        while m + 1 < taken.len() && line_of(&starts, taken[m + 1].start) <= last {
            m += 1;
            last = last.max(end_line(&taken[m]));
        }
        let block_start = starts[first];
        let block_end = original.len().min(starts[last + 1]);
        p.updated.push_str(&original[cursor..block_start]);
        let mut block = String::new();
        let mut pos = block_start;
        for e in &taken[k..=m] {
            block.push_str(&original[pos..e.start]);
            block.push_str(&e.text);
            pos = e.end;
        }
        block.push_str(&original[pos..block_end]);
        p.updated.push_str(&block);
        cursor = block_end;
        let last_l = last.min(old.len().saturating_sub(1));
        let new_lines = split_lines(&block);
        let same = last_l + 1 - first == new_lines.len() && new_lines[..] == old[first..=last_l];
        if !same {
            changes.push(Change { first, last: last_l, new_lines });
        }
        k = m + 1;
    }
    p.updated.push_str(&original[cursor..]);
    if p.updated != original && !old.is_empty() {
        p.diff = render_diff(&old, &changes, display);
    }
    p
}

/// True when `text[start..end]` calls a macro (`` `name ``) that `replacement`
/// no longer calls: the fix was built from the expansion and would inline it.
pub fn drops_macro_call(text: &str, start: usize, end: usize, replacement: &str) -> bool {
    if start >= end || end > text.len() {
        return false;
    }
    let span = &text.as_bytes()[start..end];
    let mut pos = 0;
    while let Some(off) = span[pos..].iter().position(|&b| b == b'`') {
        let at = pos + off;
        let mut stop = at + 1;
        while stop < span.len() && (span[stop].is_ascii_alphanumeric() || span[stop] == b'_') {
            stop += 1;
        }
        if stop > at + 1 {
            let call = std::str::from_utf8(&span[at..stop]).unwrap_or("");
            if !replacement.contains(call) {
                return true;
            }
        }
        pos = at + 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(start: usize, end: usize, text: &str) -> TextEdit {
        TextEdit { start, end, text: text.into(), rule: "R" }
    }

    #[test]
    fn edits_in_order_repeats_once_overlaps_wait() {
        let src = "a  \nbcd\nefg\n";
        let p = plan(src, vec![e(1, 3, ""), e(1, 3, ""), e(4, 5, "B"), e(4, 6, "x")], "f.sv");
        assert_eq!(p.updated, "a\nBcd\nefg\n");
        assert_eq!((p.applied, p.duplicates, p.skipped.len()), (2, 1, 1));
        assert_eq!(p.diff, "--- a/f.sv\n+++ b/f.sv\n@@ -1,3 +1,3 @@\n-a  \n+a\n-bcd\n+Bcd\n efg\n");
    }

    #[test]
    fn macro_calls_are_not_inlined() {
        assert!(drops_macro_call("x = `W;", 4, 6, "8"));
        assert!(!drops_macro_call("x = `W;", 4, 6, "`W"));
    }
}

/// What one `--fix` (or `--fix-dry-run`) pass did.
#[derive(Default, Debug)]
pub struct PassResult {
    pub applied: usize,
    /// Files written, with a hash of their new text (to spot fixes that undo
    /// each other across passes).
    pub written: Vec<(String, u64)>,
    /// Unified diffs (`--fix-dry-run`).
    pub diffs: String,
    /// The summary line, empty when there was nothing to do.
    pub summary: String,
    pub changed_files: usize,
}

fn hash(s: &str) -> u64 {
    let mut h: u64 = 1_469_598_103_934_665_603;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(1_099_511_628_211);
    }
    h
}

/// Applies the fixes of `found` (Driver's --fix pass): `only` limits them to
/// these rule IDs or names, `unsafe_ok` admits unsafe fixes, files in `broken`
/// (with compile errors) are left alone, and with `dry_run` nothing is
/// written.
pub fn apply(
    found: &[crate::Finding],
    only: &[String],
    unsafe_ok: bool,
    dry_run: bool,
    broken: &std::collections::HashSet<String>,
) -> PassResult {
    use std::collections::BTreeMap;
    let mut r = PassResult::default();
    let mut per_file: BTreeMap<String, Vec<TextEdit>> = BTreeMap::new();
    let (mut unsafe_skipped, mut broken_fixes) = (0usize, 0usize);
    for f in found {
        if !only.is_empty() && !only.iter().any(|o| o == f.rule || o == f.name) {
            continue;
        }
        for x in &f.fixes {
            match x.safety {
                FixSafety::DisplayOnly => continue,
                FixSafety::Unsafe if !unsafe_ok => {
                    unsafe_skipped += 1;
                    continue;
                }
                _ => {}
            }
            if x.path.is_empty() {
                continue;
            }
            if broken.contains(&x.path) {
                broken_fixes += 1;
                continue;
            }
            per_file.entry(x.path.clone()).or_default().push(TextEdit {
                start: x.start as usize,
                end: x.end as usize,
                text: x.text.clone(),
                rule: f.rule,
            });
        }
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut skipped = 0;
    for (path, edits) in per_file {
        let display = std::path::Path::new(&path)
            .strip_prefix(&cwd)
            .map_or_else(|_| path.clone(), |p| p.to_string_lossy().into_owned());
        let Ok(content) = std::fs::read_to_string(&path) else {
            eprintln!("Warning: --fix: cannot read {display}");
            continue;
        };
        let p = plan(&content, edits, &display);
        r.applied += p.applied;
        skipped += p.skipped.len();
        if p.updated == content {
            continue;
        }
        r.changed_files += 1;
        if dry_run {
            r.diffs.push_str(&p.diff);
            continue;
        }
        // A sibling file renamed over the original: an interrupted run never
        // leaves a half-written source.
        let tmp = format!("{path}.uvmcheck-fix.tmp");
        if std::fs::write(&tmp, &p.updated).is_err() {
            eprintln!("Warning: --fix: cannot write {display}");
            let _ = std::fs::remove_file(&tmp);
            continue;
        }
        if let Ok(meta) = std::fs::metadata(&path) {
            let _ = std::fs::set_permissions(&tmp, meta.permissions());
        }
        if let Err(e) = std::fs::rename(&tmp, &path) {
            eprintln!("Warning: --fix: cannot replace {display}: {e}");
            let _ = std::fs::remove_file(&tmp);
            continue;
        }
        r.written.push((path, hash(&p.updated)));
    }
    if r.applied > 0 || skipped > 0 || unsafe_skipped > 0 || broken_fixes > 0 {
        let mut line = if unsafe_skipped > 0 {
            format!(
                "[uvmcheck] {} {}, {unsafe_skipped} unsafe fixes skipped (use --unsafe-fixes)",
                r.applied,
                if dry_run { "fixes would be applied" } else { "fixes applied" }
            )
        } else {
            format!(
                "[uvmcheck] {} {} fix-it(s) in {} file(s)",
                if dry_run { "Would apply" } else { "Applied" },
                r.applied,
                r.changed_files
            )
        };
        if skipped > 0 {
            line.push_str(&format!(
                "; {skipped} overlapping fix-it(s) {}",
                if dry_run { "left for another --fix run" } else { "deferred to the next pass" }
            ));
        }
        if broken_fixes > 0 {
            line.push_str(&format!("; {broken_fixes} in files with compile errors not applied"));
        }
        line.push_str(".\n");
        r.summary = line;
    }
    r
}

/// Marks fixes that would rewrite a macro call from its expansion as shown
/// only (DiagnosticEngine::noteFixLocations).
pub fn guard_macro_calls(found: &mut [crate::Finding]) {
    let mut texts: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for f in found {
        for x in &mut f.fixes {
            if x.safety == FixSafety::DisplayOnly {
                continue;
            }
            let text =
                texts.entry(x.path.clone()).or_insert_with(|| std::fs::read_to_string(&x.path).unwrap_or_default());
            if drops_macro_call(text, x.start as usize, x.end as usize, &x.text) {
                x.safety = FixSafety::DisplayOnly;
                x.description.push_str(" (not applied: it would remove a macro call)");
            }
        }
    }
}

/// Line and column (1-based) of byte offsets in files, for reports.
#[derive(Default)]
pub struct Positions {
    starts: std::collections::HashMap<String, Vec<usize>>,
}

impl Positions {
    /// (line, column) of `offset` in `path`.
    pub fn at(&mut self, path: &str, offset: u32) -> (usize, usize) {
        let starts = self.starts.entry(path.to_string()).or_insert_with(|| {
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let mut s = vec![0];
            s.extend(text.bytes().enumerate().filter(|(_, b)| *b == b'\n').map(|(i, _)| i + 1));
            s
        });
        let o = offset as usize;
        let line = starts.partition_point(|&s| s <= o).max(1);
        (line, o - starts[line - 1] + 1)
    }
}

/// `f` with the given safety.
pub(crate) fn with_safety(f: Option<Fix>, safety: FixSafety) -> Option<Fix> {
    f.map(|mut x| {
        x.safety = safety;
        x
    })
}
