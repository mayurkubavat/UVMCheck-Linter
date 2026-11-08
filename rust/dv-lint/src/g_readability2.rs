//! Class-level readability rules (builder slice N6.3, group F), ported
//! from `cpp/src/rules/readability/`: undocumented extension points and
//! testbench classes, names that say nothing and vowel-dropped
//! abbreviations. The C++ rules walk the classes of slang's AST that
//! are user code (`swe::userClasses`); here the classes come from the
//! syntax trees, with the UVM role of each read from its base chain as
//! `swe::classRole` does. Messages are verbatim.

use std::collections::{HashMap, HashSet};

use dv_model::{Design, Loc};
use sv_lexer::Kind;
use sv_syntax::NodeId;

use crate::g_readability::ident_words;
use crate::syntax::Cst;
use crate::Check::Mixed;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-RDB-010", name: "UndocumentedTestbenchClass", check: Mixed(undocumented_testbench_class) },
        Rule { id: "SV-RDB-007", name: "NonDescriptiveName", check: Mixed(non_descriptive_name) },
        Rule { id: "SV-RDB-008", name: "UnclearAbbreviation", check: Mixed(unclear_abbreviation) },
        Rule { id: "SV-RDB-004", name: "UndocumentedExtensionPoint", check: Mixed(undocumented_extension_point) },
    ]
}

// ---- classes and roles ----

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Role {
    None,
    Object,
    Config,
    Item,
    Sequence,
    Component,
    Driver,
    Monitor,
    Sequencer,
    Agent,
    Scoreboard,
    Subscriber,
    Env,
    Test,
    RegModel,
    Catcher,
    Callback,
}

impl Role {
    pub(crate) fn name(self) -> &'static str {
        use Role::*;
        match self {
            None => "class",
            Object => "object",
            Config => "config",
            Item => "sequence item",
            Sequence => "sequence",
            Component => "component",
            Driver => "driver",
            Monitor => "monitor",
            Sequencer => "sequencer",
            Agent => "agent",
            Scoreboard => "scoreboard",
            Subscriber => "subscriber",
            Env => "env",
            Test => "test",
            RegModel => "register model",
            Catcher => "report catcher",
            Callback => "callback",
        }
    }

    pub(crate) fn is_component(self) -> bool {
        use Role::*;
        matches!(self, Component | Driver | Monitor | Sequencer | Agent | Scoreboard | Subscriber | Env | Test)
    }
}

/// A class declaration in one of the trees.
pub(crate) struct ClassInfo {
    pub ix: usize,
    pub node: NodeId,
    pub name: String,
    /// The base class's name as written (without package or parameters).
    pub base: Option<String>,
    pub generic: bool,
    pub library: bool,
    /// Whether slang's model indexes it: not declared in a module,
    /// interface or program, and not a generic class in a package.
    pub indexed: bool,
    pub role: Role,
}

pub(crate) struct Classes {
    pub list: Vec<ClassInfo>,
    by_name: HashMap<String, usize>,
}

fn decl_name(c: &Cst, n: NodeId) -> String {
    c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default()
}

fn name_tok(c: &Cst, n: NodeId) -> Option<u32> {
    c.tree.child(n, Name).and_then(|x| c.first_tok(x))
}

/// The file a token's text was written in (`getFullyOriginalLoc`).
fn written_path(c: &Cst, t: u32) -> String {
    let f = c.tree.tokens[t as usize].written.file;
    c.tree.files[f as usize].to_string_lossy().into_owned()
}

fn name_says_config(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.contains("cfg") || l.contains("config")
}

