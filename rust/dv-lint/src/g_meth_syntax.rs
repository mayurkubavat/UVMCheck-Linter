//! Group A construct and package rules that read the source text and the
//! syntax trees: `define constants and routines, `ifdef in classes, enum
//! base types, legacy 'integer', class file naming, wildcard imports and the
//! package import graph.
//!
//! The C++ rules read slang's macro list and directive trivia; here each
//! file of a tree is lexed again and the preprocessor's skipped ranges say
//! which directives were live.

use std::collections::{BTreeMap, HashMap, HashSet};

use dv_model::{Design, Loc};
use sv_lexer::Kind;
use sv_syntax::{NodeId, Origin, Tree};

use crate::g_methodology::{key, role, user_classes, Role};
use crate::syntax::Cst;
use crate::Check::Mixed;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-CNS-001", name: "DefineForConstant", check: Mixed(define_for_constant) },
        Rule { id: "SV-CNS-002", name: "MacroAsRoutine", check: Mixed(macro_as_routine) },
        Rule { id: "SV-CNS-003", name: "IfdefInReusableClass", check: Mixed(ifdef_in_reusable_class) },
        Rule { id: "SV-CNS-006", name: "ImplicitEnumBaseType", check: Mixed(implicit_enum_base_type) },
        Rule { id: "SV-CNS-007", name: "LegacyIntegerType", check: Mixed(legacy_integer_type) },
        Rule { id: "SV-PKG-001", name: "FileClassNameMismatch", check: Mixed(file_class_name_mismatch) },
        Rule { id: "SV-PKG-002", name: "WildcardImportOutsidePackage", check: Mixed(wildcard_import_outside_package) },
        Rule { id: "SV-PKG-004", name: "PackageLayerInversion", check: Mixed(package_layer_inversion) },
        Rule { id: "SV-PKG-005", name: "UnstablePackageDependedOn", check: Mixed(unstable_package_depended_on) },
    ]
}

// ---- files of a tree ----

fn base_name(p: &str) -> &str {
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}

/// One lexed file of a tree: its tokens without trivia (kind, start,
/// length, whether a line break came before it).
struct Lexed<'s> {
    src: &'s str,
    toks: Vec<(Kind, u32, u32, bool)>,
}

impl<'s> Lexed<'s> {
    fn new(src: &'s str) -> Self {
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
        Lexed { src, toks }
    }

    fn text(&self, i: usize) -> &'s str {
        let (_, s, l, _) = self.toks[i];
        &self.src[s as usize..(s + l) as usize]
    }

    /// The tokens after `i` on the same logical line (line continuations
    /// join lines).
    fn rest_of_line(&self, i: usize) -> std::ops::Range<usize> {
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
struct Files<'t> {
    tree: &'t Tree,
    lib: Vec<bool>,
    lexed: Vec<Lexed<'t>>,
    /// Name -> (file, index of the `define token).
    macros: HashMap<String, (u32, usize)>,
}

