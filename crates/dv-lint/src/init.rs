//! `--init`: sets up a project folder as uvmcheck's ProjectInit does. It
//! finds the SystemVerilog sources (top-level files nothing `includes, or
//! readable `.f` file lists nothing else names), the include folders the
//! includes need, and the UVM library (in the tree, else `$UVM_HOME`); it
//! learns the naming conventions the UVM classes follow where most of them
//! do not follow the defaults; and it writes `.uvmcheck/config.py` (the
//! caller then writes a first baseline with it).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dv_model::Design;

const MAX_FILES: usize = 50_000;

/// What `--init` found. The `*_rel` lists are relative to the root (or
/// start with `$UVM_HOME`) and go into the config.
#[derive(Default, Debug)]
pub struct Discovery {
    pub root: PathBuf,
    pub sources_rel: Vec<String>,
    pub filelists_rel: Vec<String>,
    pub include_dirs_rel: Vec<String>,
    pub library_rel: Vec<String>,
    /// File lists found but not readable as they are.
    pub unused_filelists: Vec<String>,
    /// Where UVM came from, for the config comment.
    pub uvm_note: String,
    pub sv_files: usize,
}

/// A naming target most of the code does not follow by default.
#[derive(Debug)]
pub struct NamingChoice {
    pub target: String,
    /// The learnt pattern; empty when the target is turned off.
    pub pattern: String,
    pub disable: bool,
    pub evidence: String,
}

fn skip_dir(name: &str) -> bool {
    const SKIP: [&str; 10] = [
        "build",
        "node_modules",
        "work",
        "sim_build",
        "xcelium.d",
        "csrc",
        "simv.daidir",
        "__pycache__",
        "INCA_libs",
        "obj_dir",
    ];
    // .git, .uvmcheck, editor folders.
    name.starts_with('.') || SKIP.contains(&name) || name.starts_with("build-") || name.starts_with("cmake-build")
}

fn rel(p: &Path, root: &Path) -> String {
    match p.strip_prefix(root) {
        Ok(r) if !r.as_os_str().is_empty() => r.to_string_lossy().replace('\\', "/"),
        Ok(_) => ".".into(),
        Err(_) => p.to_string_lossy().replace('\\', "/"),
    }
}

fn walk(dir: &Path, sv: &mut Vec<PathBuf>, lists: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if sv.len() >= MAX_FILES {
            return;
        }
        if p.is_dir() {
            if !p.file_name().is_some_and(|n| skip_dir(&n.to_string_lossy())) {
                walk(&p, sv, lists);
            }
            continue;
        }
        match p.extension().and_then(|e| e.to_str()) {
            Some("sv" | "svh" | "v" | "vh") => sv.push(p),
            Some("f") => lists.push(p),
            _ => {}
        }
    }
}

/// The `` `include "x" `` names in `text`.
fn includes(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("`include") {
        rest = &rest[at + "`include".len()..];
        let t = rest.trim_start();
        if let Some(body) = t.strip_prefix('"') {
            if let Some(end) = body.find('"') {
                out.push(&body[..end]);
            }
        }
    }
    out
}

/// A file list the native front end reads (relative to itself, as the
/// config's file lists are read) and that names a source.
fn usable_filelist(f: &Path) -> bool {
    let l = f.display().to_string();
    let cwd = f.parent().unwrap_or(Path::new("."));
    let fl = svlang::source::FileList::read_args(
        ["-F", l.as_str()],
        cwd,
        svlang::source::Dialect::Any,
        &svlang::source::ProcessEnv,
    );
    !fl.has_errors() && !fl.sources.is_empty()
}

