//! Which compilation a header belongs to: an index from each included file
//! (`.svh`, `.vh`, ...) to the files that `` `include `` it, and from there
//! up to the compile roots (files nothing includes). A header such as
//! `my_driver.svh` is not compiled on its own: it means something only
//! inside the package that includes it, after that package's imports and
//! the defines before the include. Tools given a lone header (an editor
//! opening it, `dv-lint --in-context`) compile its owner instead and keep
//! what concerns the header.
//!
//! The index reads `` `include "name" `` lines as text, without
//! preprocessing: an include under an inactive `` `ifdef `` still counts,
//! which is what an editor wants. A name resolves as a compiler does,
//! against the including file's folder and then the include directories;
//! one that does not resolve matches any indexed file whose path ends
//! with it.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// Extensions of the files the index reads.
pub const SV_EXTENSIONS: &[&str] = &["sv", "svh", "v", "vh", "svi", "svp", "inc"];

/// True for files that are usually included rather than compiled.
pub fn is_header(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()), Some("svh" | "vh" | "svi" | "inc"))
}

/// Where a header is compiled: a root that includes it (maybe through
/// other headers), the package the root's include sits in, and the
/// folders on the way (each including file's), which a compile of the root
/// needs as include directories.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner {
    /// Top-level source file that transitively includes the header and is not included by any indexed file.
    pub root: PathBuf,
    /// Package open in `root` at the `` `include `` site that begins the chain, if any.
    pub package: Option<String>,
    /// From the header up to the root, not counting either.
    pub chain: Vec<PathBuf>,
    /// Parent directories of each file along the include chain from the header up to `root`.
    pub include_dirs: Vec<PathBuf>,
}

/// Reverse `` `include `` graph mapping included headers back to their enclosing source roots.
#[derive(Default)]
pub struct IncludeIndex {
    /// Included file -> (including file, package around the include).
    includers: HashMap<PathBuf, Vec<(PathBuf, Option<String>)>>,
    files: Vec<PathBuf>,
}

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// `` `include "name" `` targets of `text`, each with the package open at
/// that point (`package p;` ... `endpackage`), comments skipped.
pub fn includes(text: &str) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut package: Option<String> = None;
    let mut in_block = false;
    for line in text.lines() {
        let mut l = line;
        if in_block {
            match l.find("*/") {
                Some(i) => {
                    l = &l[i + 2..];
                    in_block = false;
                }
                None => continue,
            }
        }
        let l = match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        };
        let l = match l.find("/*") {
            Some(i) => {
                if !l[i..].contains("*/") {
                    in_block = true;
                }
                &l[..i]
            }
            None => l,
        };
        let words: Vec<&str> = l.split(|c: char| c.is_whitespace() || c == ';').filter(|w| !w.is_empty()).collect();
        for (i, w) in words.iter().enumerate() {
            match *w {
                "package" => package = words.get(i + 1).map(|s| s.to_string()),
                "endpackage" => package = None,
                _ => {}
            }
        }
        if let Some(i) = l.find("`include") {
            let rest = l[i + 8..].trim_start();
            let name = if let Some(r) = rest.strip_prefix('"') {
                r.split('"').next()
            } else if let Some(r) = rest.strip_prefix('<') {
                r.split('>').next()
            } else {
                None
            };
            if let Some(n) = name.filter(|n| !n.is_empty()) {
                out.push((n.to_string(), package.clone()));
            }
        }
    }
    out
}

impl IncludeIndex {
    /// Indexes every SystemVerilog file under `roots` (recursively, skipping
    /// hidden folders), resolving includes against each file's folder and
    /// then `include_dirs`.
    pub fn scan(roots: &[PathBuf], include_dirs: &[PathBuf]) -> IncludeIndex {
        let mut files = Vec::new();
        let mut seen = HashSet::new();
        for r in roots {
            walk(r, &mut files, &mut seen, 0);
        }
        let mut idx = IncludeIndex { includers: HashMap::new(), files: files.clone() };
        let by_suffix: Vec<(String, PathBuf)> =
            files.iter().map(|f| (f.to_string_lossy().replace('\\', "/"), f.clone())).collect();
        for f in &files {
            let Ok(bytes) = std::fs::read(f) else { continue };
            let text = String::from_utf8_lossy(&bytes);
            if !text.contains("`include") {
                continue;
            }
            let dir = f.parent().map(Path::to_path_buf).unwrap_or_default();
            for (name, pkg) in includes(&text) {
                let found = std::iter::once(&dir)
                    .chain(include_dirs.iter())
                    .map(|d| d.join(&name))
                    .find(|p| p.is_file())
                    .map(|p| canon(&p))
                    .or_else(|| {
                        let tail = format!("/{}", name.trim_start_matches("./"));
                        by_suffix.iter().find(|(s, _)| s.ends_with(&tail)).map(|(_, p)| p.clone())
                    });
                if let Some(h) = found {
                    idx.includers.entry(h).or_default().push((f.clone(), pkg));
                }
            }
        }
        idx
    }