impl Classes {
    pub(crate) fn new(csts: &[Cst]) -> Self {
        let mut list = Vec::new();
        for (ix, c) in csts.iter().enumerate() {
            for n in c.nodes(ClassDecl) {
                let name = decl_name(c, n);
                let header = c.tree.child(n, Header);
                let generic = header.and_then(|h| c.tree.child(h, ParamList)).is_some();
                let base = header
                    .and_then(|h| c.tree.children(h).find(|&e| c.kind(e) == Extends && c.first_tok(e).is_some_and(|t| c.text_of(t) == "extends")))
                    .and_then(|e| c.tree.child(e, DataType))
                    .map(|ty| {
                        let toks: Vec<&str> = c.tree.token_range(ty).iter().map(|&t| c.text_of(t)).collect();
                        let end = toks.iter().position(|t| *t == "#").unwrap_or(toks.len());
                        toks[..end].last().copied().unwrap_or("").to_string()
                    });
                let mut library = false;
                let mut indexed = true;
                let mut up = Some(n);
                while let Some(p) = up {
                    match c.kind(p) {
                        ModuleDecl | InterfaceDecl | ProgramDecl => indexed = false,
                        PackageDecl if generic => indexed = false,
                        _ => {}
                    }
                    match c.kind(p) {
                        PackageDecl if decl_name(c, p) == "uvm_pkg" => library = true,
                        ClassDecl if decl_name(c, p).starts_with("uvm_") => library = true,
                        _ => {}
                    }
                    up = c.parent(p);
                }
                if let Some(t) = name_tok(c, n) {
                    let path = written_path(c, t);
                    library |= path.contains("uvm_mock.svh") || path.contains("uvm_pkg.sv");
                }
                list.push(ClassInfo { ix, node: n, name, base, generic, library, indexed, role: Role::None });
            }
        }
        let mut by_name = HashMap::new();
        for (i, k) in list.iter().enumerate() {
            by_name.entry(k.name.clone()).or_insert(i);
        }
        let mut cs = Classes { list, by_name };
        for i in 0..cs.list.len() {
            cs.list[i].role = cs.compute_role(i);
        }
        cs
    }

    /// The resolved base class: one declared with the written name.
    pub(crate) fn base_of(&self, i: usize) -> Option<usize> {
        let b = self.list[i].base.as_ref()?;
        let j = *self.by_name.get(b)?;
        (j != i).then_some(j)
    }

    /// `swe::classRole`: the first uvm_* class up the base chain decides.
    fn compute_role(&self, i: usize) -> Role {
        let mut cur = self.base_of(i);
        let mut guard = 0;
        while let Some(j) = cur {
            if guard > 64 {
                break;
            }
            guard += 1;
            let n = self.list[j].name.as_str();
            if n.starts_with("uvm_") {
                let r = match n {
                    "uvm_driver" | "uvm_push_driver" => Some(Role::Driver),
                    "uvm_monitor" => Some(Role::Monitor),
                    _ if n.starts_with("uvm_sequencer") || n == "uvm_push_sequencer" => Some(Role::Sequencer),
                    "uvm_agent" => Some(Role::Agent),
                    "uvm_scoreboard" => Some(Role::Scoreboard),
                    "uvm_subscriber" => Some(Role::Subscriber),
                    "uvm_env" => Some(Role::Env),
                    "uvm_test" => Some(Role::Test),
                    _ if n.starts_with("uvm_reg") || n == "uvm_mem" || n == "uvm_vreg" => Some(Role::RegModel),
                    "uvm_report_catcher" => Some(Role::Catcher),
                    _ if n.starts_with("uvm_callback") => Some(Role::Callback),
                    "uvm_sequence" | "uvm_sequence_base" | "uvm_reg_sequence" => Some(Role::Sequence),
                    "uvm_sequence_item" | "uvm_transaction" => Some(Role::Item),
                    _ if n == "uvm_component" || n.starts_with("uvm_tlm") || n == "uvm_root" => Some(Role::Component),
                    "uvm_object" | "uvm_void" => Some(if name_says_config(&self.list[i].name) {
                        Role::Config
                    } else {
                        Role::Object
                    }),
                    _ => None,
                };
                if let Some(r) = r {
                    return r;
                }
            }
            cur = self.base_of(j);
        }
        Role::None
    }

    /// User classes (`swe::userClasses`): not the UVM library.
    pub(crate) fn user(&self) -> impl Iterator<Item = (usize, &ClassInfo)> {
        self.list.iter().enumerate().filter(|(_, k)| !k.library && k.indexed)
    }
}

