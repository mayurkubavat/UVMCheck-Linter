//! Classes, their methods and the comments around them, for the class-level
//! readability rules.

use std::collections::HashMap;

use svlang::lexer::Kind;

use svlang::syntax::NodeId;

use crate::syntax::Cst;

use svlang::syntax::SyntaxKind::*;

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
    pub(crate) by_name: HashMap<String, usize>,
}

pub(crate) fn decl_name(c: &Cst, n: NodeId) -> String {
    c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default()
}

pub(crate) fn name_tok(c: &Cst, n: NodeId) -> Option<u32> {
    c.tree.child(n, Name).and_then(|x| c.first_tok(x))
}

/// The file a token's text was written in (`getFullyOriginalLoc`).
pub(crate) fn written_path(c: &Cst, t: u32) -> String {
    let f = c.tree.tokens[t as usize].written.file;
    c.tree.files[f as usize].to_string_lossy().into_owned()
}

pub(crate) fn name_says_config(name: &str) -> bool {
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
                    .and_then(|h| {
                        c.tree.children(h).find(|&e| {
                            c.kind(e) == Extends && c.first_tok(e).is_some_and(|t| c.text_of(t) == "extends")
                        })
                    })
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
    pub(crate) fn compute_role(&self, i: usize) -> Role {
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
                    "uvm_object" | "uvm_void" => {
                        Some(if name_says_config(&self.list[i].name) { Role::Config } else { Role::Object })
                    }
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
pub(crate) fn routine_index(csts: &[Cst]) -> std::sync::Arc<HashMap<String, (usize, NodeId)>> {
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
pub(crate) fn own_comments(c: &Cst, t: u32) -> Vec<(u32, String)> {
    let tok = &c.tree.tokens[t as usize];
    if tok.from_macro {
        return Vec::new();
    }
    let mut out = Vec::new();
    region_comments(c, tok.origin.file, tok.trivia_start, tok.origin.offset, true, &mut out);
    out
}

/// Comments of a trivia region `[from, to)` of file `f`, as above.
pub(crate) fn region_comments(c: &Cst, f: u32, from: u32, to: u32, nested: bool, out: &mut Vec<(u32, String)>) {
    let text = &c.tree.sources[f as usize][from as usize..to as usize];
    let toks = svlang::lexer::lex(text);
    let tx = |x: &svlang::lexer::Token| &text[x.start as usize..(x.start + x.len) as usize];
    // Included files first: their tails come before this file's comments.
    for (i, x) in toks.iter().enumerate() {
        if x.kind != Kind::Directive || tx(x) != "`include" {
            continue;
        }
        let Some(name) = toks[i + 1..].iter().find(|y| !y.kind.is_trivia()).filter(|y| y.kind == Kind::StringLiteral)
        else {
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
pub(crate) fn included_file(c: &Cst, f: u32, name: &str) -> Option<u32> {
    let want = std::path::Path::new(name);
    (f as usize + 1..c.tree.files.len())
        .chain(0..f as usize)
        .map(|i| i as u32)
        .find(|&g| c.tree.files[g as usize].ends_with(want))
}

/// Class properties declared directly in the class: (name, name token).
pub(crate) fn properties(c: &Cst, cls: NodeId) -> Vec<(String, u32)> {
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