/// Scans `root`; `cli_files`, when given, are the sources instead.
pub fn discover(root: &Path, cli_files: &[PathBuf]) -> Discovery {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut d = Discovery { root: root.clone(), ..Discovery::default() };
    let (mut sv, mut lists) = (Vec::new(), Vec::new());
    walk(&root, &mut sv, &mut lists);
    d.sv_files = sv.len();
    let uvm_src = sv.iter().find(|f| f.file_name().is_some_and(|n| n == "uvm_pkg.sv")).and_then(|f| f.parent());
    let uvm_src = uvm_src.map(Path::to_path_buf);
    let (mut uvm_pkg, mut uvm_inc) = (String::new(), String::new());
    if let Some(src) = &uvm_src {
        let home = if src.file_name().is_some_and(|n| n == "src") { src.parent().unwrap_or(src) } else { src };
        uvm_pkg = rel(&src.join("uvm_pkg.sv"), &root);
        uvm_inc = rel(src, &root);
        d.library_rel.push(rel(home, &root));
        d.uvm_note = format!("UVM library found in the project at {}", rel(home, &root));
    } else if let Some(env) =
        std::env::var_os("UVM_HOME").filter(|e| !e.is_empty() && Path::new(e).join("src/uvm_pkg.sv").is_file())
    {
        uvm_pkg = "$UVM_HOME/src/uvm_pkg.sv".into();
        uvm_inc = "$UVM_HOME/src".into();
        d.library_rel.push("$UVM_HOME".into());
        d.uvm_note = format!("UVM library from $UVM_HOME ({})", env.to_string_lossy());
    } else {
        d.uvm_note = "UVM library not found: set UVM_HOME, or add its uvm_pkg.sv to sources".into();
    }
    let in_uvm = |f: &Path| uvm_src.as_ref().is_some_and(|u| f.starts_with(u));
    // Which files others include, and the folders that make the includes resolve.
    let mut by_name: BTreeMap<String, Vec<&PathBuf>> = BTreeMap::new();
    for f in &sv {
        by_name.entry(f.file_name().unwrap_or_default().to_string_lossy().into_owned()).or_default().push(f);
    }
    let mut included: BTreeSet<String> = BTreeSet::new();
    let mut inc_dirs: BTreeSet<String> = BTreeSet::new();
    for f in sv.iter().filter(|f| !in_uvm(f)) {
        let text = String::from_utf8_lossy(&std::fs::read(f).unwrap_or_default()).into_owned();
        for inc in includes(&text) {
            let inc = Path::new(inc);
            let name = inc.file_name().unwrap_or_default().to_string_lossy().into_owned();
            included.insert(name.clone());
            for cand in by_name.get(&name).into_iter().flatten().filter(|c| !in_uvm(c)) {
                let mut dir = cand.parent().map(Path::to_path_buf).unwrap_or_default();
                for _ in inc.parent().into_iter().flat_map(Path::components) {
                    dir = dir.parent().map(Path::to_path_buf).unwrap_or_default();
                }
                if Some(dir.as_path()) != f.parent() {
                    inc_dirs.insert(rel(&dir, &root));
                }
            }
        }
    }
    if !uvm_pkg.is_empty() {
        d.sources_rel.push(uvm_pkg);
    }
    if !uvm_inc.is_empty() {
        d.include_dirs_rel.push(uvm_inc.clone());
    }
    d.include_dirs_rel.extend(inc_dirs.into_iter().filter(|x| *x != uvm_inc));
    if !cli_files.is_empty() {
        let cwd = std::env::current_dir().unwrap_or_default();
        for f in cli_files {
            let abs = if f.is_relative() { cwd.join(f) } else { f.clone() };
            d.sources_rel.push(rel(&abs.canonicalize().unwrap_or(abs), &root));
        }
        return d;
    }
    // File lists no other list names are the tops; used when readable.
    let texts: Vec<String> = lists.iter().map(|f| std::fs::read_to_string(f).unwrap_or_default()).collect();
    for (i, f) in lists.iter().enumerate() {
        let name = f.file_name().unwrap_or_default().to_string_lossy();
        if texts.iter().enumerate().any(|(j, t)| j != i && t.contains(name.as_ref())) {
            continue;
        }
        if usable_filelist(f) { &mut d.filelists_rel } else { &mut d.unused_filelists }.push(rel(f, &root));
    }
    if d.filelists_rel.is_empty() {
        for f in &sv {
            let name = f.file_name().unwrap_or_default().to_string_lossy();
            let header = f.extension().is_some_and(|e| e == "svh" || e == "vh");
            if !in_uvm(f) && !included.contains(name.as_ref()) && !header {
                d.sources_rel.push(rel(f, &root));
            }
        }
    }
    d
}

