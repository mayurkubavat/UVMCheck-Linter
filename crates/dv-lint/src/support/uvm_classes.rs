//! UVM class roles, sequence bodies and the calls in them.
//!
//! slang's AST visits each statement and expression once per elaborated
//! context; here a class's body is read once (its default specialization).
//! An expression slang cannot bind drops the statement list around it from
//! the AST, which syntax alone cannot see, so these ports assume clean code.

use std::collections::HashMap;

use dv_model::{Class, ClassId, Design, Loc};

use sv_syntax::{NodeId, SyntaxKind};

use crate::support::uvm_syntax::{
    arg_list, callee, class_routines, decl_name, enclosing_class,
    name_node, ordered_expr, params, routine_last_name, subtree, trim_ws,
};

use crate::syntax::Cst;


use crate::{Severity, Sink};

use SyntaxKind::*;

pub(crate) type Key = (u32, u32, u32, Option<(u32, u32)>);

pub(crate) fn key(l: Loc) -> Key {
    (l.file, l.line, l.column, l.expansion)
}

/// A node of one of the trees.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct At {
    pub(crate) tree: usize,
    pub(crate) node: NodeId,
}

/// Design classes and their declarations in the trees.
pub(crate) struct Ix<'a, 't> {
    pub(crate) d: &'a Design,
    pub(crate) csts: &'a [Cst<'t>],
    pub(crate) decl: HashMap<ClassId, At>,
    pub(crate) class_at: HashMap<At, ClassId>,
    /// Out-of-block bodies by (class name, method name).
    pub(crate) out_of_block: HashMap<(String, String), At>,
}

impl<'a, 't> Ix<'a, 't> {
    pub(crate) fn new(d: &'a Design, csts: &'a [Cst<'t>]) -> Self {
        let by_loc: HashMap<Key, ClassId> = d.classes().iter().map(|c| (key(c.loc), c.id)).collect();
        let mut decl = HashMap::new();
        let mut class_at = HashMap::new();
        let mut out_of_block = HashMap::new();
        for (ti, c) in csts.iter().enumerate() {
            for n in c.nodes(ClassDecl) {
                let Some(t) = name_node(c, n).and_then(|x| c.first_tok(x)) else { continue };
                if let Some(&id) = by_loc.get(&key(c.loc(t))) {
                    let at = At { tree: ti, node: n };
                    decl.entry(id).or_insert(at);
                    class_at.insert(at, id);
                }
            }
            for n in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
                if c.parent(n).is_some_and(|p| c.kind(p) == ClassDecl) {
                    continue;
                }
                let Some(nm) = name_node(c, n) else { continue };
                let toks = c.tree.token_range(nm);
                if toks.len() >= 3 && c.text_of(toks[toks.len() - 2]) == "::" {
                    let cls = c.text_of(toks[toks.len() - 3]).to_string();
                    let m = c.text_of(toks[toks.len() - 1]).to_string();
                    out_of_block.entry((cls, m)).or_insert(At { tree: ti, node: n });
                }
            }
        }
        Ix { d, csts, decl, class_at, out_of_block }
    }

    pub(crate) fn cst(&self, at: At) -> &'a Cst<'t> {
        &self.csts[at.tree]
    }

