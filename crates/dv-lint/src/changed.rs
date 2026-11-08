//! `--changed-since REF`: the lines added or changed since a git revision,
//! as uvmcheck's ChangedLines reads them. `git diff --unified=0 REF --`
//! (the working tree against REF, staged or not) gives the changed ranges
//! of each file; untracked files (`git ls-files --others --exclude-standard`)
//! count as changed throughout. A file that did not exist at REF (added in
//! the diff, or untracked) is a new file, for `--strict-new-files`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The changed lines of a work tree since a revision.
#[derive(Default, Debug)]
pub struct ChangedLines {
    ranges: HashMap<PathBuf, Vec<(u32, u32)>>,
    whole: HashSet<PathBuf>,
    new_files: HashSet<PathBuf>,
}

fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

impl ChangedLines {
    /// The lines changed since `rev` in the repository holding `dir`.
    pub fn load(rev: &str, dir: &Path) -> Result<ChangedLines, String> {
        let d = dir.to_string_lossy();
        let root = git(&["-C", &d, "rev-parse", "--show-toplevel"])
            .ok_or_else(|| format!("not inside a git repository: {}", dir.display()))?;
        let root = PathBuf::from(root.trim_end_matches(['\n', '\r']));
        let r = root.to_string_lossy();
        git(&["-C", &r, "rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")])
            .ok_or_else(|| format!("unknown git revision '{rev}'"))?;
        let diff = git(&[
            "-c",
            "core.quotepath=off",
            "-C",
            &r,
            "diff",
            "--unified=0",
            "--no-color",
            "--no-ext-diff",
            "--no-renames",
            rev,
            "--",
        ])
        .ok_or_else(|| format!("git diff {rev} failed"))?;
        let mut c = ChangedLines::default();
        c.parse_diff(&diff, &root);
        if let Some(others) = git(&["-c", "core.quotepath=off", "-C", &r, "ls-files", "--others", "--exclude-standard"])
        {
            for f in others.lines().filter(|f| !f.is_empty()) {
                let p = canonical(&root.join(f));
                c.whole.insert(p.clone());
                c.new_files.insert(p);
            }
        }
        Ok(c)
    }

    /// Reads a `--unified=0` diff; paths are relative to `root`.
    pub fn parse_diff(&mut self, diff: &str, root: &Path) {
        let mut current: Option<PathBuf> = None;
        let mut added = false;
        // `---`/`+++` are headers only before a file's first hunk: a removed
        // line `-- x` reads `--- x` in a hunk (uvmcheck takes it for a header).
        let mut header = true;
        for line in diff.lines() {
            if line.starts_with("diff ") {
                header = true;
            } else if !header && !line.starts_with("@@ ") {
                continue;
            }
            if let Some(old) = line.strip_prefix("--- ") {
                added = old == "/dev/null";
            } else if let Some(new) = line.strip_prefix("+++ ") {
                if new == "/dev/null" {
                    current = None;
                    added = false;
                    continue;
                }
                let p = canonical(&root.join(new.strip_prefix("b/").unwrap_or(new)));
                // A touched file, even if lines were only removed.
                self.ranges.entry(p.clone()).or_default();
                if added {
                    self.new_files.insert(p.clone());
                }
                added = false;
                current = Some(p);
            } else if line.starts_with("@@ ") {
                header = false;
                // @@ -a[,b] +c[,d] @@
                let (Some(cur), Some(plus)) = (&current, line.find(" +")) else { continue };
                let spec = line[plus + 2..].split(' ').next().unwrap_or("");
                let mut it = spec.splitn(2, ',');
                let start: u32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                let count: u32 = it.next().map_or(1, |s| s.parse().unwrap_or(0));
                if count > 0 {
                    self.ranges.entry(cur.clone()).or_default().push((start, start + count - 1));
                }
            }
        }
    }

    /// Line `line` of `file` was added or changed.
    pub fn contains(&self, file: &str, line: u32) -> bool {
        let p = canonical(Path::new(file));
        self.whole.contains(&p) || self.ranges.get(&p).is_some_and(|r| r.iter().any(|&(a, b)| line >= a && line <= b))
    }

    /// `file` did not exist at the revision.
    pub fn is_new_file(&self, file: &str) -> bool {
        self.new_files.contains(&canonical(Path::new(file)))
    }

    /// How many files changed (touched in the diff, or untracked).
    pub fn file_count(&self) -> usize {
        self.ranges.keys().chain(self.whole.iter()).collect::<HashSet<_>>().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_hunks_and_new_files() {
        let root = Path::new("/nonexistent-root");
        let diff = "diff --git a/tb.sv b/tb.sv\n--- a/tb.sv\n+++ b/tb.sv\n@@ -4,0 +5 @@\n+x\n@@ -9,2 +10,3 @@\n\
                    --- comment removed\n+++ added\ndiff --git a/new.sv b/new.sv\n--- /dev/null\n+++ b/new.sv\n\
                    @@ -0,0 +1,2 @@\ndiff --git a/gone.sv b/gone.sv\n--- a/gone.sv\n+++ /dev/null\n@@ -1 +0,0 @@\n";
        let mut c = ChangedLines::default();
        c.parse_diff(diff, root);
        let tb = "/nonexistent-root/tb.sv";
        assert!(!c.contains(tb, 4));
        assert!(c.contains(tb, 5));
        assert!(c.contains(tb, 10) && c.contains(tb, 12) && !c.contains(tb, 13));
        assert!(c.is_new_file("/nonexistent-root/new.sv"));
        assert!(!c.is_new_file(tb));
        assert_eq!(c.file_count(), 2);
    }
}