/// A class method as `swe::classMethods` gives it: a routine with a body
/// declared in the class, or the out-of-block body of an extern prototype.
pub(crate) struct MethodInfo {
    pub ix: usize,
    /// The routine with the body.
    pub node: NodeId,
    pub name: String,
}

/// Every routine by its declared name: the first of a name over the
/// trees (functions, then tasks, of each tree).
fn routine_index(csts: &[Cst]) -> std::sync::Arc<HashMap<String, (usize, NodeId)>> {
    let build = || {
        let mut out = HashMap::new();
        for (ix, cc) in csts.iter().enumerate() {
            for n in cc.nodes(FunctionDecl).into_iter().chain(cc.nodes(TaskDecl)) {
                out.entry(decl_name(cc, n)).or_insert((ix, n));
            }
        }
        out
    };
    match csts.first() {
        // The same for every set of all the compile's trees.
        Some(c) if csts.len() == c.all.len() => crate::shared::get("routine_index", c.compile_key(), build),
        _ => std::sync::Arc::new(build()),
    }
}

pub(crate) fn class_methods(csts: &[Cst], k: &ClassInfo) -> Vec<MethodInfo> {
    let c = &csts[k.ix];
    let mut out = Vec::new();
    for m in c.tree.children(k.node) {
        match c.kind(m) {
            FunctionDecl | TaskDecl => {
                if c.toks(m).any(|t| c.text_of(t) == "import") {
                    continue;
                }
                out.push(MethodInfo { ix: k.ix, node: m, name: decl_name(c, m) });
            }
            MethodPrototype => {
                if c.toks(m).any(|t| c.text_of(t) == "pure") {
                    continue;
                }
                let name = decl_name(c, m);
                let full = format!("{}::{}", k.name, name);
                if let Some(&(ix, node)) = routine_index(csts).get(&full) {
                    out.push(MethodInfo { ix, node, name: name.clone() });
                }
            }
            _ => {}
        }
    }
    out
}

/// Comments slang keeps as the token's own trivia: those after the last
/// directive or macro use (the earlier ones belong to the directive), plus
/// those of the trailing trivia of each file included in between, which
/// slang carries over to the next token of the including file (one level:
/// what a nested include carried into that file is lost).
fn own_comments(c: &Cst, t: u32) -> Vec<(u32, String)> {
    let tok = &c.tree.tokens[t as usize];
    if tok.from_macro {
        return Vec::new();
    }
    let mut out = Vec::new();
    region_comments(c, tok.origin.file, tok.trivia_start, tok.origin.offset, true, &mut out);
    out
}

/// Comments of a trivia region `[from, to)` of file `f`, as above.
fn region_comments(c: &Cst, f: u32, from: u32, to: u32, nested: bool, out: &mut Vec<(u32, String)>) {
    let text = &c.tree.sources[f as usize][from as usize..to as usize];
    let toks = sv_lexer::lex(text);
    let tx = |x: &sv_lexer::Token| &text[x.start as usize..(x.start + x.len) as usize];
    // Included files first: their tails come before this file's comments.
    for (i, x) in toks.iter().enumerate() {
        if x.kind != Kind::Directive || tx(x) != "`include" {
            continue;
        }
        let Some(name) = toks[i + 1..].iter().find(|y| !y.kind.is_trivia()).filter(|y| y.kind == Kind::StringLiteral) else {
            continue;
        };
        let name = tx(name).trim_matches('"');
        if let Some(g) = included_file(c, f, name).filter(|_| nested) {
            let last = (0..c.tree.tokens.len())
                .rev()
                .map(|i| &c.tree.tokens[i])
                .find(|k| !k.from_macro && k.origin.file == g)
                .map_or(0, |k| k.origin.offset + k.text.len() as u32);
            let end = c.tree.sources[g as usize].len() as u32;
            region_comments(c, g, last, end, false, out);
        }
    }
    let after = toks.iter().rposition(|x| !x.kind.is_trivia()).map_or(0, |i| i + 1);
    out.extend(
        toks[after..]
            .iter()
            .filter(|x| matches!(x.kind, Kind::LineComment | Kind::BlockComment))
            .map(|x| (from + x.start, tx(x).to_string())),
    );
}