    /// The design class a class declaration stands for.
    pub(crate) fn class_of(&self, at: At) -> Option<&'a Class> {
        self.class_at.get(&at).map(|&id| self.d.class(id))
    }

    /// slang's `cls->members()` subroutines with bodies: methods defined in
    /// the class (not extern ones).
    pub(crate) fn own_routines(&self, id: ClassId) -> Vec<At> {
        let Some(&at) = self.decl.get(&id) else { return Vec::new() };
        class_routines(self.cst(at), at.node).into_iter().map(|node| At { tree: at.tree, node }).collect()
    }

    /// `swe::classMethods`: [`Self::own_routines`] plus extern bodies.
    pub(crate) fn all_routines(&self, id: ClassId) -> Vec<At> {
        let mut v = self.own_routines(id);
        let Some(&at) = self.decl.get(&id) else { return v };
        let c = self.cst(at);
        let cls = self.d.class(id).name.clone();
        for p in c.tree.children(at.node).filter(|&n| c.kind(n) == MethodPrototype) {
            if c.tok(p, "pure").is_some() {
                continue;
            }
            let m = routine_last_name(c, p);
            if let Some(&b) = self.out_of_block.get(&(cls.clone(), m)) {
                v.push(b);
            }
        }
        v
    }

    /// The class a routine belongs to: its enclosing declaration, or the
    /// class an out-of-block body names.
    pub(crate) fn routine_class(&self, at: At) -> Option<ClassId> {
        let c = self.cst(at);
        if let Some(k) = c.parent(at.node).filter(|&p| c.kind(p) == ClassDecl) {
            return self.class_at.get(&At { tree: at.tree, node: k }).copied();
        }
        let nm = name_node(c, at.node)?;
        let toks = c.tree.token_range(nm);
        if toks.len() >= 3 && c.text_of(toks[toks.len() - 2]) == "::" {
            let cls = c.text_of(toks[toks.len() - 3]);
            return self.d.classes().iter().find(|k| k.name == cls && !k.is_parameterized).map(|k| k.id);
        }
        None
    }

    /// Classes slang's UvmModel indexes (no parameterized ones), in order.
    pub(crate) fn model_classes(&self) -> impl Iterator<Item = &'a Class> {
        self.d.classes().iter().filter(|c| !c.is_parameterized)
    }
}

/// `UvmModel::isDerivedFrom`: the class or a resolved base is named `name`.
pub(crate) fn derived(d: &Design, id: ClassId, name: &str) -> bool {
    d.class(id).name == name || d.ancestors(id).any(|a| a.name == name)
}

pub(crate) fn is_component(d: &Design, id: ClassId) -> bool {
    derived(d, id, "uvm_component")
}

pub(crate) fn is_sequence(d: &Design, id: ClassId) -> bool {
    derived(d, id, "uvm_sequence_base") || derived(d, id, "uvm_sequence")
}

/// `swe::Role`.
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

pub(crate) fn class_role(d: &Design, id: ClassId) -> Role {
    for a in d.ancestors(id).take(64) {
        let n = a.name.as_str();
        if !n.starts_with("uvm_") {
            continue;
        }
        if n == "uvm_driver" || n == "uvm_push_driver" {
            return Role::Driver;
        }
        if n == "uvm_monitor" {
            return Role::Monitor;
        }
        if n.starts_with("uvm_sequencer") || n == "uvm_push_sequencer" {
            return Role::Sequencer;
        }
        match n {
            "uvm_agent" => return Role::Agent,
            "uvm_scoreboard" => return Role::Scoreboard,
            "uvm_subscriber" => return Role::Subscriber,
            "uvm_env" => return Role::Env,
            "uvm_test" => return Role::Test,
            _ => {}
        }
        if n.starts_with("uvm_reg") || n == "uvm_mem" || n == "uvm_vreg" {
            return Role::RegModel;
        }
        if n == "uvm_report_catcher" {
            return Role::Catcher;
        }
        if n.starts_with("uvm_callback") {
            return Role::Callback;
        }
        if matches!(n, "uvm_sequence" | "uvm_sequence_base" | "uvm_reg_sequence") {
            return Role::Sequence;
        }
        if matches!(n, "uvm_sequence_item" | "uvm_transaction") {
            return Role::Item;
        }
        if n == "uvm_component" || n.starts_with("uvm_tlm") || n == "uvm_root" {
            return Role::Component;
        }
        if n == "uvm_object" || n == "uvm_void" {
            let l = d.class(id).name.to_ascii_lowercase();
            return if l.contains("cfg") || l.contains("config") { Role::Config } else { Role::Object };
        }
    }
    Role::None
}