impl Discovery {
    /// The compile arguments the discovered settings stand for.
    pub fn compile_args(&self) -> Vec<String> {
        let abs = |p: &str| -> String {
            let e = match p.strip_prefix("$UVM_HOME") {
                Some(rest) => format!("{}{rest}", std::env::var("UVM_HOME").unwrap_or_default()),
                None => p.to_string(),
            };
            let pb = Path::new(&e);
            if pb.is_relative() {
                self.root.join(pb).display().to_string()
            } else {
                e
            }
        };
        let mut out = Vec::new();
        for d in &self.include_dirs_rel {
            out.push(format!("+incdir+{}", abs(d)));
        }
        for f in &self.filelists_rel {
            out.extend(["-F".to_string(), abs(f)]);
        }
        out.extend(self.sources_rel.iter().map(|s| abs(s)));
        out
    }
}

/// The ending a name's convention is judged by: from the last `_` for
/// snake_case, else the last capitalised word.
fn ending(n: &str) -> String {
    if let Some(u) = n.rfind('_').filter(|&u| u > 0) {
        return n[u..].to_string();
    }
    let b = n.as_bytes();
    let mut i = b.len();
    while i > 1 && !b[i - 1].is_ascii_uppercase() {
        i -= 1;
    }
    if i > 1 {
        n[i - 1..].to_string()
    } else {
        String::new()
    }
}

/// Infers house naming conventions for the UVM class kinds of the user
/// classes (outside `library_dir`), as uvmcheck's `inferNaming`.
pub fn infer_naming(design: &Design, library_dir: Option<&Path>) -> Vec<NamingChoice> {
    use crate::rules::style::configurable_naming_engine::{class_kind, convention};
    let mut names: BTreeMap<&'static str, Vec<&str>> = BTreeMap::new();
    for c in crate::support::uvm_classes::user_classes(design) {
        if let Some(lib) = library_dir {
            let f = crate::tbgraph::where_of(design.files(), c.loc).file;
            let f = Path::new(&f).canonicalize().unwrap_or_else(|_| PathBuf::from(&f));
            if f.starts_with(lib) {
                continue;
            }
        }
        if let Some(kind) = class_kind(design, c.id).filter(|k| k.starts_with("Uvm")) {
            names.entry(kind).or_default().push(&c.name);
        }
    }
    let mut out = Vec::new();
    for (kind, list) in names {
        if list.len() < 2 {
            continue;
        }
        let Some((default, _)) = convention(kind, "") else { continue };
        let pass = list.iter().filter(|n| convention(kind, n).is_some_and(|c| c.1)).count();
        // 80% already follow the default.
        if pass * 10 >= list.len() * 8 {
            continue;
        }
        let mut endings: BTreeMap<String, usize> = BTreeMap::new();
        for n in &list {
            *endings.entry(ending(n)).or_default() += 1;
        }
        let mut sorted: Vec<(String, usize)> = endings.into_iter().collect();
        sorted.sort_by_key(|e| std::cmp::Reverse(e.1));
        let (mut chosen, mut covered) = (Vec::new(), 0);
        for (e, n) in &sorted {
            if e.is_empty() || *n < 2 || chosen.len() == 3 {
                break;
            }
            chosen.push(e.clone());
            covered += n;
            if covered * 10 >= list.len() * 8 {
                break;
            }
        }
        let fits = !chosen.is_empty() && covered * 10 >= list.len() * 8;
        let mut c =
            NamingChoice { target: kind.to_string(), pattern: String::new(), disable: !fits, evidence: String::new() };
        if fits {
            c.pattern = format!("^[A-Za-z0-9_]*({})$", chosen.join("|"));
            c.evidence = format!("{covered} of {} classes end in {}", list.len(), chosen.join(" or "));
        } else {
            let top = sorted.first().map_or_else(|| "none".to_string(), |(e, n)| format!("{e} x{n}"));
            c.evidence = format!("no common ending across {} classes (top: {top})", list.len());
        }
        c.evidence.push_str(&format!("; {pass} follow the default {default}"));
        out.push(c);
    }
    out
}

