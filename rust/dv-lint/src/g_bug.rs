//! Correctness rules ported after slice 3: SV-BUG-001 (identical branches)
//! and SV-BUG-004 (field missing from a copy or compare).

use std::collections::HashMap;

use sv_syntax::{NodeId, SyntaxKind};

use crate::g_c_support::{class_methods, decl_name, user_classes};
use crate::g_design_class::Classes;
use crate::g_random::{declarators, kids, op};
use crate::g_random_bug::expr_text;
use crate::g_random_oop::{strip_conv, unparen};

use crate::g_design::{case_parts, if_parts, in_bad_body, list, report, stmt, St};
use crate::g_design_class::{count_statements, strongest_report, Level};
use crate::g_random_sema::ast_ok;
use crate::g_sva_cov::user_code_times;
use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "SV-BUG-001",
            name: "IdenticalBranches",
            check: Syntax(identical_branches),
        },
        Rule {
            id: "SV-BUG-004",
            name: "MismatchedFieldInCopyCompare",
            check: Syntax(mismatched_field),
        },
        Rule {
            id: "SV-OOP-013",
            name: "AbstractClassInstantiation",
            check: Syntax(abstract_new),
        },
    ]
}

// ---- swe::originalText ----

/// The end offset of the macro use whose backtick is at `at`: past its
/// name and, when one follows, its parenthesized arguments.
fn macro_use_end(src: &str, at: usize) -> usize {
    let b = src.as_bytes();
    let mut i = at + 1;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
        i += 1;
    }
    let mut j = i;
    while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
        j += 1;
    }
    if j >= b.len() || b[j] != b'(' {
        return i;
    }
    let mut depth = 0;
    while j < b.len() {
        match b[j] {
            b'"' => {
                j += 1;
                while j < b.len() && b[j] != b'"' {
                    if b[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return j + 1;
                }
            }
            _ => {}
        }
        j += 1;
    }
    b.len()
}