/// `swe::isLibrarySymbol` for a class.
pub(crate) fn is_library(d: &Design, c: &Class) -> bool {
    if c.name.starts_with("uvm_") || c.scope.split("::").any(|s| s == "uvm_pkg" || s.starts_with("uvm_")) {
        return true;
    }
    let f = d.file_name(c.loc);
    f.contains("uvm_mock.svh") || f.contains("uvm_pkg.sv")
}

/// `swe::userClasses`: model classes, then parameterized ones, minus the library.
pub(crate) fn user_classes(d: &Design) -> Vec<&Class> {
    let a = d.classes().iter().filter(|c| !c.is_parameterized);
    let b = d.classes().iter().filter(|c| c.is_parameterized);
    a.chain(b).filter(|c| !is_library(d, c)).collect()
}

/// A type as written, reduced to the class name it names.
pub(crate) fn type_head(s: &str) -> &str {
    let s = s.split(['#', '[', '(']).next().unwrap_or("");
    let s = s.rsplit("::").next().unwrap_or("");
    trim_ws(s)
}

pub(crate) fn class_named<'a>(d: &'a Design, name: &str) -> Option<&'a Class> {
    if name.is_empty() {
        return None;
    }
    d.classes().iter().find(|c| c.name == name && !c.is_parameterized).or_else(|| d.find_class(name))
}

/// An if statement: condition, then-branch and else-branch.
pub(crate) fn if_parts(c: &Cst, n: NodeId) -> Option<(NodeId, NodeId, Option<NodeId>)> {
    let items: Vec<NodeId> = c.items(n);
    let cond = *items.first()?;
    let then = *items.get(1)?;
    let els = c.tree.child(n, ElseClause).and_then(|e| c.items(e).into_iter().next());
    Some((cond, then, els))
}

/// Statements a procedural block or routine holds in order.
pub(crate) fn block_stmts(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.items(n)
}

pub(crate) fn is_routine(k: SyntaxKind) -> bool {
    matches!(k, FunctionDecl | TaskDecl)
}

/// Calls in a subtree (slang's CallExpression), with `$display;` written
/// without parentheses.
pub(crate) fn calls_in(c: &Cst, n: NodeId) -> Vec<NodeId> {
    subtree(c, n)
        .into_iter()
        .filter(|&x| match c.kind(x) {
            CallExpr => true,
            NameExpr => {
                c.parent(x).is_some_and(|p| c.kind(p) == ExprStmt) && c.first_tok(x).is_some_and(|t| c.text_of(t).starts_with('$'))
            }
            _ => false,
        })
        .collect()
}

/// A call's name: `getSubroutineName()`.
pub(crate) fn call_name<'t>(c: &Cst<'t>, n: NodeId) -> &'t str {
    if c.kind(n) == NameExpr {
        return c.last_text(n);
    }
    callee(c, n)
}

/// The receiver of a method call (`a.b` in `a.b.f()`), when there is one.
pub(crate) fn receiver(c: &Cst, call: NodeId) -> Option<NodeId> {
    if c.kind(call) != CallExpr {
        return None;
    }
    let left = c.tree.children(call).next()?;
    (c.kind(left) == MemberExpr).then(|| c.tree.children(left).next()).flatten()
}

/// slang binds `a.b[0].c` as one name: only its outermost part keeps syntax.
pub(crate) fn is_name_chain(c: &Cst, n: NodeId) -> bool {
    match c.kind(n) {
        NameExpr | ScopedExpr => true,
        MemberExpr | SelectExpr => c.tree.children(n).next().is_some_and(|l| is_name_chain(c, l)),
        _ => false,
    }
}

/// The expression nodes of an argument list, as slang's `arguments()`
/// (written ones only).
pub(crate) fn args(c: &Cst, call: NodeId) -> Vec<Option<NodeId>> {
    let Some(list) = arg_list(c, call) else { return Vec::new() };
    params(c, list).into_iter().map(|a| a.and_then(|a| ordered_expr(c, a))).collect()
}

