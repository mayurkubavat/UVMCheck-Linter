//! Statement shapes (if, case, for, blocks), assignments, calls and per-file
//! compile counts, for the design, dead-code and metrics rules.
//!
//! The C++ rules walk slang's AST. Here they walk the sv-syntax tree, with a
//! statement view ([`stmt`]) that names the slang statement each node stands
//! for, and a class index ([`crate::support::class_index`]) that plays the
//! part of slang's class symbols. Code inside a module that is instantiated
//! `n` times is reported `n` times, as slang visits each instance body
//! ([`Units::times`]).

use sv_sema::fx::FxMap as HashMap;

use sv_syntax::{Child, NodeId, SyntaxKind};

use crate::syntax::Cst;

use crate::{Severity, Sink};

use SyntaxKind::*;

/// The slang statement kind a syntax node stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum St {
    /// `begin ... end`
    Seq,
    /// `fork ... join*`; the join keyword.
    Par(Join),
    If,
    Case,
    RandCase,
    For,
    Foreach,
    While,
    Repeat,
    Forever,
    DoWhile,
    Timed,
    Wait,
    Assert,
    Return,
    Break,
    Continue,
    Expr,
    /// A variable declaration among a block's statements.
    Decl,
    Empty,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Join {
    All,
    Any,
    None,
}

pub(crate) fn stmt(c: &Cst, n: NodeId) -> Option<St> {
    let kw = || c.keyword(n).map(|t| c.text_of(t)).unwrap_or("");
    Some(match c.kind(n) {
        SeqBlock => St::Seq,
        ParBlock => St::Par(match c.toks(n).map(|t| c.text_of(t)).find(|t| t.starts_with("join")) {
            Some("join_any") => Join::Any,
            Some("join_none") => Join::None,
            _ => Join::All,
        }),
        IfStmt => St::If,
        CaseStmt => {
            if c.toks(n).any(|t| c.text_of(t) == "randcase") {
                St::RandCase
            } else {
                St::Case
            }
        }
        LoopStmt => match kw() {
            "for" => St::For,
            "foreach" => St::Foreach,
            "while" => St::While,
            "repeat" => St::Repeat,
            "forever" => St::Forever,
            _ => St::Other,
        },
        DoWhileStmt => St::DoWhile,
        TimedStmt => St::Timed,
        WaitStmt => St::Wait,
        AssertStmt => St::Assert,
        ReturnStmt => St::Return,
        JumpStmt => match kw() {
            "break" => St::Break,
            "continue" => St::Continue,
            _ => St::Other,
        },
        ExprStmt => St::Expr,
        DataDecl => St::Decl,
        EmptyStmt => St::Empty,
        DisableStmt | EventTrigger | ProcAssign | ConcurrentAssert | RandSequenceStmt => St::Other,
        _ => return None,
    })
}

pub(crate) fn is_stmt(c: &Cst, n: NodeId) -> bool {
    stmt(c, n).is_some_and(|s| s != St::Decl)
}

/// Child nodes that are statements or declarations: a block's or routine
/// body's list, in order.
pub(crate) fn list(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.items(n).into_iter().filter(|&x| stmt(c, x).is_some()).collect()
}

/// Statement children of a node (bodies of loops, branches).
pub(crate) fn sub_stmts(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.items(n).into_iter().filter(|&x| is_stmt(c, x)).collect()
}

/// Expression children (not statements, labels or declarations).
pub(crate) fn exprs(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.items(n).into_iter().filter(|&x| stmt(c, x).is_none()).collect()
}

/// An if statement: condition, then-branch, else-branch.
pub(crate) fn if_parts(c: &Cst, n: NodeId) -> (Option<NodeId>, Option<NodeId>, Option<NodeId>) {
    let items = c.items(n);
    let cond = items.iter().copied().find(|&x| stmt(c, x).is_none() && c.kind(x) != ElseClause);
    let then = items.iter().copied().find(|&x| is_stmt(c, x));
    let els = items.iter().copied().find(|&x| c.kind(x) == ElseClause).and_then(|e| sub_stmts(c, e).into_iter().next());
    (cond, then, els)
}

