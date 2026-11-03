//! Simulator file lists, read the way VCS and Xcelium read them.
//!
//! [`FileList::read`] follows `-f`/`-F` nesting, expands `$VAR`, `${VAR}` and
//! `$(VAR)`, and sorts every token into sources, include directories,
//! defines, library directories and files, and simulator options. Every item
//! keeps where it came from, and every problem (a missing file, an undefined
//! variable, a list that includes itself) is reported with that origin rather
//! than stopping the read.
//!
//! Path rules: `-f` resolves paths inside the list against the current
//! directory; `-F` resolves them against the list's own directory (both
//! simulators). Absolute paths are kept as written.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// Where a token came from: a file list and line, or the command line (line 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    pub file: Option<PathBuf>,
    pub line: u32,
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.file {
            Some(p) => write!(f, "{}:{}", p.display(), self.line),
            None => f.write_str("<command line>"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item<T> {
    pub value: T,
    pub origin: Origin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    /// Accept both simulators' spellings (the default).
    Any,
    Vcs,
    Xcelium,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub origin: Origin,
}

/// What the simulator is asked to do about UVM.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UvmRequest {
    /// `-uvm` (Xcelium) or `-ntb_opts uvm*` (VCS).
    pub requested: bool,
    /// `-uvmhome <dir>` (Xcelium), or the version in `-ntb_opts uvm-1.2`.
    pub home_or_version: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct FileList {
    pub sources: Vec<Item<PathBuf>>,
    pub include_dirs: Vec<Item<PathBuf>>,
    /// Name and optional value, in order; a later define wins.
    pub defines: Vec<Item<(String, Option<String>)>>,
    pub undefines: Vec<Item<String>>,
    /// `-y <dir>`: modules are looked up there by name.
    pub library_dirs: Vec<Item<PathBuf>>,
    /// `-v <file>`: a library file, read only for what it defines.
    pub library_files: Vec<Item<PathBuf>>,
    /// `+libext+.v+.sv`, `-libext .sv`: extensions tried in library dirs.
    pub library_exts: Vec<String>,
    pub top: Vec<Item<String>>,
    pub timescale: Option<Item<String>>,
    pub uvm: UvmRequest,
    /// Simulator options with no meaning to a front end (`-l run.log`,
    /// `+acc`, `-debug_access+all`), kept for reports and round trips.
    pub ignored: Vec<Item<String>>,
    /// Every file list read, in order.
    pub lists: Vec<PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Options that take the next token as their value and mean nothing to a
/// front end. Matched case-sensitively, as the simulators do.
const SKIP_WITH_VALUE: &[&str] = &[
    "-l", "-o", "-log", "-logfile", "-work", "-access", "-cm", "-cm_dir", "-cm_name", "-licqueue_timeout",
    "-xmlibdirname", "-xmlibdirpath", "-input", "-ucli", "-do", "-gui_config", "-assert", "-covoverwrite",
    "-covfile", "-covtest", "-covworkdir", "-seed", "-svseed", "-snapshot", "-reflib", "-makelib", "-lib",
    "-Mdir", "-P", "-ntb_define", "-ntb_incdir", "-errormax", "-timeout", "-profile",
];

/// Variables for `$VAR` expansion; the process environment by default.
pub trait Env {
    fn get(&self, name: &str) -> Option<String>;
}

pub struct ProcessEnv;

impl Env for ProcessEnv {
    fn get(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

impl Env for BTreeMap<String, String> {
    fn get(&self, name: &str) -> Option<String> {
        BTreeMap::get(self, name).cloned()
    }
}

struct Token {
    text: String,
    line: u32,
}

/// Splits a file list into tokens: whitespace separated, `"..."` and `'...'`
/// quoting, `//` and `#` line comments, `/* */` block comments.
fn tokenize(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut cur_line = 1;
    let mut line = 1u32;
    let mut chars = text.chars().peekable();
    let mut quote: Option<char> = None;
    let flush = |cur: &mut String, out: &mut Vec<Token>, l: u32| {
        if !cur.is_empty() {
            out.push(Token { text: std::mem::take(cur), line: l });
        }
    };
    while let Some(ch) = chars.next() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                if ch == '\n' {
                    line += 1;
                }
                cur.push(ch);
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                if cur.is_empty() {
                    cur_line = line;
                }
                quote = Some(ch);
            }
            '\n' => {
                flush(&mut cur, &mut out, cur_line);
                line += 1;
            }
            c if c.is_whitespace() => flush(&mut cur, &mut out, cur_line),
            '/' if cur.is_empty() && chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        line += 1;
                        break;
                    }
                }
            }
            '/' if cur.is_empty() && chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = ' ';
                for c in chars.by_ref() {
                    if c == '\n' {
                        line += 1;
                    }
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
            }
            '#' if cur.is_empty() => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        line += 1;
                        break;
                    }
                }
            }
            c => {
                if cur.is_empty() {
                    cur_line = line;
                }
                cur.push(c);
            }
        }
    }
    flush(&mut cur, &mut out, cur_line);
    out
}