/// Strips parentheses (slang's conversions do not show in syntax).
pub(crate) fn unparen(c: &Cst, mut n: NodeId) -> NodeId {
    while c.kind(n) == ParenExpr {
        match c.items(n).into_iter().next() {
            Some(x) => n = x,
            None => break,
        }
    }
    n
}

/// The value of a string literal expression.
pub(crate) fn string_value(c: &Cst, n: NodeId) -> Option<String> {
    let n = unparen(c, n);
    if c.kind(n) != Literal {
        return None;
    }
    let toks = c.tree.token_range(n);
    if toks.len() != 1 {
        return None;
    }
    let t = c.text_of(toks[0]);
    let inner = t.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut it = inner.chars();
    while let Some(ch) = it.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(x) => out.push(x),
            None => {}
        }
    }
    Some(out)
}

/// Declared names of a routine: ports and locals, with their types as written.
pub(crate) fn routine_vars(c: &Cst, f: NodeId) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    if let Some(pl) = c.tree.child(f, PortList) {
        let mut last = String::new();
        for p in c.tree.children(pl).filter(|&p| c.kind(p) == Port) {
            if let Some(dt) = c.tree.child(p, DataType) {
                last = c.tree.bare_text(dt);
            }
            for d in c.tree.children(p).filter(|&d| c.kind(d) == Declarator) {
                if let Some(nm) = name_node(c, d) {
                    vars.entry(c.tree.bare_text(nm)).or_insert_with(|| last.clone());
                }
            }
        }
    }
    for n in subtree(c, f) {
        if c.kind(n) != DataDecl {
            continue;
        }
        let Some(dt) = c.tree.child(n, DataType) else { continue };
        let ty = c.tree.bare_text(dt);
        for d in c.tree.children(n).filter(|&d| c.kind(d) == Declarator) {
            if let Some(nm) = name_node(c, d) {
                vars.entry(c.tree.bare_text(nm)).or_insert_with(|| ty.clone());
            }
        }
    }
    vars
}

/// Where an expression is evaluated: the routine's class and declared names.
pub(crate) struct Ctx {
    pub(crate) class: Option<ClassId>,
    pub(crate) vars: HashMap<String, String>,
}

impl Ix<'_, '_> {
    pub(crate) fn ctx(&self, at: At) -> Ctx {
        let c = self.cst(at);
        let mut r = at.node;
        loop {
            if is_routine(c.kind(r)) {
                break;
            }
            match c.parent(r) {
                Some(p) => r = p,
                None => {
                    return Ctx { class: None, vars: HashMap::new() };
                }
            }
        }
        let rat = At { tree: at.tree, node: r };
        Ctx { class: self.routine_class(rat), vars: routine_vars(c, r) }
    }

    /// A property's class type, in the class or its bases.
    pub(crate) fn property_class(&self, id: ClassId, name: &str) -> Option<ClassId> {
        let d = self.d;
        for c in std::iter::once(d.class(id)).chain(d.ancestors(id)) {
            if let Some(p) = c.properties.iter().find(|p| p.name == name) {
                return p.type_class.or_else(|| class_named(d, type_head(&p.ty)).map(|k| k.id));
            }
        }
        None
    }

    /// A method's return class type, in the class or its bases.
    pub(crate) fn method_class(&self, id: ClassId, name: &str) -> Option<ClassId> {
        let d = self.d;
        for c in std::iter::once(d.class(id)).chain(d.ancestors(id)) {
            if let Some(m) = c.methods.iter().find(|m| m.name == name) {
                return class_named(d, type_head(&m.return_type)).map(|k| k.id);
            }
        }
        None
    }