impl<'t> Files<'t> {
    fn new(tree: &'t Tree) -> Self {
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
    fn walk(&mut self, f: usize, lib: bool, next: &mut usize, depth: usize) {
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

    fn path(&self, f: u32) -> String {
        self.tree.files[f as usize].to_string_lossy().into_owned()
    }

    fn user(&self, f: u32) -> bool {
        !self.lib[f as usize]
    }
}

/// A directive at this offset was seen by the preprocessor: it is not inside
/// a branch that was skipped (the directive that starts skipping is live).
fn live(tree: &Tree, f: u32, off: u32) -> bool {
    !tree.skipped.iter().any(|&(sf, s, e)| sf == f && s < off && off < e)
}

/// A conditional directive (token `i` of file `f`) was seen by the
/// preprocessor. A skipped range runs from the first branch not taken, so a
/// later `elsif, `else or `endif of the same conditional inside it is
/// live; one of a conditional nested in skipped text is not.
fn live_conditional(tree: &Tree, lx: &Lexed, f: u32, i: usize) -> bool {
    let off = lx.toks[i].1;
    let closes = matches!(lx.text(i), "`elsif" | "`else" | "`endif");
    tree.skipped.iter().filter(|&&(sf, s, e)| sf == f && s < off && off < e).all(|&(_, s, _)| {
        let Ok(j) = lx.toks.binary_search_by_key(&s, |t| t.1) else { return false };
        let mut depth = 1;
        for k in j + 1..i {
            if lx.toks[k].0 != Kind::Directive {
                continue;
            }
            match lx.text(k) {
                "`ifdef" | "`ifndef" => depth += 1,
                "`endif" => depth -= 1,
                _ => {}
            }
        }
        closes && depth == 1
    })
}

/// Compiler directive names, which cannot be defined as macros.
const DIRECTIVES: &[&str] = &[
    "define", "undef", "undefineall", "ifdef", "ifndef", "elsif", "else", "endif", "include", "timescale", "default_nettype",
    "resetall", "celldefine", "endcelldefine", "unconnected_drive", "nounconnected_drive", "line", "pragma",
    "begin_keywords", "end_keywords", "__FILE__", "__LINE__", "default_decay_time", "default_trireg_strength",
    "delay_mode_distributed", "delay_mode_path", "delay_mode_unit", "delay_mode_zero", "protect", "endprotect",
];

/// A token of a tree written in a user file (not made by a macro).
fn user_tok(c: &Cst, fs: &Files, t: u32) -> bool {
    let tok = &c.tree.tokens[t as usize];
    !tok.from_macro && fs.user(tok.origin.file)
}

fn origin_loc(c: &Cst, f: u32, offset: u32) -> Loc {
    c.loc_at(Origin { file: f, offset })
}

// ---- SV-CNS-001, SV-CNS-002 ----

struct Define {
    tree: usize,
    file: u32,
    /// Offsets of the backtick and of the name.
    at: u32,
    name_at: u32,
    name: String,
    formals: Option<Vec<String>>,
    body: Vec<(Kind, String)>,
}

/// Each macro defined in user code once, even when several trees include
/// its header.
fn user_defines(csts: &[Cst]) -> Vec<Define> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (ti, c) in csts.iter().enumerate() {
        let fs = Files::new(c.tree);
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
fn is_overridable_default(src: &str, d: &Define) -> bool {
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

fn define_src<'a>(csts: &'a [Cst], d: &Define) -> &'a str {
    &csts[d.tree].tree.sources[d.file as usize]
}

fn define_for_constant(_: &Design, csts: &[Cst], out: &mut Sink) {
    for d in user_defines(csts) {
        if d.formals.is_some() || d.body.is_empty() || is_overridable_default(define_src(csts, &d), &d) {
            continue;
        }
        let (mut numbers, mut strings) = (0, 0);
        let mut constant = true;
        for (k, t) in &d.body {
            match k {
                Kind::IntegerLiteral | Kind::IntegerBase | Kind::RealLiteral | Kind::TimeLiteral => numbers += 1,
                Kind::StringLiteral | Kind::TripleQuotedString => strings += 1,
                // An unterminated string is still a string literal to slang.
                Kind::Unknown if t.starts_with('"') => strings += 1,
                // slang lexes the digits after a base as an identifier, or
                // as a number and then an identifier when they start with a
                // digit and go on with letters.
                Kind::VectorDigits => {
                    if t.starts_with(|c: char| c.is_ascii_digit())
                        && t.contains(|c: char| c.is_ascii_alphabetic() || c == '?')
                    {
                        constant = false;
                    } else {
                        numbers += 1;
                    }
                }
                Kind::Punct if matches!(t.as_str(), "(" | ")" | "-" | "+" | "*" | "/" | "<<") => {}
                _ => constant = false,
            }
        }
        if !constant || (numbers == 0) == (strings == 0) || strings > 1 {
            continue;
        }
        let body: String = d.body.iter().map(|(_, t)| t.as_str()).collect();
        out.add(
            "SV-CNS-001",
            "DefineForConstant",
            Severity::Warning,
            format!(
                "`define {} holds the constant '{body}'. As a macro it has no type, no scope and can be redefined by any later file. \
Declare it as a parameter or localparam in a package instead.",
                d.name
            ),
            origin_loc(&csts[d.tree], d.file, d.name_at),
        );
    }
}

const DECLARATION_KEYWORDS: &[&str] = &[
    "class", "function", "task", "typedef", "covergroup", "constraint", "module", "interface", "property", "sequence",
    "rand", "randc", "local", "protected", "static", "virtual", "assert", "cover", "import", "always", "always_comb",
    "always_ff", "always_latch", "initial", "final", "assign", "localparam", "parameter", "generate",
];

fn macro_as_routine(_: &Design, csts: &[Cst], out: &mut Sink) {
    for d in user_defines(csts) {
        if is_overridable_default(define_src(csts, &d), &d) {
            continue;
        }
        let formals: HashSet<&str> = d.formals.iter().flatten().map(|s| s.as_str()).collect();
        let (mut semis, mut depth) = (0, 0i32);
        let (mut has_begin, mut macro_only) = (false, false);
        for (i, (k, t)) in d.body.iter().enumerate() {
            if macro_only {
                break;
            }
            let t = t.as_str();
            match k {
                Kind::Directive | Kind::MacroPaste | Kind::MacroQuote | Kind::MacroEscapedQuote => macro_only = true,
                Kind::Keyword if DECLARATION_KEYWORDS.contains(&t) => macro_only = true,
                Kind::Keyword if matches!(t, "return" | "break" | "continue" | "disable") => macro_only = true,
                _ => {}
            }
            if *k == Kind::Identifier
                && formals.contains(t)
                && d.body.get(i + 1).is_some_and(|(_, n)| n == "::" || n == "#")
            {
                macro_only = true; // a type argument
            }
            match t {
                "(" | "{" => depth += 1,
                ")" | "}" => depth -= 1,
                ";" if depth == 0 => semis += 1,
                _ => {}
            }
            if *k == Kind::Keyword && (t == "begin" || t == "fork") {
                has_begin = true;
            }
        }
        if macro_only || !(semis >= 2 || (has_begin && semis >= 1)) {
            continue;
        }
        let kind = if formals.is_empty() { "function or task" } else { "function or task with arguments" };
        out.add(
            "SV-CNS-002",
            "MacroAsRoutine",
            Severity::Info,
            format!(
                "`define {} is a block of statements that uses nothing only a macro can do. Write it as a {kind}: arguments are \
type-checked, it can be stepped through in a debugger, and errors point at its own lines instead of every expansion.",
                d.name
            ),
            origin_loc(&csts[d.tree], d.file, d.name_at),
        );
    }
}

// ---- SV-CNS-003 ----

const SHIM_PREFIXES: &[&str] = &[
    "UVM_", "VCS", "QUESTA", "MODEL_TECH", "MTI", "XCELIUM", "INCA", "NCSIM", "CADENCE", "RIVIERA", "ALDEC",
    "VERILATOR", "SYNTHESIS", "SYNOPSYS", "IUS", "DSIM", "IVERILOG",
];

fn ifdef_in_reusable_class(_: &Design, csts: &[Cst], out: &mut Sink) {
    for c in csts {
        let fs = Files::new(c.tree);
        let mut seen = HashSet::new();
        for cls in c.nodes(ClassDecl) {
            let Some((first, last)) = c.tree.token_bounds(cls) else { continue };
            if !user_tok(c, &fs, first) {
                continue;
            }
            let name = c.tree.child(cls, Name).map(|n| c.tree.bare_text(n)).unwrap_or_default();
            let ft = &c.tree.tokens[first as usize];
            let f = ft.origin.file;
            let from = ft.origin.offset + ft.text.len() as u32;
            // Directives in the trivia of the class's tokens after the first,
            // up to the last token.
            let lt = &c.tree.tokens[last as usize];
            let to = if lt.origin.file == f && !lt.from_macro {
                lt.origin.offset
            } else {
                c.tree.sources[f as usize].len() as u32
            };
            let lx = &fs.lexed[f as usize];
            for i in 0..lx.toks.len() {
                let (k, at, ..) = lx.toks[i];
                if at < from || at >= to {
                    continue;
                }
                if k != Kind::Directive || !matches!(lx.text(i), "`ifdef" | "`ifndef" | "`elsif") || !live_conditional(c.tree, lx, f, i) {
                    continue;
                }
                if !seen.insert((f, at)) {
                    continue;
                }
                let names: Vec<&str> = lx
                    .rest_of_line(i)
                    .filter(|&j| matches!(lx.toks[j].0, Kind::Identifier | Kind::EscapedIdentifier))
                    .map(|j| lx.text(j))
                    .collect();
                if names.is_empty() || names.iter().any(|n| SHIM_PREFIXES.iter().any(|p| n.starts_with(p))) {
                    continue;
                }
                let what = if names.len() == 1 { format!("`{}", names[0]) } else { "macros".to_string() };
                out.add(
                    "SV-CNS-003",
                    "IfdefInReusableClass",
                    Severity::Warning,
                    format!(
                        "Class '{name}' changes its members or code with {what}. Each define gives a different class that has to be \
compiled and tested on its own, and a reusable component cannot be configured per instance this way. Use a config object \
field, a parameter or a factory override."
                    ),
                    origin_loc(c, f, at),
                );
            }
        }
    }
}

// ---- SV-CNS-006, SV-CNS-007 ----

#[derive(PartialEq)]
enum Container {
    None,
    Class,
    Package,
    Interface,
    Module,
}

fn container_of(c: &Cst, n: NodeId) -> Container {
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
fn user_types(c: &Cst, fs: &Files, kw: &str) -> Vec<(NodeId, u32)> {
    c.nodes_of(&[DataType, NameExpr])
        .into_iter()
        .filter(|&n| c.kind(n) == DataType || c.parent(n).is_some_and(|p| c.kind(p) == CastExpr))
        .filter_map(|n| {
            let t = c.keyword(n)?;
            (c.text_of(t) == kw && c.first_tok(n) == Some(t) && user_tok(c, fs, t)).then_some((n, t))
        })
        .collect()
}

fn implicit_enum_base_type(_: &Design, csts: &[Cst], out: &mut Sink) {
    for c in csts {
        let fs = Files::new(c.tree);
        for (n, kw) in user_types(c, &fs, "enum") {
            if c.tree.child(n, DataType).is_some() || container_of(c, n) == Container::Module {
                continue;
            }
            let first = c
                .tree
                .child(n, Declarator)
                .and_then(|d| c.tree.child(d, Name))
                .map(|x| format!(" {{{}, ...}}", c.tree.bare_text(x)))
                .unwrap_or_default();
            out.add(
                "SV-CNS-006",
                "ImplicitEnumBaseType",
                Severity::Info,
                format!(
                    "Enum{first} has no base type, so it is a 32-bit int. State the width it models, for example enum bit [1:0] {{...}}."
                ),
                c.loc(kw),
            );
        }
    }
}

fn legacy_integer_type(_: &Design, csts: &[Cst], out: &mut Sink) {
    for c in csts {
        let fs = Files::new(c.tree);
        for (n, kw) in user_types(c, &fs, "integer") {
            if container_of(c, n) != Container::Class {
                continue;
            }
            out.add(
                "SV-CNS-007",
                "LegacyIntegerType",
                Severity::Info,
                "'integer' in class code is a 4-state, signed 32-bit legacy type. Use int for counters and indices, or logic [N-1:0] \
when X and Z must be kept."
                    .into(),
                c.loc(kw),
            );
        }
    }
}

// ---- SV-PKG-001 ----

#[derive(Default)]
struct ClassFile {
    classes: Vec<(String, Loc)>,
    has_other_units: bool,
}

fn file_class_name_mismatch(_: &Design, csts: &[Cst], out: &mut Sink) {
    let mut files: BTreeMap<String, ClassFile> = BTreeMap::new();
    let mut seen = HashSet::new();
    for c in csts {
        let fs = Files::new(c.tree);
        // Only `include'd user files: the class files of a package.
        let file_of = |t: u32| -> Option<String> {
            let tok = &c.tree.tokens[t as usize];
            (!tok.from_macro && !c.tree.mains.contains(&tok.origin.file) && fs.user(tok.origin.file)).then(|| fs.path(tok.origin.file))
        };
        for n in c.nodes_of(&[ClassDecl, ModuleDecl, InterfaceDecl, ProgramDecl, PackageDecl]) {
            let Some(first) = c.first_tok(n) else { continue };
            let Some(path) = file_of(first) else { continue };
            if c.kind(n) != ClassDecl {
                files.entry(path).or_default().has_other_units = true;
                continue;
            }
            if !c.parent(n).is_some_and(|p| matches!(c.kind(p), SourceFile | PackageDecl)) {
                continue;
            }
            let Some(name) = c.tree.child(n, Name) else { continue };
            let Some((_, nt)) = c.tree.token_bounds(name) else { continue };
            if !seen.insert((path.clone(), c.tree.tokens[first as usize].origin.offset)) {
                continue;
            }
            files.entry(path).or_default().classes.push((c.tree.bare_text(name), c.loc(nt)));
        }
    }
    for (path, f) in &files {
        if f.classes.is_empty() || f.has_other_units {
            continue;
        }
        let file = base_name(path);
        let stem = file.split('.').next().unwrap_or(file);
        if f.classes.len() == 1 {
            let (name, loc) = &f.classes[0];
            if name == stem {
                continue;
            }
            out.add(
                "SV-PKG-001",
                "FileClassNameMismatch",
                Severity::Warning,
                format!(
                    "'{file}' holds class '{name}'. Name the file after the class ({name}.svh) so it can be found from the class name."
                ),
                *loc,
            );
            continue;
        }
        // Keep the class the file is named after (or the first one).
        let keep = f.classes.iter().rposition(|(n, _)| n == stem).unwrap_or(0);
        for (i, (name, loc)) in f.classes.iter().enumerate() {
            if i == keep {
                continue;
            }
            out.add(
                "SV-PKG-001",
                "FileClassNameMismatch",
                Severity::Warning,
                format!(
                    "'{file}' defines {} classes. Move '{name}' to its own file, {name}.svh, so that each class can be found and reused \
on its own.",
                    f.classes.len()
                ),
                *loc,
            );
        }
    }
}

// ---- SV-PKG-002 ----

/// The items of an import declaration: (package name token, item token).
fn import_items(c: &Cst, n: NodeId) -> Vec<(u32, u32)> {
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

fn wildcard_import_outside_package(_: &Design, csts: &[Cst], out: &mut Sink) {
    let mut seen = HashSet::new();
    for c in csts {
        let fs = Files::new(c.tree);
        for n in c.nodes(ImportDecl) {
            let Some(kw) = c.keyword(n) else { continue };
            let Some(up) = c.parent(n) else { continue };
            if !user_tok(c, &fs, kw) {
                continue;
            }
            let o = c.tree.tokens[kw as usize].origin;
            if !seen.insert((fs.path(o.file), o.offset)) {
                continue;
            }
            for (p, item) in import_items(c, n) {
                if c.text_of(item) != "*" {
                    continue;
                }
                let pkg = c.text_of(p);
                let msg = match c.kind(up) {
                    SourceFile => format!(
                        "'import {pkg}::*' at file scope puts every name of {pkg} into $unit, where it is visible to every file compiled \
after this one. Move the import inside the module, interface or package that uses it."
                    ),
                    PackageDecl => {
                        let pf = c.first_tok(up).map(|t| &c.tree.tokens[t as usize]);
                        if pf.is_some_and(|t| t.origin.file == o.file && !t.from_macro) {
                            continue;
                        }
                        format!(
                            "'import {pkg}::*' sits in a file that is `include'd into a package, so it applies to every file included after \
it and the package's real dependencies are hidden. Import in the package file itself."
                        )
                    }
                    _ => continue,
                };
                out.add("SV-PKG-002", "WildcardImportOutsidePackage", Severity::Warning, msg, c.loc(kw));
            }
        }
    }
}

// ---- package import graph: SV-PKG-004, SV-PKG-005 ----

struct PackageGraph {
    /// (from, to) and where the first import that makes the edge is.
    edges: Vec<((String, String), Loc)>,
    afferent: HashMap<String, usize>,
    efferent: HashMap<String, usize>,
    /// Highest testbench layer of the classes each package defines.
    layers: HashMap<String, i32>,
}

impl PackageGraph {
    fn instability(&self, p: &str) -> f64 {
        let ca = self.afferent.get(p).copied().unwrap_or(0);
        let ce = self.efferent.get(p).copied().unwrap_or(0);
        if ca + ce == 0 {
            0.0
        } else {
            ce as f64 / (ca + ce) as f64
        }
    }
}

fn layer_of(r: Role) -> i32 {
    match r {
        Role::Driver | Role::Monitor | Role::Sequencer | Role::Agent => 1,
        Role::Env | Role::Scoreboard => 2,
        Role::Test => 3,
        _ => 0,
    }
}

fn layer_name(l: i32) -> &'static str {
    match l {
        1 => "agent",
        2 => "env",
        3 => "test",
        _ => "object",
    }
}

fn package_graph(d: &Design, csts: &[Cst]) -> PackageGraph {
    let classes: HashMap<_, _> = user_classes(d).map(|k| (key(k.loc), k)).collect();
    // User packages: the first definition of each name decides.
    let mut packages: HashMap<String, bool> = HashMap::new();
    let mut layers: HashMap<String, i32> = HashMap::new();
    let mut imports: Vec<(String, String, Loc)> = Vec::new();
    for c in csts {
        let fs = Files::new(c.tree);
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

fn package_layer_inversion(d: &Design, csts: &[Cst], out: &mut Sink) {
    let g = package_graph(d, csts);
    for ((from, to), loc) in &g.edges {
        let lf = g.layers.get(from).copied().unwrap_or(0);
        let lt = g.layers.get(to).copied().unwrap_or(0);
        if lf == 0 || lt <= lf {
            continue;
        }
        let (nf, nt) = (layer_name(lf), layer_name(lt));
        out.add(
            "SV-PKG-004",
            "PackageLayerInversion",
            Severity::Warning,
            format!(
                "{nf} package '{from}' imports {nt} package '{to}'. Dependencies must point down the stack (item, sequence, agent, env, \
test); otherwise the {nf} cannot be reused without the {nt}. Move what is shared into the lower package or a common one."
            ),
            *loc,
        );
    }
}

fn unstable_package_depended_on(d: &Design, csts: &[Cst], out: &mut Sink) {
    let g = package_graph(d, csts);
    for ((from, to), loc) in &g.edges {
        let (i_from, i_to) = (g.instability(from), g.instability(to));
        if i_from >= i_to {
            continue;
        }
        out.add(
            "SV-PKG-005",
            "UnstablePackageDependedOn",
            Severity::Info,
            format!(
                "Package '{from}' (instability {i_from:.2}, used by {} packages) depends on '{to}' (instability {i_to:.2}), which changes \
more easily. A change there ripples into everything that uses '{from}'. Invert the dependency or move the shared part into a \
stable package.",
                g.afferent.get(from).copied().unwrap_or(0)
            ),
            *loc,
        );
    }
}