/// Expands `$VAR`, `${VAR}` and `$(VAR)`; returns the names it could not find.
fn expand(text: &str, env: &dyn Env) -> (String, Vec<String>) {
    let mut out = String::with_capacity(text.len());
    let mut missing = Vec::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != '$' || i + 1 >= bytes.len() {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let (name, next) = match bytes[i + 1] {
            open @ ('{' | '(') => {
                let close = if open == '{' { '}' } else { ')' };
                match bytes[i + 2..].iter().position(|&c| c == close) {
                    Some(n) => (bytes[i + 2..i + 2 + n].iter().collect::<String>(), i + 3 + n),
                    None => {
                        out.push('$');
                        i += 1;
                        continue;
                    }
                }
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let n = bytes[i + 1..].iter().take_while(|c| c.is_ascii_alphanumeric() || **c == '_').count();
                (bytes[i + 1..i + 1 + n].iter().collect::<String>(), i + 1 + n)
            }
            _ => {
                out.push('$');
                i += 1;
                continue;
            }
        };
        match env.get(&name) {
            Some(v) => out.push_str(&v),
            None => {
                missing.push(name.clone());
                out.push_str(&format!("${{{name}}}"));
            }
        }
        i = next;
    }
    (out, missing)
}

fn split_define(d: &str) -> (String, Option<String>) {
    match d.split_once('=') {
        Some((n, v)) => (n.to_string(), Some(v.to_string())),
        None => (d.to_string(), None),
    }
}

/// A plus-option's values: `+incdir+a+b` gives ["a", "b"].
fn plus_values<'a>(tok: &'a str, prefix: &str) -> Option<Vec<&'a str>> {
    let rest = tok.strip_prefix(prefix)?;
    Some(rest.split('+').filter(|s| !s.is_empty()).collect())
}

struct Reader<'e> {
    env: &'e dyn Env,
    dialect: Dialect,
    out: FileList,
    stack: Vec<PathBuf>,
}

