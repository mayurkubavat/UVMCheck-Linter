//! `define macros, user types, imports and the package import graph read off
//! the source text, and the UVM API pack of the compiled UVM version.
//!
//! The C++ rules read slang's macro list and directive trivia; here each
//! file of a tree is lexed again and the preprocessor's skipped ranges say
//! which directives were live.

use std::collections::{HashMap, HashSet};

use dv_model::{Design, Loc};

use sv_lexer::Kind;

use sv_syntax::{NodeId, Origin, Tree};

use crate::support::class_roles::{key, role, user_classes, Role};

use crate::syntax::Cst;



use sv_syntax::SyntaxKind::*;

pub(crate) fn base_name(p: &str) -> &str {
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}

/// One lexed file of a tree: its tokens without trivia (kind, start,
/// length, whether a line break came before it).
pub(crate) struct Lexed<'s> {
    pub(crate) src: &'s str,
    pub(crate) toks: std::sync::Arc<Vec<(Kind, u32, u32, bool)>>,
}

impl<'s> Lexed<'s> {
    pub(crate) fn new(src: &'s str) -> Self {
        let mut toks = Vec::new();
        let mut nl = true;
        let mut cont = false;
        for t in sv_lexer::lex(src) {
            match t.kind {
                Kind::LineContinuation => cont = true,
                Kind::Newline => {
                    nl |= !cont;
                    cont = false;
                }
                Kind::BlockComment if t.text(src).contains('\n') => nl = true,
                k if k.is_trivia() => {}
                k => {
                    toks.push((k, t.start, t.len, nl));
                    nl = false;
                    cont = false;
                }
            }
        }
        Lexed { src, toks: std::sync::Arc::new(toks) }
    }

    pub(crate) fn text(&self, i: usize) -> &'s str {
        let (_, s, l, _) = self.toks[i];
        &self.src[s as usize..(s + l) as usize]
    }

    /// The tokens after `i` on the same logical line (line continuations
    /// join lines).
    pub(crate) fn rest_of_line(&self, i: usize) -> std::ops::Range<usize> {
        let mut j = i + 1;
        while j < self.toks.len() && !self.toks[j].3 {
            j += 1;
        }
        i + 1..j
    }
}

/// The files of a tree, lexed, in the order the preprocessor read them:
/// whether each is library code (named uvm_* or included from such a file)
/// and the macros defined at the end (slang's `getDefinedMacros`).
pub(crate) struct Files<'t> {
    pub(crate) tree: &'t Tree,
    pub(crate) lib: Vec<bool>,
    pub(crate) lexed: Vec<Lexed<'t>>,
    /// Name -> (file, index of the `define token).
    pub(crate) macros: HashMap<String, (u32, usize)>,
}

/// [`Files`] without its sources, shared by the rules that read it.
pub(crate) struct FilesData {
    pub(crate) toks: Vec<std::sync::Arc<Vec<(Kind, u32, u32, bool)>>>,
    pub(crate) lib: Vec<bool>,
    pub(crate) macros: HashMap<String, (u32, usize)>,
}

impl<'t> Files<'t> {
    /// The tree's files, lexed once per tree.
    pub(crate) fn new(c: &crate::syntax::Cst<'t>) -> Self {
        let d = crate::shared::get("meth_files", c.tree_key(), || {
            let f = Files::build(c.tree);
            FilesData { toks: f.lexed.iter().map(|l| l.toks.clone()).collect(), lib: f.lib, macros: f.macros }
        });
        let lexed = c.tree.sources.iter().zip(&d.toks).map(|(s, t)| Lexed { src: s, toks: t.clone() }).collect();
        Files { tree: c.tree, lib: d.lib.clone(), lexed, macros: d.macros.clone() }
    }

    pub(crate) fn build(tree: &'t Tree) -> Self {
        let lexed: Vec<Lexed> = tree.sources.iter().map(|s| Lexed::new(s)).collect();
        let mut fs = Files { tree, lib: vec![false; lexed.len()], lexed, macros: HashMap::new() };
        if !fs.lexed.is_empty() {
            let mut next = 1;
            fs.walk(0, false, &mut next, 0);
        }
        fs
    }