    /// The class type of an expression, as far as names tell.
    pub(crate) fn expr_class(&self, c: &Cst, cx: &Ctx, n: NodeId, depth: usize) -> Option<ClassId> {
        if depth > 16 {
            return None;
        }
        let n = unparen(c, n);
        match c.kind(n) {
            NameExpr => {
                let name = c.tree.bare_text(n);
                if name == "this" {
                    return cx.class;
                }
                if let Some(ty) = cx.vars.get(&name) {
                    return class_named(self.d, type_head(ty)).map(|k| k.id);
                }
                cx.class.and_then(|k| self.property_class(k, &name))
            }
            MemberExpr => {
                let left = c.tree.children(n).next()?;
                let member = c.last_text(n);
                if c.tree.bare_text(left) == "super" {
                    let base = self.d.class(cx.class?).base?;
                    return self.property_class(base, member);
                }
                let k = self.expr_class(c, cx, left, depth + 1)?;
                self.property_class(k, member)
            }
            CallExpr => {
                let left = c.tree.children(n).next()?;
                match c.kind(left) {
                    NameExpr => self.method_class(cx.class?, &c.tree.bare_text(left)),
                    MemberExpr => {
                        let recv = c.tree.children(left).next()?;
                        let k = if c.tree.bare_text(recv) == "super" {
                            self.d.class(cx.class?).base?
                        } else {
                            self.expr_class(c, cx, recv, depth + 1)?
                        };
                        self.method_class(k, c.last_text(left))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

pub(crate) const COMPONENT_BASES: [&str; 7] =
    ["uvm_component", "uvm_env", "uvm_agent", "uvm_driver", "uvm_monitor", "uvm_scoreboard", "uvm_test"];

/// The first model class with this name.
pub(crate) fn first_named<'a>(ix: &Ix<'a, '_>, name: &str) -> Option<&'a Class> {
    ix.model_classes().find(|c| c.name == name)
}

/// The component test of the class-walking C++ visitors.
pub(crate) fn comp_decl(ix: &Ix, c: &Cst, cls: NodeId) -> bool {
    let name = decl_name(c, cls);
    if name.is_empty() || name == "uvm_component" {
        return false;
    }
    if first_named(ix, &name).is_some_and(|k| is_component(ix.d, k.id)) {
        return true;
    }
    let base = crate::support::uvm_syntax::base_name_node(c, cls).map(|b| c.text(b)).unwrap_or_default();
    COMPONENT_BASES.iter().any(|b| base.contains(b))
}

/// The sequence test of the class-walking C++ visitors.
pub(crate) fn seq_decl(ix: &Ix, c: &Cst, cls: NodeId) -> bool {
    let name = decl_name(c, cls);
    if name.is_empty() || name == "uvm_sequence" || name == "uvm_sequence_base" {
        return false;
    }
    if first_named(ix, &name).is_some_and(|k| is_sequence(ix.d, k.id) || derived(ix.d, k.id, "uvm_sequence_base")) {
        return true;
    }
    crate::support::uvm_syntax::base_name_node(c, cls).is_some_and(|b| c.text(b).contains("uvm_sequence"))
}

/// Routines whose innermost class passes `test` (out-of-block ones have none).
pub(crate) fn routines_in<'c>(ix: &Ix, c: &'c Cst, test: fn(&Ix, &Cst, NodeId) -> bool) -> Vec<(String, NodeId)> {
    let mut memo: HashMap<NodeId, bool> = HashMap::new();
    let mut out = Vec::new();
    for f in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        let Some(cls) = enclosing_class(c, f) else { continue };
        let ok = *memo.entry(cls).or_insert_with(|| test(ix, c, cls));
        if ok {
            out.push((decl_name(c, cls), f));
        }
    }
    out
}

/// `task body` in a sequence class.
pub(crate) fn sequence_bodies(ix: &Ix, c: &Cst) -> Vec<(String, NodeId)> {
    routines_in(ix, c, seq_decl)
        .into_iter()
        .filter(|&(_, f)| c.kind(f) == TaskDecl && name_node(c, f).is_some_and(|n| c.trimmed(n) == "body"))
        .collect()
}

pub(crate) fn is_join_none(c: &Cst, n: NodeId) -> bool {
    c.kind(n) == ParBlock && c.tok(n, "join_none").is_some()
}

pub(crate) fn is_scoreboard(d: &Design, cls: &Class) -> bool {
    let l = cls.name.to_ascii_lowercase();
    l.contains("scoreboard") || l.contains("scb") || derived(d, cls.id, "uvm_scoreboard")
}

/// Assignment operators (slang's AssignmentExpression), with `<=` only as
/// a statement.
pub(crate) fn assign_parts(c: &Cst, n: NodeId) -> Option<(NodeId, NodeId)> {
    if c.kind(n) != BinaryExpr {
        return None;
    }
    let op = c.toks(n).next()?;
    let o = c.text_of(op);
    let ok = matches!(o, "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" | "<<<=" | ">>>=")
        || (o == "<=" && c.parent(n).is_some_and(|p| c.kind(p) == ExprStmt));
    if !ok {
        return None;
    }
    let mut it = c.tree.children(n);
    Some((it.next()?, it.next()?))
}

/// An integer literal's value, when it has no x or z bits.
pub(crate) fn int_value(text: &str) -> Option<u128> {
    let t: String = text.chars().filter(|c| *c != '_' && !c.is_whitespace()).collect();
    if let Some(q) = t.find('\'') {
        let rest = &t[q + 1..];
        let rest = rest.trim_start_matches(['s', 'S']);
        let (radix, digits) = match rest.chars().next()? {
            'd' | 'D' => (10, &rest[1..]),
            'h' | 'H' => (16, &rest[1..]),
            'b' | 'B' => (2, &rest[1..]),
            'o' | 'O' => (8, &rest[1..]),
            _ => return None,
        };
        return u128::from_str_radix(digits, radix).ok();
    }
    t.parse().ok()
}

pub(crate) fn unused(d: &Design, csts: &[Cst], s: &mut Sink, role: Role, id: &'static str, name: &'static str, noun: &str) {
    let ix = Ix::new(d, csts);
    let users = user_classes(d);
    if !users.iter().any(|c| class_role(d, c.id) == Role::Test) {
        return;
    }
    for cls in &users {
        if class_role(d, cls.id) != role || cls.is_virtual {
            continue;
        }
        if users.iter().any(|h| mentions(&ix, cls, h)) {
            continue;
        }
        s.add(
            id,
            name,
            Severity::Warning,
            format!(
                "{noun} '{}' is never named by another class in this run, so nothing creates or starts it. \
                 Create it from the test, or remove it. Experimental: a use in a file that was \
                 not linted is invisible here.",
                cls.name
            ),
            cls.loc,
        );
    }
}

/// `mentionedFrom`: `host` derives from `target` or names it in a property,
/// a signature or a call's result.
pub(crate) fn mentions(ix: &Ix, target: &Class, host: &Class) -> bool {
    let d = ix.d;
    if host.id == target.id {
        return false;
    }
    if d.ancestors(host.id).any(|a| a.id == target.id) {
        return true;
    }
    let is_t = |ty: &str| class_named(d, type_head(ty)).is_some_and(|k| k.id == target.id);
    if host.properties.iter().any(|p| p.type_class == Some(target.id) || (p.type_class.is_none() && is_t(&p.ty))) {
        return true;
    }
    for m in &host.methods {
        if m.is_builtin {
            continue;
        }
        if is_t(&m.return_type) || m.arguments.iter().any(|a| a.type_class == Some(target.id)) {
            return true;
        }
        // Locals never count: the C++ visitor does not reach them.
    }
    // Calls whose result is the target: T::type_id::create(...) and the like.
    for at in ix.all_routines(host.id) {
        let c = ix.cst(at);
        for n in crate::support::uvm_syntax::subtree(c, at.node).into_iter().filter(|&x| c.kind(x) == CallExpr) {
            let Some(left) = c.tree.children(n).next() else { continue };
            if c.kind(left) != ScopedExpr || c.last_text(left) != "create" {
                continue;
            }
            let toks = c.tree.token_range(left);
            if toks.first().is_some_and(|&t| c.text_of(t) == target.name) && c.text(left).contains("type_id") {
                return true;
            }
        }
    }
    false
}