fn py_list(items: &[String], indent: &str) -> String {
    if items.is_empty() {
        return "[]".into();
    }
    let mut out = String::from("[\n");
    for i in items {
        out.push_str(&format!("{indent}    \"{i}\",\n"));
    }
    out + indent + "]"
}

/// The `.uvmcheck/config.py` text.
pub fn render_config(d: &Discovery, naming: &[NamingChoice]) -> String {
    let mut o = String::from(
        r#""""UVMCheck project configuration, written by `uvmcheck --init`.

Paths are relative to the project root (the folder holding .uvmcheck/).
See docs/guide/project_config.md for every setting.
"""
from uvmcheck.config import ProjectConfig, RuleConfig, Waiver, Severity


class Project(ProjectConfig):
"#,
    );
    o.push_str(&format!("    # {}. {} SystemVerilog files scanned.\n", d.uvm_note, d.sv_files));
    if !d.filelists_rel.is_empty() {
        o.push_str(&format!("    filelists = {}\n", py_list(&d.filelists_rel, "    ")));
    }
    for f in &d.unused_filelists {
        o.push_str(&format!("    # Found {f}, but it did not load as a file list (simulator-only options?).\n"));
    }
    o.push_str(&format!("    sources = {}\n", py_list(&d.sources_rel, "    ")));
    o.push_str(&format!("    include_dirs = {}\n", py_list(&d.include_dirs_rel, "    ")));
    o.push_str("    defines = []\n\n");
    o.push_str("    # Third-party code: compiled so the testbench elaborates, never reported.\n");
    o.push_str(&format!("    library_paths = {}\n", py_list(&d.library_rel, "    ")));
    if !naming.is_empty() {
        o.push_str("\n    # Naming conventions learnt from the code (default kept where 80% already follow it).\n");
        o.push_str("    naming = {\n");
        for n in naming {
            o.push_str(&format!("        # {}\n", n.evidence));
            if n.disable {
                o.push_str(&format!("        \"{}\": {{\"enabled\": False}},\n", n.target));
            } else {
                o.push_str(&format!(
                    "        \"{}\": {{\"pattern\": r\"{}\", \"prefix\": \"\", \"suffix\": \"\"}},\n",
                    n.target, n.pattern
                ));
            }
        }
        o.push_str("    }\n");
    }
    o.push_str(
        r#"
    # The rule set: "default" finds bugs and methodology problems; "all" adds
    # style, maintainability and advisory rules. A rule configured below
    # runs whatever the preset.
    # preset = "all"

    # Per-rule settings, e.g.:
    # class MaxLineLength(RuleConfig):
    #     max_columns = 120

    waivers = [
        # Waiver("UVM-OBJ-002", path="tb/stress_seq.sv", line=142,
        #        reason="...", owner="you", expires="2027-06-30"),
    ]
"#,
    );
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endings_as_uvmcheck_groups_them() {
        assert_eq!(ending("axi_bfm"), "_bfm");
        assert_eq!(ending("AxiDriver"), "Driver");
        assert_eq!(ending("driver"), "");
        assert_eq!(ending("_x"), "");
    }

    #[test]
    fn finds_includes() {
        assert_eq!(includes("`include \"a.svh\"\n  `include   \"b/c.svh\" // x\n`include FOO"), ["a.svh", "b/c.svh"]);
    }
}