    /// Every indexed file.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// The files that include `header` directly.
    pub fn includers(&self, header: &Path) -> &[(PathBuf, Option<String>)] {
        self.includers.get(&canon(header)).map_or(&[], Vec::as_slice)
    }

    /// The compile roots `header` belongs to, nearest first: roots that put
    /// it in a package come before the others.
    pub fn owners(&self, header: &Path) -> Vec<Owner> {
        let start = canon(header);
        let mut out = Vec::new();
        // (file, chain from the header, package seen on the way)
        let mut queue: VecDeque<(PathBuf, Vec<PathBuf>, Option<String>)> =
            VecDeque::from([(start.clone(), Vec::new(), None)]);
        let mut seen = HashSet::from([start.clone()]);
        while let Some((f, chain, pkg)) = queue.pop_front() {
            let up = self.includers.get(&f).map_or(&[][..], Vec::as_slice);
            if up.is_empty() && f != start {
                let mut dirs: Vec<PathBuf> = Vec::new();
                for p in std::iter::once(&start).chain(chain.iter()) {
                    if let Some(d) = p.parent() {
                        if !dirs.iter().any(|x| x == d) {
                            dirs.push(d.to_path_buf());
                        }
                    }
                }
                let mut chain = chain;
                chain.pop();
                out.push(Owner { root: f, package: pkg, chain, include_dirs: dirs });
                continue;
            }
            for (by, p) in up {
                if seen.insert(by.clone()) {
                    let mut c = chain.clone();
                    c.push(by.clone());
                    queue.push_back((by.clone(), c, p.clone().or_else(|| pkg.clone())));
                }
            }
        }
        out.sort_by_key(|o| (o.package.is_none(), o.chain.len(), o.root.clone()));
        out
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, depth: usize) {
    if depth > 24 || !seen.insert(canon(dir)) {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        let hidden = p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'));
        if p.is_dir() {
            if !hidden {
                walk(&p, out, seen, depth + 1);
            }
        } else if p.extension().and_then(|e| e.to_str()).is_some_and(|e| SV_EXTENSIONS.contains(&e)) {
            out.push(canon(&p));
        }
    }
}

/// The folder to index for a header: the nearest one above it holding a
/// `.git` (the repository), else its folder's parent.
pub fn search_root(header: &Path) -> PathBuf {
    let h = canon(header);
    let mut d = h.parent();
    while let Some(x) = d {
        if x.join(".git").exists() {
            return x.to_path_buf();
        }
        d = x.parent();
    }
    h.parent().and_then(Path::parent).or(h.parent()).map(Path::to_path_buf).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_header_finds_its_package() {
        let d = std::env::temp_dir().join(format!("sv-source-owners-{}", std::process::id()));
        fs::remove_dir_all(&d).ok();
        fs::create_dir_all(d.join("agent/inc")).unwrap();
        fs::write(d.join("agent/agent_pkg.sv"), "package agent_pkg;\n  import uvm_pkg::*;\n  `include \"agent_items.svh\"\n  // `include \"gone.svh\"\nendpackage\n").unwrap();
        fs::write(d.join("agent/agent_items.svh"), "`include \"inc/drv.svh\"\n").unwrap();
        fs::write(d.join("agent/inc/drv.svh"), "class drv; endclass\n").unwrap();
        fs::write(d.join("tb.sv"), "`include \"agent/inc/drv.svh\"\nmodule tb; endmodule\n").unwrap();
        let idx = IncludeIndex::scan(std::slice::from_ref(&d), &[]);
        let o = idx.owners(&d.join("agent/inc/drv.svh"));
        assert_eq!(o.len(), 2, "{o:?}");
        assert_eq!(o[0].root, canon(&d.join("agent/agent_pkg.sv")));
        assert_eq!(o[0].package.as_deref(), Some("agent_pkg"));
        assert_eq!(o[0].chain, [canon(&d.join("agent/agent_items.svh"))]);
        assert_eq!(o[0].include_dirs, [canon(&d.join("agent/inc")), canon(&d.join("agent"))]);
        assert_eq!(o[1].root, canon(&d.join("tb.sv")));
        assert_eq!(o[1].package, None);
        // A root owns nothing above it.
        assert!(idx.owners(&d.join("tb.sv")).is_empty());
        fs::remove_dir_all(&d).ok();
    }
}