impl Reader<'_> {
    fn diag(&mut self, severity: Severity, message: String, origin: &Origin) {
        self.out.diagnostics.push(Diagnostic { severity, message, origin: origin.clone() });
    }

    fn path(&self, raw: &str, base: &Path) -> PathBuf {
        let p = PathBuf::from(raw);
        if p.is_absolute() {
            p
        } else {
            base.join(p)
        }
    }

    fn check_exists(&mut self, p: &Path, what: &str, origin: &Origin) {
        if !p.exists() {
            self.diag(Severity::Error, format!("{what} not found: {}", p.display()), origin);
        }
    }

    fn read_list(&mut self, list: &Path, relative_to_list: bool, cwd: &Path, origin: &Origin) {
        let canon = list.canonicalize().unwrap_or_else(|_| list.to_path_buf());
        if self.stack.contains(&canon) {
            self.diag(Severity::Error, format!("file list includes itself: {}", list.display()), origin);
            return;
        }
        let text = match std::fs::read_to_string(list) {
            Ok(t) => t,
            Err(e) => {
                self.diag(Severity::Error, format!("cannot read file list {}: {e}", list.display()), origin);
                return;
            }
        };
        self.out.lists.push(list.to_path_buf());
        self.stack.push(canon);
        let base = if relative_to_list {
            list.parent().map(Path::to_path_buf).unwrap_or_else(|| cwd.to_path_buf())
        } else {
            cwd.to_path_buf()
        };
        let tokens = tokenize(&text);
        self.process(tokens, Some(list), &base, cwd);
        self.stack.pop();
    }

    fn process(&mut self, tokens: Vec<Token>, list: Option<&Path>, base: &Path, cwd: &Path) {
        let mut i = 0;
        while i < tokens.len() {
            let origin = Origin { file: list.map(Path::to_path_buf), line: tokens[i].line };
            let (tok, missing) = expand(&tokens[i].text, self.env);
            for m in missing {
                self.diag(Severity::Error, format!("undefined variable ${m}"), &origin);
            }
            // The value of an option written as two tokens.
            let value = |i: &mut usize, me: &mut Self| -> Option<String> {
                if *i + 1 < tokens.len() {
                    *i += 1;
                    let (v, missing) = expand(&tokens[*i].text, me.env);
                    for m in missing {
                        me.diag(Severity::Error, format!("undefined variable ${m}"), &origin);
                    }
                    Some(v)
                } else {
                    me.diag(Severity::Error, format!("{tok} needs a value"), &origin);
                    None
                }
            };
            let item = |v: PathBuf| Item { value: v, origin: origin.clone() };
            let xcelium = self.dialect != Dialect::Vcs;
            let vcs = self.dialect != Dialect::Xcelium;

            if tok == "-f" || tok == "-F" {
                if let Some(v) = value(&mut i, self) {
                    let p = self.path(&v, base);
                    self.read_list(&p, tok == "-F", cwd, &origin);
                }
            } else if let Some(dirs) = plus_values(&tok, "+incdir+") {
                for d in dirs {
                    let p = self.path(d, base);
                    self.check_exists(&p, "include directory", &origin);
                    self.out.include_dirs.push(item(p));
                }
            } else if (xcelium && tok == "-incdir") || tok == "-I" {
                if let Some(v) = value(&mut i, self) {
                    let p = self.path(&v, base);
                    self.check_exists(&p, "include directory", &origin);
                    self.out.include_dirs.push(item(p));
                }
            } else if let Some(defs) = plus_values(&tok, "+define+") {
                for d in defs {
                    self.out.defines.push(Item { value: split_define(d), origin: origin.clone() });
                }
            } else if (xcelium && tok == "-define") || tok == "-D" {
                if let Some(v) = value(&mut i, self) {
                    self.out.defines.push(Item { value: split_define(&v), origin: origin.clone() });
                }
            } else if let Some(rest) = tok.strip_prefix("-D").filter(|r| !r.is_empty()) {
                self.out.defines.push(Item { value: split_define(rest), origin: origin.clone() });
            } else if let Some(names) = plus_values(&tok, "+undefine+") {
                for n in names {
                    self.out.undefines.push(Item { value: n.to_string(), origin: origin.clone() });
                }
            } else if xcelium && tok == "-undefine" {
                if let Some(v) = value(&mut i, self) {
                    self.out.undefines.push(Item { value: v, origin: origin.clone() });
                }
            } else if tok == "-y" {
                if let Some(v) = value(&mut i, self) {
                    let p = self.path(&v, base);
                    self.check_exists(&p, "library directory", &origin);
                    self.out.library_dirs.push(item(p));
                }
            } else if tok == "-v" {
                if let Some(v) = value(&mut i, self) {
                    let p = self.path(&v, base);
                    self.check_exists(&p, "library file", &origin);
                    self.out.library_files.push(item(p));
                }
            } else if let Some(exts) = plus_values(&tok, "+libext+") {
                self.out.library_exts.extend(exts.into_iter().map(str::to_string));
            } else if xcelium && tok == "-libext" {
                if let Some(v) = value(&mut i, self) {
                    self.out.library_exts.extend(v.split(',').filter(|s| !s.is_empty()).map(str::to_string));
                }
            } else if tok == "-top" || tok == "-toplevel" {
                if let Some(v) = value(&mut i, self) {
                    self.out.top.push(Item { value: v, origin: origin.clone() });
                }
            } else if let Some(ts) = tok.strip_prefix("-timescale=") {
                self.out.timescale = Some(Item { value: ts.to_string(), origin: origin.clone() });
            } else if tok == "-timescale" {
                if let Some(v) = value(&mut i, self) {
                    self.out.timescale = Some(Item { value: v, origin: origin.clone() });
                }
            } else if xcelium && tok == "-uvm" {
                self.out.uvm.requested = true;
            } else if xcelium && tok == "-uvmhome" {
                if let Some(v) = value(&mut i, self) {
                    self.out.uvm.requested = true;
                    self.out.uvm.home_or_version = Some(v);
                }
            } else if vcs && tok == "-ntb_opts" {
                if let Some(v) = value(&mut i, self) {
                    if let Some(u) = v.split('+').find(|o| o.starts_with("uvm")) {
                        self.out.uvm.requested = true;
                        if let Some(ver) = u.strip_prefix("uvm-") {
                            self.out.uvm.home_or_version = Some(ver.to_string());
                        }
                    }
                    self.out.ignored.push(Item { value: format!("-ntb_opts {v}"), origin: origin.clone() });
                }
            } else if SKIP_WITH_VALUE.contains(&tok.as_str()) {
                let v = value(&mut i, self).unwrap_or_default();
                self.out.ignored.push(Item { value: format!("{tok} {v}"), origin: origin.clone() });
            } else if tok.starts_with('-') || tok.starts_with('+') {
                self.out.ignored.push(Item { value: tok.clone(), origin: origin.clone() });
            } else {
                let p = self.path(&tok, base);
                self.check_exists(&p, "source file", &origin);
                self.out.sources.push(item(p));
            }
            i += 1;
        }
    }
}

