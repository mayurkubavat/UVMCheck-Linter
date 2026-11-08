//! SV-BUG-004 (MismatchedFieldInCopyCompare): In do_copy/do_compare and similar
//! field-wise methods, flags a field of this object paired with a differently
//! named field of the other object when the matching field exists.

use std::collections::HashMap;
use sv_syntax::{NodeId, SyntaxKind};
use crate::support::slang_ast::{class_methods, decl_name, user_classes};
use crate::support::class_index::Classes;
use crate::support::class_items::{declarators, kids, op};
use crate::support::type_text::expr_text;
use crate::support::data_items::{strip_conv, unparen};
use crate::support::statements::in_bad_body;
use crate::support::sema_facts::ast_ok;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;

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

pub(crate) fn mismatched_field(c: &Cst, s: &mut Sink) {
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
            for d in crate::support::statements::subtree(c, m) {
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
            for e in crate::support::statements::subtree(c, body) {
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