/// The file an `include of `name` in file `f` read: a later file of the
/// tree whose path ends with the name.
fn included_file(c: &Cst, f: u32, name: &str) -> Option<u32> {
    let want = std::path::Path::new(name);
    (f as usize + 1..c.tree.files.len()).chain(0..f as usize).map(|i| i as u32).find(|&g| c.tree.files[g as usize].ends_with(want))
}

/// A class's or method's first token is from a macro or a uvm_* file.
fn library_tok(c: &Cst, t: u32) -> bool {
    let tok = &c.tree.tokens[t as usize];
    tok.from_macro
        || c.tree.files[tok.origin.file as usize]
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("uvm_"))
}

// ---- SV-RDB-010 ----

fn prose_words(comments: &[(u32, String)]) -> usize {
    let mut words = 0;
    for (_, text) in comments {
        for line in text.split('\n') {
            let line = line.to_ascii_lowercase();
            if line.contains("copyright") || line.contains("license") || line.contains("spdx") {
                continue;
            }
            let mut in_word = false;
            for ch in line.bytes() {
                let letter = ch.is_ascii_alphabetic();
                if letter && !in_word {
                    words += 1;
                }
                in_word = letter;
            }
        }
    }
    words
}

fn undocumented_testbench_class(_: &Design, csts: &[Cst], s: &mut Sink) {
    const MIN_WORDS: usize = 10;
    let classes = Classes::new(csts);
    for (_, k) in classes.user() {
        if !k.role.is_component() && k.role != Role::Sequence {
            continue;
        }
        if k.generic {
            continue;
        }
        let c = &csts[k.ix];
        let Some(first) = c.first_tok(k.node) else { continue };
        if library_tok(c, first) {
            continue;
        }
        if prose_words(&own_comments(c, first)) >= MIN_WORDS {
            continue;
        }
        let Some(nt) = name_tok(c, k.node) else { continue };
        s.add(
            "SV-RDB-010",
            "UndocumentedTestbenchClass",
            Severity::Info,
            format!(
                "{} '{}' has no description. Two sentences above the class on what it models, drives \
                 or checks save every reader a trip through the code.",
                k.role.name(),
                k.name
            ),
            c.loc(nt),
        );
    }
}

// ---- SV-RDB-007 ----

fn is_generic_name(name: &str, local: bool) -> bool {
    const STEMS: [&str; 13] = ["tmp", "temp", "val", "obj", "foo", "bar", "baz", "var", "thing", "stuff", "dummy", "d", "t"];
    let mut n = name.to_ascii_lowercase();
    while n.len() > 1 && n.ends_with(|ch: char| ch.is_ascii_digit()) {
        n.pop();
    }
    STEMS.contains(&n.as_str()) || (local && (n == "data" || n == "value"))
}

/// Class properties declared directly in the class: (name, name token).
fn properties(c: &Cst, cls: NodeId) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    for d in c.tree.children(cls).filter(|&d| c.kind(d) == DataDecl) {
        for dd in c.tree.children(d).filter(|&x| c.kind(x) == Declarator) {
            let Some(nm) = c.tree.child(dd, Name) else { continue };
            let Some(t) = c.first_tok(nm) else { continue };
            out.push((c.tree.bare_text(nm), t));
        }
    }
    out
}

/// Line span of a method body: from its first item to its last.
fn body_lines(c: &Cst, routine: NodeId) -> Option<u32> {
    let body = c.tree.child(routine, Body)?;
    let (a, e) = c.tree.token_bounds(body)?;
    let line = |t: u32| {
        let l = c.loc(t);
        l.expansion.map_or(l.line, |(_, x)| x)
    };
    Some(line(e).saturating_sub(line(a)))
}