impl FileList {
    /// Reads command-line style arguments (which may name `-f` lists),
    /// resolving relative paths against `cwd`.
    pub fn read_args<I, S>(args: I, cwd: &Path, dialect: Dialect, env: &dyn Env) -> FileList
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let tokens = args.into_iter().map(|a| Token { text: a.as_ref().to_string(), line: 0 }).collect();
        let mut r = Reader { env, dialect, out: FileList::default(), stack: Vec::new() };
        r.process(tokens, None, cwd, cwd);
        r.out
    }

    /// Reads one file list as `-f <list>` would.
    pub fn read(list: &Path, cwd: &Path, dialect: Dialect) -> FileList {
        let l = list.display().to_string();
        FileList::read_args(["-f", l.as_str()], cwd, dialect, &ProcessEnv)
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }

    /// The defines after `+undefine+` and redefinition: last value wins.
    pub fn effective_defines(&self) -> BTreeMap<String, Option<String>> {
        let mut m = BTreeMap::new();
        for d in &self.defines {
            m.insert(d.value.0.clone(), d.value.1.clone());
        }
        for u in &self.undefines {
            m.remove(&u.value);
        }
        m
    }

    /// Compile options for uvmcheck and `dv_model::Design::load`, in
    /// slang's spelling. UVM requests become uvmcheck's vendor options, which
    /// need the `filelist-dialect` override (or a compat profile) to act.
    pub fn to_compile_args(&self) -> Vec<String> {
        let mut a = Vec::new();
        for d in &self.include_dirs {
            a.push(format!("+incdir+{}", d.value.display()));
        }
        for (n, v) in self.effective_defines() {
            a.push(match v {
                Some(v) => format!("+define+{n}={v}"),
                None => format!("+define+{n}"),
            });
        }
        for d in &self.library_dirs {
            a.push("-y".into());
            a.push(d.value.display().to_string());
        }
        for f in &self.library_files {
            a.push("-v".into());
            a.push(f.value.display().to_string());
        }
        if !self.library_exts.is_empty() {
            a.push(format!("+libext+{}", self.library_exts.join("+")));
        }
        for t in &self.top {
            a.push("--top".into());
            a.push(t.value.clone());
        }
        if let Some(ts) = &self.timescale {
            a.push("--timescale".into());
            a.push(ts.value.clone());
        }
        for s in &self.sources {
            a.push(s.value.display().to_string());
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Dir(PathBuf);
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn tmp(name: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("sv-source-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, "").unwrap();
    }

    #[test]
    fn tokenizer_handles_comments_and_quotes() {
        let t: Vec<String> =
            tokenize("a.sv // x\n# y\n/* z\n */ \"b c.sv\" +define+X\n").into_iter().map(|t| t.text).collect();
        assert_eq!(t, ["a.sv", "b c.sv", "+define+X"]);
        let lines: Vec<u32> = tokenize("a\n\n/*\n*/ b").into_iter().map(|t| t.line).collect();
        assert_eq!(lines, [1, 4]);
    }

    #[test]
    fn expands_all_three_spellings() {
        let env: BTreeMap<String, String> = [("A".into(), "x".into()), ("B_2".into(), "y".into())].into();
        assert_eq!(expand("$A/${B_2}/$(A)/$$", &env).0, "x/y/x/$$");
        let (s, missing) = expand("$NOPE/a", &env);
        assert_eq!((s.as_str(), missing), ("${NOPE}/a", vec!["NOPE".to_string()]));
    }

    #[test]
    fn dash_f_and_dash_capital_f_resolve_differently() {
        let d = tmp("rel");
        let root = &d.0;
        touch(&root.join("sub/a.sv"));
        touch(&root.join("b.sv"));
        fs::write(root.join("sub/rel.f"), "a.sv\n").unwrap();
        fs::write(root.join("sub/cwd.f"), "b.sv\n").unwrap();
        fs::write(root.join("top.f"), "-F sub/rel.f\n-f sub/cwd.f\n").unwrap();
        let fl = FileList::read(&root.join("top.f"), root, Dialect::Any);
        assert!(!fl.has_errors(), "{:?}", fl.diagnostics);
        assert_eq!(fl.sources.len(), 2);
        assert_eq!(fl.sources[0].value, root.join("sub/a.sv"));
        assert_eq!(fl.sources[1].value, root.join("b.sv"));
        assert_eq!(fl.lists.len(), 3);
        assert_eq!(fl.sources[0].origin.line, 1);
    }

    #[test]
    fn sorts_options_from_both_simulators() {
        let d = tmp("opts");
        let root = &d.0;
        for p in ["inc", "inc2", "lib"] {
            fs::create_dir_all(root.join(p)).unwrap();
        }
        touch(&root.join("cells.v"));
        touch(&root.join("tb.sv"));
        let env: BTreeMap<String, String> = [("ROOT".into(), root.display().to_string())].into();
        let fl = FileList::read_args(
            [
                "+incdir+$ROOT/inc+${ROOT}/inc2", "-incdir", "inc", "+define+A=1+B", "-define", "C=x", "-DA=2",
                "+undefine+B", "-y", "lib", "-v", "cells.v", "+libext+.v+.sv", "-l", "run.log", "+acc",
                "-ntb_opts", "uvm-1.2", "-timescale=1ns/1ps", "-top", "tb", "tb.sv",
            ],
            root,
            Dialect::Any,
            &env,
        );
        assert!(!fl.has_errors(), "{:?}", fl.diagnostics);
        assert_eq!(fl.include_dirs.len(), 3);
        let defs = fl.effective_defines();
        assert_eq!(defs.get("A"), Some(&Some("2".to_string())));
        assert_eq!(defs.get("C"), Some(&Some("x".to_string())));
        assert!(!defs.contains_key("B"));
        assert_eq!(fl.library_dirs[0].value, root.join("lib"));
        assert_eq!(fl.library_files[0].value, root.join("cells.v"));
        assert_eq!(fl.library_exts, [".v", ".sv"]);
        assert_eq!(fl.uvm, UvmRequest { requested: true, home_or_version: Some("1.2".into()) });
        assert_eq!(fl.timescale.as_ref().unwrap().value, "1ns/1ps");
        assert_eq!(fl.top[0].value, "tb");
        assert_eq!(fl.sources.len(), 1);
        let ignored: Vec<&str> = fl.ignored.iter().map(|i| i.value.as_str()).collect();
        assert_eq!(ignored, ["-l run.log", "+acc", "-ntb_opts uvm-1.2"]);
        let args = fl.to_compile_args();
        assert!(args.contains(&"+define+A=2".to_string()));
        assert!(args.contains(&"+libext+.v+.sv".to_string()));
        assert_eq!(args.last().unwrap(), &root.join("tb.sv").display().to_string());
    }

    #[test]
    fn reports_problems_with_their_origin() {
        let d = tmp("bad");
        let root = &d.0;
        fs::write(root.join("loop.f"), "-f loop.f\n").unwrap();
        fs::write(root.join("top.f"), "missing.sv\n$UNSET_SV_SOURCE_VAR/x.sv\n-F loop.f\n").unwrap();
        let fl = FileList::read_args(["-F", &root.join("top.f").display().to_string()], root, Dialect::Any, &ProcessEnv);
        let msgs: Vec<String> = fl.diagnostics.iter().map(|d| format!("{} {}", d.origin.line, d.message)).collect();
        assert!(msgs.iter().any(|m| m.starts_with("1 source file not found")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.starts_with("2 undefined variable $UNSET_SV_SOURCE_VAR")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("includes itself")), "{msgs:?}");
    }

    #[test]
    fn dialect_limits_spellings() {
        let d = tmp("dialect");
        let fl = FileList::read_args(["-uvm"], &d.0, Dialect::Vcs, &ProcessEnv);
        assert!(!fl.uvm.requested);
        assert_eq!(fl.ignored[0].value, "-uvm");
        let fl = FileList::read_args(["-uvm"], &d.0, Dialect::Xcelium, &ProcessEnv);
        assert!(fl.uvm.requested);
    }
}