/// A case statement's selector, items (labels and statement) and default.
pub(crate) struct CaseParts {
    pub sel: Option<NodeId>,
    pub items: Vec<(Vec<NodeId>, NodeId)>,
    pub default: Option<NodeId>,
}

pub(crate) fn case_parts(c: &Cst, n: NodeId) -> CaseParts {
    let mut out = CaseParts { sel: None, items: Vec::new(), default: None };
    let randcase = c.toks(n).any(|t| c.text_of(t) == "randcase");
    let mut after_paren = randcase;
    let mut labels = Vec::new();
    let mut default = false;
    for k in c.kids(n) {
        match *k {
            Child::Token(t) => {
                let s = c.text_of(t);
                if s == ")" && !after_paren {
                    after_paren = true;
                } else if s == "default" && after_paren {
                    default = true;
                }
            }
            Child::Node(x) => {
                if matches!(c.kind(x), StmtLabel | Attribute | EndLabel) {
                    continue;
                }
                if !after_paren {
                    out.sel.get_or_insert(x);
                } else if is_stmt(c, x) {
                    if default {
                        out.default = Some(x);
                        default = false;
                    } else {
                        out.items.push((std::mem::take(&mut labels), x));
                    }
                } else {
                    labels.push(x);
                }
            }
        }
    }
    out
}

/// A for loop's initializers, stop expression, steps and body.
pub(crate) struct ForParts {
    pub init: Vec<NodeId>,
    pub stop: Option<NodeId>,
    pub body: Option<NodeId>,
}

pub(crate) fn for_parts(c: &Cst, n: NodeId) -> ForParts {
    let mut out = ForParts { init: Vec::new(), stop: None, body: None };
    let mut semis = 0;
    let mut closed = false;
    for k in c.kids(n) {
        match *k {
            Child::Token(t) => match c.text_of(t) {
                ";" => semis += 1,
                ")" => closed = true,
                _ => {}
            },
            Child::Node(x) => {
                if matches!(c.kind(x), StmtLabel | Attribute) {
                    continue;
                }
                if closed || (semis >= 2 && is_stmt(c, x) && c.kind(x) != DataDecl) {
                    out.body = Some(x);
                } else if semis == 0 {
                    out.init.push(x);
                } else if semis == 1 {
                    out.stop = Some(x);
                }
            }
        }
    }
    out
}

/// The last statement child: a loop's, timed or wait statement's body.
pub(crate) fn body_of(c: &Cst, n: NodeId) -> Option<NodeId> {
    match stmt(c, n) {
        Some(St::For) => for_parts(c, n).body,
        Some(St::DoWhile) => sub_stmts(c, n).into_iter().next(),
        _ => sub_stmts(c, n).into_iter().last(),
    }
}

/// A binary expression's operator text.
pub(crate) fn op<'t>(c: &Cst<'t>, n: NodeId) -> &'t str {
    if c.kind(n) != BinaryExpr && c.kind(n) != PrefixExpr && c.kind(n) != PostfixExpr {
        return "";
    }
    c.toks(n).next().map(|t| c.text_of(t)).unwrap_or("")
}

pub(crate) const ASSIGN_OPS: &[&str] =
    &["=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=", "<<<=", ">>>="];

/// An assignment as an expression statement: `x = e;`, `x += e;`, `x <= e;`.
pub(crate) fn assignment<'t>(c: &Cst<'t>, s: NodeId) -> Option<(NodeId, NodeId, &'t str)> {
    if c.kind(s) != ExprStmt {
        return None;
    }
    let e = exprs(c, s).into_iter().next()?;
    assign_expr(c, e)
}

pub(crate) fn assign_expr<'t>(c: &Cst<'t>, e: NodeId) -> Option<(NodeId, NodeId, &'t str)> {
    if c.kind(e) != BinaryExpr {
        return None;
    }
    let o = op(c, e);
    if !ASSIGN_OPS.contains(&o) && o != "<=" {
        return None;
    }
    let mut it = c.tree.children(e);
    let (l, r) = (it.next()?, it.next()?);
    Some((l, r, o))
}