/// Variables a routine declares (not arguments; for-loop variables left
/// out, their bodies kept).
fn locals(c: &Cst, routine: NodeId) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    let Some(body) = c.tree.child(routine, Body) else { return out };
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        if c.kind(n) == DataDecl {
            let in_for_init = c.parent(n).is_some_and(|p| c.kind(p) == LoopStmt && c.keyword(p).is_some_and(|k| c.text_of(k) == "for"));
            if !in_for_init {
                for dd in c.tree.children(n).filter(|&x| c.kind(x) == Declarator) {
                    let Some(nm) = c.tree.child(dd, Name) else { continue };
                    let Some(t) = c.first_tok(nm) else { continue };
                    out.push((c.tree.bare_text(nm), t));
                }
            }
            continue;
        }
        let kids: Vec<NodeId> = c.tree.children(n).collect();
        stack.extend(kids.into_iter().rev());
    }
    out
}

fn non_descriptive_name(_: &Design, csts: &[Cst], s: &mut Sink) {
    const MIN_LINES: u32 = 10;
    let classes = Classes::new(csts);
    let report = |s: &mut Sink, name: &str, where_: String, loc: Loc| {
        s.add(
            "SV-RDB-007",
            "NonDescriptiveName",
            Severity::Info,
            format!(
                "'{name}' {where_} says nothing about what it holds. Name it for its meaning (for example \
                 aligned_addr, expected_txn) so logs and waveforms read without the source."
            ),
            loc,
        );
    };
    for (_, k) in classes.user() {
        let c = &csts[k.ix];
        for (p, t) in properties(c, k.node) {
            if is_generic_name(&p, false) {
                report(s, &p, format!("in class '{}'", k.name), c.loc(t));
            }
        }
        for m in class_methods(csts, k) {
            let mc = &csts[m.ix];
            if !body_lines(mc, m.node).is_some_and(|n| n >= MIN_LINES) {
                continue;
            }
            for (v, t) in locals(mc, m.node) {
                if is_generic_name(&v, true) {
                    report(s, &v, format!("in '{}'", m.name), mc.loc(t));
                }
            }
        }
    }
}

// ---- SV-RDB-008 ----

fn is_unclear_abbreviation(word: &str) -> bool {
    const LEXICON: &[&str] = &[
        "clk", "rst", "cfg", "cmd", "pkt", "txn", "req", "rsp", "vif", "drv", "mon", "sqr", "scb", "env", "agt", "ral",
        "csr", "irq", "crc", "fsm", "spi", "ddr", "dll", "pll", "phy", "rtl", "hdl", "dpm", "xfr", "msg", "ptr", "cnt",
        "len", "buf", "src", "dst", "mst", "slv", "lst", "pwr", "mmcm", "bfm", "dmn", "dsp", "jtag", "gpr", "rcv",
        "trn", "sts", "ctl", "ctrl", "ctx", "hw", "sw",
    ];
    if word.len() < 4 || word.bytes().any(|b| b"aeiouy".contains(&b)) {
        return false;
    }
    if LEXICON.contains(&word) {
        return false;
    }
    !(word.ends_with('s') && LEXICON.contains(&&word[..word.len() - 1]))
}

/// Arguments of a routine: (name, name token).
fn arguments(c: &Cst, routine: NodeId) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    let Some(pl) = c.tree.child(routine, PortList) else { return out };
    for p in c.tree.children(pl).filter(|&p| c.kind(p) == Port) {
        for dd in c.tree.children(p).filter(|&x| c.kind(x) == Declarator) {
            let Some(nm) = c.tree.child(dd, Name) else { continue };
            let Some(t) = c.first_tok(nm) else { continue };
            out.push((c.tree.bare_text(nm), t));
        }
    }
    out
}