/// `stripCommentsAndSpace`: comments dropped, whitespace runs one space,
/// string literals verbatim.
fn strip_comments_and_space(src: &str) -> String {
    let b = src.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut space = false;
    let mut i = 0;
    while i < b.len() {
        let ch = b[i];
        if ch == b'"' {
            if space && !out.is_empty() {
                out.push(b' ');
            }
            space = false;
            out.push(ch);
            i += 1;
            while i < b.len() {
                out.push(b[i]);
                if b[i] == b'\\' && i + 1 < b.len() {
                    i += 1;
                    out.push(b[i]);
                    i += 1;
                    continue;
                }
                if b[i] == b'"' {
                    break;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        if ch == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            space = true;
            i += 1;
            continue;
        }
        if ch == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            space = true;
            continue;
        }
        if ch.is_ascii_whitespace() {
            space = true;
            i += 1;
            continue;
        }
        if space && !out.is_empty() {
            out.push(b' ');
        }
        space = false;
        out.push(ch);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `swe::originalText`: the statement as written in its file, macro uses
/// unexpanded, comments removed and whitespace collapsed.
pub(crate) fn original_text(c: &Cst, n: NodeId) -> String {
    let fallback = || strip_comments_and_space(&c.text(n));
    let Some((a, b)) = c.tree.token_bounds(n) else { return String::new() };
    let ta = &c.tree.tokens[a as usize];
    let tb = &c.tree.tokens[b as usize];
    if ta.origin.file != tb.origin.file {
        return fallback();
    }
    let Some(src) = c.tree.sources.get(ta.origin.file as usize) else { return fallback() };
    let start = ta.origin.offset as usize;
    let end = if tb.from_macro {
        macro_use_end(src, tb.origin.offset as usize)
    } else {
        tb.origin.offset as usize + tb.text.len()
    };
    if end < start || end > src.len() {
        return fallback();
    }
    strip_comments_and_space(&src[start..end])
}

// ---- SV-BUG-001 ----

/// Lone return/break/continue statements, empty blocks and report-only
/// branches are commonly repeated on purpose.
fn trivial_branch(c: &Cst, n: NodeId) -> bool {
    let mut s = n;
    while matches!(stmt(c, s), Some(St::Seq | St::Par(_))) {
        let body = list(c, s);
        match body.as_slice() {
            [] => return true,
            [one] => s = *one,
            _ => break,
        }
    }
    if matches!(stmt(c, s), Some(St::Empty | St::Return | St::Break | St::Continue)) {
        return true;
    }
    // A block of several statements counts each twice in slang's walk
    // (measured: two statements with a report are not trivial).
    let k = if matches!(stmt(c, s), Some(St::Seq | St::Par(_))) { 2 } else { 1 };
    k * count_statements(c, s) <= 2 && strongest_report(c, s) != Level::None
}

fn single_macro_call(text: &str) -> bool {
    if !text.starts_with("`uvm_") {
        return false;
    }
    match text.find(';') {
        None => true,
        Some(i) => i + 1 == text.len(),
    }
}

/// The statement slang binds for a branch: an unnamed `begin ... end`
/// holding one statement and no declarations is that statement.
fn bound_stmt(c: &Cst, n: NodeId) -> NodeId {
    let mut s = n;
    while c.kind(s) == SeqBlock && !c.toks(s).any(|t| c.text_of(t) == ":") && c.tree.child(s, StmtLabel).is_none() {
        match list(c, s).as_slice() {
            [one] if stmt(c, *one) != Some(St::Decl) => s = *one,
            _ => break,
        }
    }
    s
}

/// An `if` that is the `else` of another `if` (part of its chain).
fn chained(c: &Cst, n: NodeId) -> bool {
    c.parent(n)
        .filter(|&p| c.kind(p) == ElseClause)
        .and_then(|p| c.parent(p))
        .is_some_and(|g| c.kind(g) == IfStmt)
}

fn identical_branches(c: &Cst, s: &mut Sink) {
    let mut stmts: Vec<NodeId> = c.nodes(IfStmt);
    stmts.extend(c.nodes(CaseStmt));
    stmts.sort_by_key(|n| n.0);
    for n in stmts {
        let what = match stmt(c, n) {
            Some(St::If) if !chained(c, n) => "if/else",
            Some(St::Case) => "case",
            _ => continue,
        };
        let times = user_code_times(c, n);
        if times == 0 || in_bad_body(c, n) {
            continue;
        }
        let mut branches = Vec::new();
        if what == "case" {
            let p = case_parts(c, n);
            if p.sel.is_some_and(|e| !ast_ok(c, e)) {
                continue;
            }
            branches.extend(p.items.iter().map(|(_, b)| *b));
            branches.extend(p.default);
        } else {
            let mut cur = n;
            let mut ok = true;
            loop {
                let (cond, then, els) = if_parts(c, cur);
                if cond.is_some_and(|e| !ast_ok(c, e)) {
                    ok = false;
                    break;
                }
                branches.extend(then);
                match els {
                    Some(e) if c.kind(e) == IfStmt => cur = e,
                    Some(e) => {
                        branches.push(e);
                        break;
                    }
                    None => break,
                }
            }
            if !ok {
                continue;
            }
        }
        let branches: Vec<NodeId> = branches.into_iter().map(|b| bound_stmt(c, b)).collect();
        let texts: Vec<String> = branches.iter().map(|&b| original_text(c, b)).collect();
        for i in 1..branches.len() {
            if texts[i].is_empty() || trivial_branch(c, branches[i]) || single_macro_call(&texts[i]) {
                continue;
            }
            let Some(j) = (0..i).find(|&j| texts[j] == texts[i]) else { continue };
            let (Some(ti), Some(tj)) = (c.first_tok(branches[i]), c.first_tok(branches[j])) else {
                continue;
            };
            let lj = c.loc(tj);
            let line = lj.expansion.map_or(lj.line, |e| e.1);
            report(
                s,
                times,
                "SV-BUG-001",
                "IdenticalBranches",
                Severity::Warning,
                format!(
                    "This {what} branch is identical to the branch at line {line}. A pasted branch that was never \
                     edited is the usual cause; if both really do the same thing, merge the conditions."
                ),
                c.loc(ti),
            );
        }
    }
}

// ---- SV-BUG-004 ----

fn fieldwise(name: &str) -> bool {
    name.starts_with("do_") || name.contains("copy") || name.contains("compare") || name.contains("equal")
}

/// One method body being checked: the class index, the class, and the
/// routine's formals and locals with their declared types.
struct Pairing<'a, 'c, 't> {
    c: &'c Cst<'t>,
    classes: &'a Classes,
    cls: usize,
    locals: HashMap<String, Option<NodeId>>,
}

impl Pairing<'_, '_, '_> {
    /// A class property of class `i` or a class it extends.
    fn property(&self, i: usize, name: &str) -> Option<Option<NodeId>> {
        let mut cur = Some(i);
        for _ in 0..64 {
            let k = cur?;
            if let Some(p) = self.classes.list[k].props.iter().find(|p| p.name == name) {
                return Some(p.ty);
            }
            cur = self.classes.base(k);
        }
        None
    }

    fn strip(&self, e: NodeId) -> NodeId {
        let c = self.c;
        let mut e = unparen(c, strip_conv(c, unparen(c, e)).0);
        while c.kind(e) == SelectExpr {
            match c.tree.children(e).next() {
                Some(x) => e = unparen(c, strip_conv(c, unparen(c, x)).0),
                None => break,
            }
        }
        e
    }

    /// `addr` or `this.addr` (or `super.addr`): a property of this object.
    fn own_field(&self, e: NodeId) -> Option<String> {
        let c = self.c;
        let e = self.strip(e);
        match c.kind(e) {
            NameExpr => {
                let toks: Vec<u32> = c.toks(e).collect();
                let [t] = toks[..] else { return None };
                let name = c.text_of(t);
                if self.locals.contains_key(name) {
                    return None;
                }
                self.property(self.cls, name).map(|_| name.to_string())
            }
            MemberExpr => {
                let base = c.tree.children(e).next()?;
                let base = unparen(c, base);
                if c.kind(base) != NameExpr || !matches!(c.tree.bare_text(base).as_str(), "this" | "super") {
                    return None;
                }
                let (_, last) = c.tree.token_bounds(e)?;
                let name = c.text_of(last);
                self.property(self.cls, name).map(|_| name.to_string())
            }
            _ => None,
        }
    }

    /// `rhs_.addr`: a property of another handle, and that handle's class.
    fn other_field(&self, e: NodeId) -> Option<(String, usize)> {
        let c = self.c;
        let e = self.strip(e);
        if c.kind(e) != MemberExpr {
            return None;
        }
        let base = self.strip(c.tree.children(e).next()?);
        if c.kind(base) != NameExpr {
            return None;
        }
        let toks: Vec<u32> = c.toks(base).collect();
        let [t] = toks[..] else { return None };
        let h = c.text_of(t);
        if matches!(h, "this" | "super") {
            return None;
        }
        let ty = match self.locals.get(h) {
            Some(ty) => *ty,
            None => self.property(self.cls, h)?,
        }?;
        let tname = c.tree.bare_text(ty);
        let k = self.classes.find(tname.split('#').next().unwrap_or(""))?;
        let (_, last) = c.tree.token_bounds(e)?;
        let m = c.text_of(last);
        self.property(k, m)?;
        Some((m.to_string(), k))
    }

    /// The declared type text of a field, as written.
    fn field_type(&self, cls: usize, name: &str) -> Option<String> {
        let ty = self.property(cls, name)??;
        Some(self.c.tree.bare_text(ty).split_whitespace().collect())
    }

    fn check(&self, mine: NodeId, theirs: NodeId, at: NodeId, how: How, method: &str, s: &mut Sink) {
        let c = self.c;
        let Some(a) = self.own_field(mine) else { return };
        let Some((b, k)) = self.other_field(theirs) else { return };
        if a == b || self.property(k, &a).is_none() {
            return;
        }
        let Some(tok) = c.first_tok(at) else { return };
        // slang's exprText of an implicitly converted operand is empty.
        let ta = self.field_type(self.cls, &a);
        let tb = self.field_type(k, &b);
        let converted = match (how, &ta, &tb) {
            (How::Call, _, _) => false,
            (_, Some(x), Some(y)) if x == y => false,
            (How::Equality, Some(x), Some(y)) => match (bit_width(x), bit_width(y)) {
                (Some(wx), Some(wy)) => wy <= wx,
                _ => true,
            },
            _ => true,
        };
        // An explicit cast is a conversion too; parentheses stay in the text.
        let cast = c.kind(unparen(c, theirs)) == CastExpr;
        let text = if converted || cast { String::new() } else { expr_text(c, theirs) };
        s.add(
            "SV-BUG-004",
            "MismatchedFieldInCopyCompare",
            Severity::Error,
            format!(
                "In {method}(), field '{a}' is paired with '{}' instead of the matching field '{a}'. Field-wise \
                 copy and compare code is usually pasted line by line; check this line.",
                text
            ),
            c.loc(tok),
        );
    }
}

#[derive(Clone, Copy)]
enum How {
    Assign,
    Equality,
    Call,
}

/// Bits of a simple integral type written as text (`bit[31:0]`, `int`).
fn bit_width(t: &str) -> Option<u64> {
    let (kw, rest) = match t.find('[') {
        Some(i) => (&t[..i], &t[i..]),
        None => (t, ""),
    };
    let kw = kw.trim_end_matches("signed").trim_end_matches("un");
    let mut w: u64 = match kw {
        "bit" | "logic" | "reg" => 1,
        "byte" => 8,
        "shortint" => 16,
        "int" | "integer" => 32,
        "longint" => 64,
        _ => return None,
    };
    let mut r = rest;
    while let Some(body) = r.strip_prefix('[') {
        let end = body.find(']')?;
        let (a, b) = body[..end].split_once(':')?;
        let (a, b): (i64, i64) = (a.parse().ok()?, b.parse().ok()?);
        w *= a.abs_diff(b) + 1;
        r = &body[end + 1..];
    }
    r.is_empty().then_some(w)
}

/// Compound assignments are left out: slang's right operand is the whole
/// `a op b` (measured).
const ASSIGN_OPS: &[&str] = &["="];

fn mismatched_field(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for cls in user_classes(c) {
        let Some(&ci) = classes.list.iter().position(|k| k.node == cls).as_ref() else { continue };
        for m in class_methods(c, cls) {
            let full = decl_name(c, m);
            let method = full.rsplit("::").next().unwrap_or("").to_string();
            if !fieldwise(&method) || in_bad_body(c, m) {
                continue;
            }
            let mut locals = HashMap::new();
            for d in crate::g_design::subtree(c, m) {
                if !matches!(c.kind(d), DataDecl | Port) {
                    continue;
                }
                let ty = c.tree.child(d, DataType);
                for (_, v, _) in declarators(c, d) {
                    locals.insert(v.to_string(), ty);
                }
            }
            let p = Pairing {
                c,
                classes: &classes,
                cls: ci,
                locals,
            };
            let Some(body) = c.tree.child(m, Body) else { continue };
            for e in crate::g_design::subtree(c, body) {
                match c.kind(e) {
                    BinaryExpr => {
                        let o = op(c, e);
                        let k = kids(c, e);
                        let (Some(&l), Some(&r)) = (k.first(), k.get(1)) else { continue };
                        let nonblocking = o == "<=" && c.parent(e).is_some_and(|x| c.kind(x) == ExprStmt);
                        if ASSIGN_OPS.contains(&o) || nonblocking {
                            if ast_ok(c, e) {
                                p.check(l, r, l, How::Assign, &method, s);
                            }
                        } else if matches!(o, "==" | "!=" | "===" | "!==") && ast_ok(c, e) {
                            p.check(l, r, e, How::Equality, &method, s);
                            p.check(r, l, e, How::Equality, &method, s);
                        }
                    }
                    CallExpr => {
                        if !ast_ok(c, e) {
                            continue;
                        }
                        let Some(al) = c.tree.child(e, ArgList) else { continue };
                        let mut own = None;
                        let mut other = None;
                        let (mut owns, mut others) = (0, 0);
                        for a in c.tree.children(al).filter(|&x| c.kind(x) == Arg) {
                            let Some(v) = c.tree.children(a).next() else { continue };
                            if p.own_field(v).is_some() {
                                own = Some(v);
                                owns += 1;
                            } else if p.other_field(v).is_some() {
                                other = Some(v);
                                others += 1;
                            }
                        }
                        if let (1, 1, Some(o), Some(t)) = (owns, others, own, other) {
                            p.check(o, t, e, How::Call, &method, s);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

// ---- SV-OOP-013 ----

/// The declared type of the variable a name stands for at `at`: a formal
/// or local of the routine around it, a property of the class around it
/// (or a class it extends), else a declaration of an enclosing scope.
pub(crate) fn declared_type(c: &Cst, classes: &Classes, at: NodeId, name: &str) -> Option<NodeId> {
    let mut x = at;
    while let Some(p) = c.parent(x) {
        match c.kind(p) {
            FunctionDecl | TaskDecl => {
                for d in crate::g_design::subtree(c, p) {
                    if matches!(c.kind(d), DataDecl | Port) && declarators(c, d).iter().any(|(_, v, _)| v == name) {
                        return c.tree.child(d, DataType);
                    }
                }
                // `function C::f`: the members of C.
                let full = decl_name(c, p);
                if let Some((cls, _)) = full.rsplit_once("::") {
                    if let Some(k) = classes.find(cls) {
                        if let Some(t) = class_property(classes, k, name) {
                            return t;
                        }
                    }
                }
            }
            ClassDecl => {
                let k = classes.list.iter().position(|k| k.node == p)?;
                if let Some(t) = class_property(classes, k, name) {
                    return t;
                }
            }
            SeqBlock | ParBlock | ModuleDecl | InterfaceDecl | ProgramDecl | PackageDecl | SourceFile => {
                for d in kids(c, p) {
                    if c.kind(d) == DataDecl && declarators(c, d).iter().any(|(_, v, _)| v == name) {
                        return c.tree.child(d, DataType);
                    }
                }
            }
            _ => {}
        }
        x = p;
    }
    None
}

fn class_property(classes: &Classes, i: usize, name: &str) -> Option<Option<NodeId>> {
    let mut cur = Some(i);
    for _ in 0..64 {
        let k = cur?;
        if let Some(p) = classes.list[k].props.iter().find(|p| p.name == name) {
            return Some(p.ty);
        }
        cur = classes.base(k);
    }
    None
}

/// The type a `new` constructs: the declared type of what it is assigned
/// to (`x = new`, `T x = new`).
fn new_target(c: &Cst, classes: &Classes, n: NodeId) -> Option<NodeId> {
    let p = c.parent(n)?;
    match c.kind(p) {
        BinaryExpr if op(c, p) == "=" && kids(c, p).get(1) == Some(&n) => {
            let lhs = unparen(c, *kids(c, p).first()?);
            match c.kind(lhs) {
                NameExpr => {
                    let toks: Vec<u32> = c.toks(lhs).collect();
                    let [t] = toks[..] else { return None };
                    declared_type(c, classes, n, c.text_of(t))
                }
                MemberExpr => {
                    let base = c.tree.children(lhs).next()?;
                    if c.kind(base) != NameExpr || c.tree.bare_text(base) != "this" {
                        return None;
                    }
                    let (_, last) = c.tree.token_bounds(lhs)?;
                    let k = classes.enclosing(c, n)?;
                    class_property(classes, k, c.text_of(last))?
                }
                _ => None,
            }
        }
        Declarator => {
            let d = c.parent(p)?;
            (c.kind(d) == DataDecl).then(|| c.tree.child(d, DataType)).flatten()
        }
        _ => None,
    }
}

/// slang's NewVirtualClass error, once per location (diagnostics of every
/// specialization are merged by location).
fn abstract_new(c: &Cst, s: &mut Sink) {
    let news = c.nodes(NewExpr);
    if news.is_empty() {
        return;
    }
    let classes = Classes::new(c);
    let trees = std::slice::from_ref(c.tree);
    let mut model: Option<sv_sema::SpecModel> = None;
    for n in news {
        // `new[n]`, `new obj` (a copy) and `super.new` call no constructor.
        if c.toks(n).any(|t| c.text_of(t) == "[") || c.tree.child(n, ArgList).is_none() && kids(c, n).len() > 0 {
            continue;
        }
        if c.parent(n).is_some_and(|p| c.kind(p) == MemberExpr) {
            continue;
        }
        let Some(dt) = new_target(c, &classes, n) else { continue };
        let m = model.get_or_insert_with(|| sv_sema::SpecModel::new(trees));
        let any_virtual = m.class_per_named_spec(0, dt).into_iter().flatten().any(|(_, cls)| {
            c.toks(cls).any(|t| c.text_of(t) == "virtual") && c.toks(cls).any(|t| c.text_of(t) == "class")
        });
        if !any_virtual {
            continue;
        }
        let Some(at) = c.first_tok(n) else { continue };
        s.add(
            "SV-OOP-013",
            "AbstractClassInstantiation",
            Severity::Error,
            "Cannot instantiate abstract (virtual) class via 'new()'. In SystemVerilog (IEEE 1800 § 8.21), virtual \
             classes cannot have direct instances; instantiate a concrete derived class instead."
                .to_string(),
            c.loc(at),
        );
    }
}