/// `x` for a plain identifier expression (slang's NamedValue), not `this`,
/// `super`, `null` or a system name.
pub(crate) fn ident<'t>(c: &Cst<'t>, e: NodeId) -> Option<&'t str> {
    if c.kind(e) != NameExpr {
        return None;
    }
    let mut toks = c.toks(e);
    let t = toks.next()?;
    if toks.next().is_some() {
        return None;
    }
    let s = c.text_of(t);
    let first = s.chars().next()?;
    if !(first.is_ascii_alphabetic() || first == '_' || first == '\\') {
        return None;
    }
    if matches!(s, "this" | "super" | "null" | "local") {
        return None;
    }
    Some(s)
}

/// Every node under `n` (itself included), in source order.
pub(crate) fn subtree(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.subtree(n)
}

/// The node's first token location, as slang's `sourceRange.start()`.
pub(crate) fn start(c: &Cst, n: NodeId) -> dv_model::Loc {
    c.first_tok(n).map(|t| c.loc(t)).unwrap_or_default()
}

/// A call: its callee name (last identifier) and the callee node.
pub(crate) fn call_name<'a>(c: &'a Cst, call: NodeId) -> Option<&'a str> {
    if c.kind(call) != CallExpr {
        return None;
    }
    let callee = c.tree.children(call).next()?;
    match c.kind(callee) {
        NameExpr | ScopedExpr | MemberExpr => {
            let (_, b) = c.tree.token_bounds(callee)?;
            Some(c.text_of(b))
        }
        // `x.m() with (...)` nests the call.
        CallExpr => call_name(c, callee),
        _ => None,
    }
}

/// A call's arguments, in order (the Arg nodes' expressions).
pub(crate) fn call_args(c: &Cst, call: NodeId) -> Vec<Option<NodeId>> {
    let Some(list) = c.tree.child(call, ArgList) else {
        return Vec::new();
    };
    c.tree.children(list).filter(|&a| c.kind(a) == Arg).map(|a| c.tree.children(a).next()).collect()
}