fn unclear_abbreviation(_: &Design, csts: &[Cst], s: &mut Sink) {
    let classes = Classes::new(csts);
    let check = |s: &mut Sink, name: &str, loc: Loc| {
        if let Some(w) = ident_words(name).into_iter().find(|w| is_unclear_abbreviation(w)) {
            s.add(
                "SV-RDB-008",
                "UnclearAbbreviation",
                Severity::Info,
                format!("'{name}' abbreviates by dropping vowels ('{w}'). Spell the word out, or use the team's agreed short form."),
                loc,
            );
        }
    };
    for (_, k) in classes.user() {
        let c = &csts[k.ix];
        if let Some(t) = name_tok(c, k.node) {
            check(s, &k.name, c.loc(t));
        }
        for (p, t) in properties(c, k.node) {
            check(s, &p, c.loc(t));
        }
        for m in class_methods(csts, k) {
            let mc = &csts[m.ix];
            // The method symbol is located at its name in the body's
            // declaration (the out-of-block one for an extern method).
            let Some(t) = mc.tree.child(m.node, Name).and_then(|x| mc.tree.token_bounds(x)).map(|(_, b)| b) else {
                continue;
            };
            check(s, &m.name, mc.loc(t));
            for (a, t) in arguments(mc, m.node) {
                check(s, &a, mc.loc(t));
            }
        }
    }
}

// ---- SV-RDB-004 ----

fn is_uvm_standard_name(n: &str) -> bool {
    n == "new"
        || n.ends_with("_phase")
        || n.starts_with("do_")
        || matches!(
            n,
            "convert2string" | "get_type_name" | "create" | "body" | "pre_body" | "post_body" | "pre_start" | "post_start" | "write" | "catch"
        )
}

/// Methods of a class by name, with whether each is virtual.
fn method_virtuality(c: &Cst, cls: NodeId) -> HashMap<String, bool> {
    let mut out = HashMap::new();
    for m in c.tree.children(cls) {
        if matches!(c.kind(m), FunctionDecl | TaskDecl | MethodPrototype) {
            let v = c.toks(m).any(|t| c.text_of(t) == "virtual");
            out.insert(decl_name(c, m), v);
        }
    }
    out
}

fn undocumented_extension_point(_: &Design, csts: &[Cst], s: &mut Sink) {
    let classes = Classes::new(csts);
    let mut extended: HashSet<usize> = HashSet::new();
    for (i, _) in classes.user() {
        if let Some(b) = classes.base_of(i) {
            extended.insert(b);
        }
    }
    for (i, k) in classes.user() {
        let is_base = extended.contains(&i) || k.name.contains("base") || k.role == Role::Callback;
        if !is_base {
            continue;
        }
        let c = &csts[k.ix];
        // Virtual methods of the ancestors: an override is not a new
        // extension point.
        let mut inherited: HashMap<String, bool> = HashMap::new();
        let mut cur = classes.base_of(i);
        let mut guard = 0;
        while let Some(j) = cur {
            if guard > 64 {
                break;
            }
            guard += 1;
            let b = &classes.list[j];
            for (n, v) in method_virtuality(&csts[b.ix], b.node) {
                let e = inherited.entry(n).or_insert(false);
                *e |= v;
            }
            cur = classes.base_of(j);
        }
        for m in c.tree.children(k.node) {
            if !matches!(c.kind(m), FunctionDecl | TaskDecl | MethodPrototype) {
                continue;
            }
            let name = decl_name(c, m);
            let virt = c.toks(m).any(|t| matches!(c.text_of(t), "virtual" | "pure"));
            let overrides = inherited.get(&name).copied().unwrap_or(false);
            if !virt || overrides || is_uvm_standard_name(&name) {
                continue;
            }
            let Some(first) = c.first_tok(m) else { continue };
            if !own_comments(c, first).is_empty() {
                continue;
            }
            let Some(t) = name_tok(c, m) else { continue };
            s.add(
                "SV-RDB-004",
                "UndocumentedExtensionPoint",
                Severity::Info,
                format!(
                    "Virtual method '{}::{}' is an extension point with no comment. Write down when it is called and what \
                     an override must and must not do.",
                    k.name, name
                ),
                c.loc(t),
            );
        }
    }
}