    /// Replays the live directives of file `f`: each `include reads the
    /// next file of the tree (every inclusion is a file of its own).
    pub(crate) fn walk(&mut self, f: usize, lib: bool, next: &mut usize, depth: usize) {
        let path = self.tree.files[f].to_string_lossy().into_owned();
        self.lib[f] = lib || base_name(&path).starts_with("uvm_");
        if depth > 64 {
            return;
        }
        for i in 0..self.lexed[f].toks.len() {
            let lx = &self.lexed[f];
            if lx.toks[i].0 != Kind::Directive || !live(self.tree, f as u32, lx.toks[i].1) {
                continue;
            }
            let line = lx.rest_of_line(i);
            match lx.text(i) {
                "`define" if !line.is_empty() => {
                    let name = lx.text(line.start);
                    if DIRECTIVES.contains(&name) {
                        continue;
                    }
                    self.macros.insert(name.to_string(), (f as u32, i));
                }
                "`undef" if !line.is_empty() => {
                    self.macros.remove(lx.text(line.start));
                }
                "`undefineall" => self.macros.clear(),
                "`include" if !line.is_empty() => {
                    let first = line.start;
                    let name = match (lx.toks[first].0, lx.text(first)) {
                        (Kind::StringLiteral, t) => Some(t.trim_matches('"').to_string()),
                        (_, "<") => {
                            Some(line.clone().skip(1).map(|j| lx.text(j)).take_while(|t| *t != ">").collect::<String>())
                        }
                        (Kind::Directive, _) => None,
                        _ => continue,
                    };
                    if *next >= self.lexed.len() {
                        continue;
                    }
                    let g = *next;
                    let gp = self.tree.files[g].to_string_lossy().into_owned();
                    if let Some(n) = name {
                        if gp != n && !gp.ends_with(&format!("/{n}")) {
                            continue;
                        }
                    }
                    *next += 1;
                    let l = self.lib[f];
                    self.walk(g, l, next, depth + 1);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn path(&self, f: u32) -> String {
        self.tree.files[f as usize].to_string_lossy().into_owned()
    }

    pub(crate) fn user(&self, f: u32) -> bool {
        !self.lib[f as usize]
    }
}

/// A directive at this offset was seen by the preprocessor: it is not inside
/// a branch that was skipped (the directive that starts skipping is live).
pub(crate) fn live(tree: &Tree, f: u32, off: u32) -> bool {
    !tree.skipped.iter().any(|&(sf, s, e)| sf == f && s < off && off < e)
}

/// Compiler directive names, which cannot be defined as macros.
pub(crate) const DIRECTIVES: &[&str] = &[
    "define", "undef", "undefineall", "ifdef", "ifndef", "elsif", "else", "endif", "include", "timescale", "default_nettype",
    "resetall", "celldefine", "endcelldefine", "unconnected_drive", "nounconnected_drive", "line", "pragma",
    "begin_keywords", "end_keywords", "__FILE__", "__LINE__", "default_decay_time", "default_trireg_strength",
    "delay_mode_distributed", "delay_mode_path", "delay_mode_unit", "delay_mode_zero", "protect", "endprotect",
];

/// A token of a tree written in a user file (not made by a macro).
pub(crate) fn user_tok(c: &Cst, fs: &Files, t: u32) -> bool {
    let tok = &c.tree.tokens[t as usize];
    !tok.from_macro && fs.user(tok.origin.file)
}

pub(crate) fn origin_loc(c: &Cst, f: u32, offset: u32) -> Loc {
    c.loc_at(Origin { file: f, offset })
}

pub(crate) struct Define {
    pub(crate) tree: usize,
    pub(crate) file: u32,
    /// Offsets of the backtick and of the name.
    pub(crate) at: u32,
    pub(crate) name_at: u32,
    pub(crate) name: String,
    pub(crate) formals: Option<Vec<String>>,
    pub(crate) body: Vec<(Kind, String)>,
}

/// Each macro defined in user code once, even when several trees include
/// its header.
pub(crate) fn user_defines(csts: &[Cst]) -> Vec<Define> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (ti, c) in csts.iter().enumerate() {
        let fs = Files::new(c);
        let mut defs: Vec<_> = fs.macros.values().copied().collect();
        defs.sort();
        for (f, i) in defs {
            if !fs.user(f) {
                continue;
            }
            let lx = &fs.lexed[f as usize];
            let line = lx.rest_of_line(i);
            let n = line.start;
            let name = lx.text(n).to_string();
            if name.starts_with("uvm_") || name.starts_with("UVM_") {
                continue;
            }
            let name_at = lx.toks[n].1;
            if !seen.insert((fs.path(f), name_at)) {
                continue;
            }
            let mut j = n + 1;
            let mut formals = None;
            let name_end = lx.toks[n].1 + lx.toks[n].2;
            if j < line.end && lx.text(j) == "(" && lx.toks[j].1 == name_end {
                let mut names = Vec::new();
                let mut depth = 0;
                let mut expect = false;
                while j < line.end {
                    let t = lx.text(j);
                    match t {
                        "(" | "[" | "{" => {
                            depth += 1;
                            expect = depth == 1;
                        }
                        ")" | "]" | "}" => depth -= 1,
                        "," if depth == 1 => expect = true,
                        _ if expect => {
                            names.push(t.to_string());
                            expect = false;
                        }
                        _ => {}
                    }
                    j += 1;
                    if depth == 0 {
                        break;
                    }
                }
                formals = Some(names);
            }
            let body = (j..line.end).map(|k| (lx.toks[k].0, lx.text(k).to_string())).collect();
            out.push(Define { tree: ti, file: f, at: lx.toks[i].1, name_at, name, formals, body });
        }
    }
    out
}

/// `ifndef NAME / `define NAME ... / `endif: a default the build may
/// override with +define+NAME=...
pub(crate) fn is_overridable_default(src: &str, d: &Define) -> bool {
    let before = &src[..d.at as usize];
    let trimmed = before.trim_end_matches([' ', '\t', '\r', '\n']);
    if trimmed.is_empty() {
        return false;
    }
    let line = &trimmed[trimmed.rfind('\n').map_or(0, |i| i + 1)..];
    let line = line.trim_start_matches([' ', '\t']);
    let Some(rest) = line.strip_prefix("`ifndef") else { return false };
    let rest = rest.trim_start_matches([' ', '\t']);
    let Some(after) = rest.strip_prefix(d.name.as_str()) else { return false };
    !after.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_')
}

pub(crate) fn define_src<'a>(csts: &'a [Cst], d: &Define) -> &'a str {
    &csts[d.tree].tree.sources[d.file as usize]
}

#[derive(PartialEq)]
pub(crate) enum Container {
    None,
    Class,
    Package,
    Interface,
    Module,
}

pub(crate) fn container_of(c: &Cst, n: NodeId) -> Container {
    let mut p = c.parent(n);
    while let Some(x) = p {
        match c.kind(x) {
            ClassDecl => return Container::Class,
            PackageDecl => return Container::Package,
            InterfaceDecl => return Container::Interface,
            ModuleDecl | ProgramDecl => return Container::Module,
            _ => {}
        }
        p = c.parent(x);
    }
    Container::None
}

/// Type nodes that start with this keyword, written in user files.
/// A cast's type (`integer'(x)`) is a name here.
pub(crate) fn user_types(c: &Cst, fs: &Files, kw: &str) -> Vec<(NodeId, u32)> {
    c.nodes_of(&[DataType, NameExpr])
        .into_iter()
        .filter(|&n| c.kind(n) == DataType || c.parent(n).is_some_and(|p| c.kind(p) == CastExpr))
        .filter_map(|n| {
            let t = c.keyword(n)?;
            (c.text_of(t) == kw && c.first_tok(n) == Some(t) && user_tok(c, fs, t)).then_some((n, t))
        })
        .collect()
}

/// The items of an import declaration: (package name token, item token).
pub(crate) fn import_items(c: &Cst, n: NodeId) -> Vec<(u32, u32)> {
    let toks: Vec<u32> = c.toks(n).collect();
    let mut out = Vec::new();
    let mut i = 1;
    while i + 2 < toks.len() {
        if c.text_of(toks[i + 1]) == "::" {
            out.push((toks[i], toks[i + 2]));
            i += 3;
        } else {
            i += 1;
        }
        if i < toks.len() && c.text_of(toks[i]) == "," {
            i += 1;
        }
    }
    out
}

pub(crate) struct PackageGraph {
    /// (from, to) and where the first import that makes the edge is.
    pub(crate) edges: Vec<((String, String), Loc)>,
    pub(crate) afferent: HashMap<String, usize>,
    pub(crate) efferent: HashMap<String, usize>,
    /// Highest testbench layer of the classes each package defines.
    pub(crate) layers: HashMap<String, i32>,
}

impl PackageGraph {
    pub(crate) fn instability(&self, p: &str) -> f64 {
        let ca = self.afferent.get(p).copied().unwrap_or(0);
        let ce = self.efferent.get(p).copied().unwrap_or(0);
        if ca + ce == 0 {
            0.0
        } else {
            ce as f64 / (ca + ce) as f64
        }
    }
}

pub(crate) fn layer_of(r: Role) -> i32 {
    match r {
        Role::Driver | Role::Monitor | Role::Sequencer | Role::Agent => 1,
        Role::Env | Role::Scoreboard => 2,
        Role::Test => 3,
        _ => 0,
    }
}

pub(crate) fn package_graph(d: &Design, csts: &[Cst]) -> PackageGraph {
    let classes: HashMap<_, _> = user_classes(d).map(|k| (key(k.loc), k)).collect();
    // User packages: the first definition of each name decides.
    let mut packages: HashMap<String, bool> = HashMap::new();
    let mut layers: HashMap<String, i32> = HashMap::new();
    let mut imports: Vec<(String, String, Loc)> = Vec::new();
    for c in csts {
        let fs = Files::new(c);
        for p in c.nodes(PackageDecl) {
            let Some(name) = c.tree.child(p, Name).map(|n| c.tree.bare_text(n)) else { continue };
            let user = c.first_tok(p).is_some_and(|t| user_tok(c, &fs, t)) && !name.starts_with("uvm_");
            packages.entry(name.clone()).or_insert(user);
            for k in c.tree.children(p) {
                match c.kind(k) {
                    ClassDecl => {
                        let Some((_, nt)) = c.tree.child(k, Name).and_then(|n| c.tree.token_bounds(n)) else { continue };
                        if let Some(cls) = classes.get(&key(c.loc(nt))) {
                            let l = layers.entry(name.clone()).or_insert(0);
                            *l = (*l).max(layer_of(role(d, cls)));
                        }
                    }
                    ImportDecl => {
                        for (pt, item) in import_items(c, k) {
                            if user_tok(c, &fs, pt) {
                                imports.push((name.clone(), c.text_of(pt).to_string(), c.loc(item)));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let user = |p: &str| packages.get(p).copied().unwrap_or(false);
    let mut g = PackageGraph { edges: Vec::new(), afferent: HashMap::new(), efferent: HashMap::new(), layers };
    let mut have = HashSet::new();
    for (from, to, loc) in imports {
        if from == to || !user(&from) || !user(&to) || !have.insert((from.clone(), to.clone())) {
            continue;
        }
        *g.efferent.entry(from.clone()).or_default() += 1;
        *g.afferent.entry(to.clone()).or_default() += 1;
        g.edges.push(((from, to), loc));
    }
    g
}

/// The UVM version string of the compile: `uvm_pkg::UVM_VERSION_STRING`,
/// else `uvm_pkg::uvm_revision` (the string they are set to).
pub(crate) fn uvm_version(csts: &[Cst]) -> Option<String> {
    for want in ["UVM_VERSION_STRING", "uvm_revision"] {
        for c in csts {
            for d in c.nodes(ParamDecl) {
                if !c.toks(d).any(|t| c.text_of(t) == want) {
                    continue;
                }
                let in_uvm = crate::support::slang_ast::ancestors(c, d)
                    .any(|a| c.kind(a) == PackageDecl && crate::support::slang_ast::decl_name(c, a) == "uvm_pkg");
                if !in_uvm {
                    continue;
                }
                if let Some(t) = c.toks(d).find(|&t| c.tree.tokens[t as usize].kind == Kind::StringLiteral) {
                    return Some(c.text_of(t).trim_matches('"').to_string());
                }
            }
        }
    }
    None
}

pub(crate) fn api_pack(csts: &[Cst]) -> Option<&'static crate::uvm_api::ApiPack> {
    crate::uvm_api::for_version(uvm_version(csts).as_deref())
}