/// File name a token was written in.
pub(crate) fn file_of(c: &Cst, tok: u32) -> String {
    let w = c.tree.tokens[tok as usize].written;
    c.tree.files.get(w.file as usize).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

/// How many times slang elaborates each module, interface or program body.
pub(crate) struct Units {
    pub(crate) times: HashMap<u32, usize>,
    /// Counting only instances outside generate blocks, as
    /// `swe::freeRoutines` walks instance bodies.
    pub(crate) direct: HashMap<u32, usize>,
    /// Specializations of each generic class (by declaration node).
    pub(crate) specs: HashMap<u32, usize>,
    /// Generic class declaration nodes by name, for out-of-block bodies.
    pub(crate) generic_names: HashMap<String, u32>,
}

impl Units {
    pub fn new(c: &Cst) -> Units {
        let specs = crate::support::specializations::spec_counts(c);
        let generic_names = c
            .nodes(ClassDecl)
            .into_iter()
            .filter(|n| specs.contains_key(&n.0))
            .filter_map(|n| c.tree.child(n, Name).map(|x| (c.tree.bare_text(x), n.0)))
            .collect();
        Units { times: Self::count(c, false), direct: Self::count(c, true), specs, generic_names }
    }

    pub(crate) fn count(c: &Cst, direct_only: bool) -> HashMap<u32, usize> {
        let all = compile_counts(c.all, direct_only);
        let ix = c.index();
        all.iter().filter(|((t, _), _)| *t == ix).map(|((_, n), &k)| (*n, k)).collect()
    }

    /// [`Self::times`] counting only instances outside generate blocks.
    pub fn direct_times(&self, _c: &Cst, unit: NodeId) -> usize {
        self.direct.get(&unit.0).copied().unwrap_or(0)
    }

    /// How many times the code at `n` is elaborated (0 for a module nobody
    /// reaches from a top, 1 outside modules).
    pub fn times(&self, c: &Cst, n: NodeId) -> usize {
        self.unit_times(c, n) * self.spec_times(c, n)
    }

    /// Instances of the enclosing module only (generic classes once, as
    /// `swe::UserCodeVisitor` walks only their default specialization).
    pub fn unit_times(&self, c: &Cst, n: NodeId) -> usize {
        let mut x = Some(n);
        while let Some(y) = x {
            if matches!(c.kind(y), ModuleDecl | InterfaceDecl | ProgramDecl) {
                return self.times.get(&y.0).copied().unwrap_or(0);
            }
            x = c.parent(y);
        }
        1
    }

    /// Specializations of the generic classes around `n`.
    pub(crate) fn spec_times(&self, c: &Cst, n: NodeId) -> usize {
        let mut k = 1;
        let mut x = Some(n);
        while let Some(y) = x {
            if let Some(&m) = self.specs.get(&y.0) {
                k *= m;
            } else if matches!(c.kind(y), FunctionDecl | TaskDecl) {
                if let Some(nm) = c.tree.child(y, Name) {
                    let toks: Vec<u32> = c.toks(nm).collect();
                    if toks.len() >= 3 && c.text_of(toks[1]) == "::" {
                        if let Some(&g) = self.generic_names.get(c.text_of(toks[0])) {
                            k *= self.specs.get(&g).copied().unwrap_or(1);
                        }
                    }
                }
            }
            x = c.parent(y);
        }
        k
    }
}

/// [`Units`]' instance counts over every tree of the compile: a module
/// one file declares is often instantiated in another. Keyed by tree index
/// and definition node; cached per compile.
pub(crate) fn compile_counts(all: &[sv_syntax::Tree], direct_only: bool) -> std::rc::Rc<HashMap<(usize, u32), usize>> {
    type Cache = Vec<((usize, usize, bool), std::rc::Rc<HashMap<(usize, u32), usize>>)>;
    thread_local! {
        static CACHE: std::cell::RefCell<Cache> = const { std::cell::RefCell::new(Vec::new()) };
    }
    let key = (all.as_ptr() as usize, all.len(), direct_only);
    if let Some(v) = CACHE.with(|k| k.borrow().iter().find(|e| e.0 == key).map(|e| e.1.clone())) {
        return v;
    }
    let units = |k: SyntaxKind| matches!(k, ModuleDecl | InterfaceDecl | ProgramDecl);
    let mut defs: Vec<(usize, u32)> = Vec::new();
    let mut by_name: HashMap<String, (usize, u32)> = HashMap::default();
    let mut parents_of: Vec<Vec<u32>> = Vec::new();
    for (ti, t) in all.iter().enumerate() {
        let pre = t.preorder();
        let mut parent = vec![u32::MAX; t.nodes.len()];
        for &n in &pre {
            for ch in t.children(n) {
                parent[ch.0 as usize] = n.0;
            }
            if units(t.kind(n)) {
                if let Some(nm) = t.child(n, Name) {
                    by_name.insert(t.bare_text(nm), (ti, n.0));
                }
                defs.push((ti, n.0));
            }
        }
        parents_of.push(parent);
    }
    let mut parents: HashMap<(usize, u32), Vec<((usize, u32), usize)>> = HashMap::default();
    let mut instantiated: sv_sema::fx::FxSet<(usize, u32)> = Default::default();
    for (ti, t) in all.iter().enumerate() {
        let parent = &parents_of[ti];
        let up = |n: u32| (parent[n as usize] != u32::MAX).then(|| parent[n as usize]);
        for inst in t.preorder() {
            if t.kind(inst) != Instantiation {
                continue;
            }
            let Some(ty) = t.child(inst, DataType) else { continue };
            let Some((a, _)) = t.token_bounds(ty) else { continue };
            let Some(&def) = by_name.get(&*t.tokens[a as usize].text) else { continue };
            // The enclosing unit, and whether a generate construct is
            // between it and the instantiation.
            let mut x = up(inst.0);
            let mut gen = false;
            let mut unit = None;
            while let Some(y) = x {
                let k = t.kind(NodeId(y));
                if units(k) {
                    unit = Some(y);
                    break;
                }
                if matches!(k, GenerateBlock | IfGenerate | LoopGenerate | CaseGenerate) {
                    gen = true;
                }
                x = up(y);
            }
            let Some(u) = unit else { continue };
            if direct_only && gen {
                continue;
            }
            let count = t.children(inst).filter(|&x| t.kind(x) == Instance).count();
            parents.entry(def).or_default().push(((ti, u), count));
            instantiated.insert(def);
        }
    }
    // Tops are elaborated once; a definition as often as its parents,
    // times the instances in each (memoized: the paths through a deep
    // hierarchy are exponentially many). A cycle contributes nothing.
    type Key = (usize, u32);
    fn times_of(
        d: Key,
        tops: &sv_sema::fx::FxSet<Key>,
        parents: &HashMap<Key, Vec<(Key, usize)>>,
        memo: &mut HashMap<Key, Option<usize>>,
    ) -> usize {
        match memo.get(&d) {
            Some(Some(t)) => return *t,
            Some(None) => return 0,
            None => {}
        }
        memo.insert(d, None);
        let mut t = usize::from(!tops.contains(&d));
        for &(p, n) in parents.get(&d).map(Vec::as_slice).unwrap_or(&[]) {
            t = t.saturating_add(times_of(p, tops, parents, memo).saturating_mul(n));
        }
        memo.insert(d, Some(t));
        t
    }
    let mut memo = HashMap::default();
    let mut out = HashMap::default();
    for &d in &defs {
        let t = times_of(d, &instantiated, &parents, &mut memo);
        if t > 0 {
            out.insert(d, t);
        }
    }
    let v = std::rc::Rc::new(out);
    CACHE.with(|k| {
        let mut k = k.borrow_mut();
        if k.len() > 4 {
            k.remove(0);
        }
        k.push((key, v.clone()));
    });
    v
}

/// Adds a finding once per elaboration of the code at `at`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn report(
    s: &mut Sink,
    times: usize,
    rule: &'static str,
    name: &'static str,
    sev: Severity,
    msg: String,
    loc: dv_model::Loc,
) {
    for _ in 0..times {
        s.add(rule, name, sev, msg.clone(), loc);
    }
}

/// Statement lists slang builds: block and routine bodies.
pub(crate) fn statement_lists(c: &Cst) -> Vec<NodeId> {
    c.nodes(SeqBlock).into_iter().chain(c.nodes(ParBlock)).chain(c.nodes(Body)).collect()
}

/// `n` is in a subroutine or procedural block whose statements hold a
/// syntax error: slang binds the body as invalid and its AST walk
/// (VisitFlags::AllGood) skips all of it.
pub(crate) fn in_bad_body(c: &Cst, n: NodeId) -> bool {
    let Some(d) = ancestors_or_self(c, n).find(|&x| matches!(c.kind(x), FunctionDecl | TaskDecl | ProceduralBlock))
    else {
        return false;
    };
    c.tree
        .children(d)
        .filter(|&k| !matches!(c.kind(k), Name | PortList | ReturnType | DataType | Attribute))
        .any(|k| has_error(c, k))
}

pub(crate) fn ancestors_or_self<'a>(c: &'a Cst, n: NodeId) -> impl Iterator<Item = NodeId> + 'a {
    std::iter::successors(Some(n), move |&x| c.parent(x))
}

/// The node's tokens hold a syntax error.
pub(crate) fn has_error(c: &Cst, n: NodeId) -> bool {
    let Some((a, b)) = c.tree.token_bounds(n) else {
        return false;
    };
    c.tree.errors.iter().any(|e| e.token >= a && e.token <= b)
}

/// The value of a string literal expression (escapes as written).
pub(crate) fn string_literal(c: &Cst, e: NodeId) -> Option<String> {
    if c.kind(e) != Literal {
        return None;
    }
    let mut toks = c.toks(e);
    let t = c.text_of(toks.next()?);
    if toks.next().is_some() || !t.starts_with('"') || t.len() < 2 {
        return None;
    }
    Some(unescape(&t[1..t.len() - 1]))
}

pub(crate) fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(ch) = it.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Looks through parentheses, which slang keeps out of the AST.
pub(crate) fn strip_parens(c: &Cst, e: NodeId) -> NodeId {
    let mut e = e;
    while c.kind(e) == ParenExpr {
        match c.tree.children(e).next() {
            Some(x) => e = x,
            None => break,
        }
    }
    e
}
