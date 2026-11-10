//! Expression types as slang's `Expression::type` gives them to the C++
//! rules (IEEE 1800 11.6 and 11.8): each expression's self-determined type,
//! then the type context-determined operators take from their context
//! (comparison operands, assignment right-hand sides, arithmetic operands).
//! Declared types come from sv-semantic's [`TypeOracle`], printed as slang's
//! `Type::toString()`. Gated against slang by tools/gates/expr_gate.py
//! (tools/oracles/exprdump.cpp is the reference).

use std::sync::Arc;
use svlang::semantic::fx::FxMap as HashMap;

use svlang::semantic::oracle::{TypeInfo, TypeOracle};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

use crate::support::slang_ast::{kids, lookup, DeclKind};
use crate::syntax::Cst;

/// A type: its text and, when integral, (width, signed, four-state).
pub(crate) type ETy = TypeInfo;

/// `bit`/`logic` vector of a width, as slang's `getType(width, flags)` prints it.
pub(crate) fn vector(w: u32, s: bool, four: bool) -> ETy {
    let base = if four { "logic" } else { "bit" };
    let sign = if s { " signed" } else { "" };
    let text = if w <= 1 { format!("{base}{sign}") } else { format!("{base}{sign}[{}:0]", w - 1) };
    ETy { text, integral: Some((w.max(1), s, four)), src: None, aggregate: false }
}

/// A vector with an explicit range, as literals and casts print (`bit[0:0]`).
fn ranged(w: u32, s: bool, four: bool) -> ETy {
    let base = if four { "logic" } else { "bit" };
    let sign = if s { " signed" } else { "" };
    ETy {
        text: format!("{base}{sign}[{}:0]", w.max(1) - 1),
        integral: Some((w.max(1), s, four)),
        src: None,
        aggregate: false,
    }
}

/// A simple bit vector type slang treats as matching another of the same
/// shape (not an enum or struct).
fn simple(t: &ETy) -> bool {
    !(t.text.starts_with("enum") || t.text.starts_with("struct") || t.text.starts_with("union")) && t.integral.is_some()
        && !t.aggregate
        // A part-select type with its own bounds (`logic[31:16]`).
        && !(t.text.ends_with(']') && t.text.rsplit_once(':').is_some_and(|(_, lo)| lo != "0]"))
}

/// slang's `binaryOperatorType`: the wider width, signed only when both are,
/// four-state when either is; an operand's own type when it has that shape.
pub(crate) fn binary_type(l: &ETy, r: &ETy, force_four: bool) -> Option<ETy> {
    let (lw, ls, l4) = l.integral?;
    let (rw, rs, r4) = r.integral?;
    let shape = (lw.max(rw), ls && rs, l4 || r4 || force_four);
    if l.integral == Some(shape) && simple(l) {
        return Some(l.clone());
    }
    if r.integral == Some(shape) && simple(r) {
        return Some(r.clone());
    }
    if l.integral == Some(shape) && !force_four && l.text == r.text {
        return Some(l.clone());
    }
    Some(vector(shape.0, shape.1, shape.2))
}

const ARITH: &[&str] = &["+", "-", "*", "/", "%", "&", "|", "^", "^~", "~^"];
const COMPARE: &[&str] = &["==", "!=", "===", "!==", "==?", "!=?", "<", "<=", ">", ">="];
const LOGICAL: &[&str] = &["&&", "||", "->", "<->"];
const SHIFT: &[&str] = &["<<", ">>", "<<<", ">>>", "**"];
const ASSIGN: &[&str] = &["=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=", "<<<=", ">>>="];

pub(crate) struct Typer<'a, 't> {
    c: &'a Cst<'t>,
    o: &'a TypeOracle<'t>,
    memo: HashMap<NodeId, Option<ETy>>,
    fmemo: HashMap<NodeId, Option<ETy>>,
}

fn op_of(c: &Cst, e: NodeId) -> &'static str {
    let t = c.keyword(e).map_or("", |k| c.text_of(k));
    for set in [ARITH, COMPARE, LOGICAL, SHIFT, ASSIGN] {
        if let Some(x) = set.iter().find(|x| **x == t) {
            return x;
        }
    }
    ""
}

/// `<=` is an assignment as a statement's expression, a comparison anywhere else.
fn is_assign(c: &Cst, e: NodeId) -> bool {
    let op = op_of(c, e);
    if op == "<=" {
        return c.parent(e).is_some_and(|p| c.kind(p) == ExprStmt);
    }
    ASSIGN.contains(&op)
}

impl<'a, 't> Typer<'a, 't> {
    pub(crate) fn new(c: &'a Cst<'t>, o: &'a TypeOracle<'t>) -> Self {
        Typer { c, o, memo: HashMap::default(), fmemo: HashMap::default() }
    }

    /// The type of `e` as the AST holds it: self-determined, or taken
    /// from its context for context-determined operators.
    pub(crate) fn final_type(&mut self, e: NodeId) -> Option<ETy> {
        if let Some(t) = self.fmemo.get(&e) {
            return t.clone();
        }
        let t = self.final_uncached(e);
        self.fmemo.insert(e, t.clone());
        t
    }

    fn final_uncached(&mut self, e: NodeId) -> Option<ETy> {
        let own = self.self_type(e)?;
        if !self.propagates(e) {
            return Some(own);
        }
        match self.context(e) {
            Some(ctx) if ctx.integral.is_some() && own.integral.is_some() => Some(ctx),
            _ => Some(own),
        }
    }

    /// An operator whose type its context sets (slang's `propagateType`).
    fn propagates(&self, e: NodeId) -> bool {
        let c = self.c;
        match c.kind(e) {
            BinaryExpr => {
                let op = op_of(c, e);
                !is_assign(c, e) && (ARITH.contains(&op) || SHIFT.contains(&op))
            }
            PrefixExpr => matches!(c.keyword(e).map(|k| c.text_of(k)), Some("-" | "+" | "~")),
            CondExpr => true,
            Literal => unbased(c, e),
            ParenExpr => kids(c, e).first().is_some_and(|&i| self.propagates(i)),
            _ => false,
        }
    }

    /// The type the parent gives a context-determined `e`.
    fn context(&mut self, e: NodeId) -> Option<ETy> {
        let c = self.c;
        let p = c.parent(e)?;
        let ks = kids(c, p);
        match c.kind(p) {
            ParenExpr => {
                if self.propagates(p) {
                    self.final_type(p)
                } else {
                    None
                }
            }
            BinaryExpr => {
                let op = op_of(c, p);
                if is_assign(c, p) {
                    if ks.get(1) != Some(&e) {
                        return None;
                    }
                    let l = self.final_type(ks[0])?;
                    if op != "=" && op != "<=" {
                        // `a op= b` is `a = a op b`.
                        if !ARITH.iter().any(|a| op.strip_suffix('=') == Some(*a)) {
                            return None;
                        }
                        let r = self.self_type(e)?;
                        if unbased(c, e) {
                            return self.assigned(l, e);
                        }
                        return binary_type(&l, &r, matches!(op, "/=" | "%="));
                    }
                    return self.assigned(l, e);
                }
                if COMPARE.contains(&op) {
                    let l = self.self_type(ks[0])?;
                    let r = self.self_type(*ks.get(1)?)?;
                    return binary_type(&l, &r, false);
                }
                if ARITH.contains(&op) {
                    return self.final_type(p);
                }
                if SHIFT.contains(&op) && ks.first() == Some(&e) {
                    return self.final_type(p);
                }
                None
            }
            PrefixExpr if self.propagates(p) => self.final_type(p),
            // `T'(x)` and `8'(x)`: x is assigned to the target type.
            CastExpr if ks.get(1) == Some(&e) => {
                let target = ks[0];
                let sign = matches!(c.tree.bare_text(target).as_str(), "signed" | "unsigned" | "const");
                if sign {
                    return None;
                }
                let l = self.self_type(p)?;
                self.assigned(l, e)
            }
            Declarator => {
                // A variable's initial value.
                let dd = c.parent(p)?;
                if !matches!(c.kind(dd), DataDecl | ParamDecl) || kids(c, p).last() != Some(&e) {
                    return None;
                }
                let dt = c.tree.child(dd, DataType);
                // An untyped parameter takes its value's type.
                if c.kind(dd) == ParamDecl && dt.is_none_or(|d| c.tree.token_bounds(d).is_none()) {
                    let l = self.o.value_type(c.index(), e);
                    return self.assigned(l, e);
                }
                let l = self.o.decl_type(c.index(), dt, Some(p));
                self.assigned(l, e)
            }
            ReturnStmt => {
                let mut f = c.parent(p);
                while let Some(x) = f {
                    if c.kind(x) == FunctionDecl {
                        break;
                    }
                    f = c.parent(x);
                }
                let dt = c.tree.child(f?, ReturnType).and_then(|r| c.tree.child(r, DataType))?;
                let l = self.o.data_type(c.index(), dt);
                self.assigned(l, e)
            }
            CondExpr if ks.first() != Some(&e) => self.final_type(p),
            _ => None,
        }
    }

    /// The type a context-determined `e` takes when assigned to type `l`.
    fn assigned(&mut self, l: ETy, e: NodeId) -> Option<ETy> {
        let c = self.c;
        let r = self.self_type(e)?;
        let (lw, ls, l4) = l.integral?;
        if c.kind(e) == Literal && unbased(c, e) {
            // `'1` takes the target's type, unsigned (a vector of its width
            // for an enum, struct or union).
            return Some(if ls || l.aggregate { vector(lw, false, l4) } else { l });
        }
        let (rw, rs, r4) = r.integral?;
        let shape = (lw.max(rw), rs, l4 || r4);
        if r.integral == Some(shape) && (simple(&r) || same_type(&r, &l)) {
            return Some(r);
        }
        if l.integral == Some(shape) && simple(&l) {
            return Some(l);
        }
        Some(vector(shape.0, shape.1, shape.2))
    }

    /// The self-determined type.
    pub(crate) fn self_type(&mut self, e: NodeId) -> Option<ETy> {
        if let Some(t) = self.memo.get(&e) {
            return t.clone();
        }
        let t = self.compute(e);
        self.memo.insert(e, t.clone());
        t
    }

    fn compute(&mut self, e: NodeId) -> Option<ETy> {
        let c = self.c;
        let ks = kids(c, e);
        match c.kind(e) {
            ParenExpr => self.self_type(*ks.first()?),
            Literal => literal(&c.tree.bare_text(e)),
            NameExpr => self.name_type(e),
            ScopedExpr => {
                // `pkg::name`.
                let toks: Vec<&str> = c.tree.token_range(e).iter().map(|&t| c.text_of(t)).collect();
                match toks.as_slice() {
                    [pkg, "::", name] => self.o.package_member(pkg, name),
                    _ => None,
                }
            }
            BinaryExpr => {
                let op = op_of(c, e);
                if is_assign(c, e) {
                    return self.final_type(*ks.first()?);
                }
                let l = self.self_type(*ks.first()?)?;
                let r = self.self_type(*ks.get(1)?);
                if COMPARE.contains(&op) {
                    let r = r?;
                    let four = !matches!(op, "===" | "!==") && (four(&l) || four(&r));
                    return Some(vector(1, false, four));
                }
                if LOGICAL.contains(&op) {
                    let four = four(&l) || r.as_ref().is_some_and(four);
                    return Some(vector(1, false, four));
                }
                if op == "**" {
                    if l.text == "real" || r.as_ref().is_some_and(|r| r.text == "real") {
                        return Some(ETy::other("real"));
                    }
                    // x for a negative or zero base: four states.
                    let (w, sg, f) = l.integral?;
                    return Some(if f { l } else { vector(w, sg, true) });
                }
                if SHIFT.contains(&op) {
                    return l.integral.is_some().then_some(l);
                }
                if ARITH.contains(&op) {
                    if l.text == "real" || r.as_ref().is_some_and(|r| r.text == "real") {
                        return Some(ETy::other("real"));
                    }
                    // Division by zero gives x: four states.
                    return binary_type(&l, &r?, matches!(op, "/" | "%"));
                }
                None
            }
            PrefixExpr => {
                let op = c.keyword(e).map_or("", |k| c.text_of(k));
                let x = self.self_type(*ks.first()?)?;
                match op {
                    "!" | "&" | "|" | "^" | "~&" | "~|" | "~^" | "^~" => Some(vector(1, false, four(&x))),
                    "-" | "+" | "~" => x.integral.is_some().then_some(x),
                    _ => None,
                }
            }
            PostfixExpr => self.self_type(*ks.first()?),
            CondExpr => {
                let a = self.self_type(*ks.get(1)?)?;
                let b = self.self_type(*ks.get(2)?)?;
                if a.integral.is_none() || b.integral.is_none() {
                    return (a.text == b.text).then_some(a);
                }
                binary_type(&a, &b, false)
            }
            InsideExpr => {
                self.self_type(*ks.first()?)?;
                Some(vector(1, false, true))
            }
            ConcatExpr => {
                let commas = c.toks(e).filter(|&t| c.text_of(t) == ",").count();
                // `{n{x}}`: a count and one concatenation.
                if commas == 0 && ks.len() == 2 && c.kind(ks[1]) == ConcatExpr {
                    let (n, _, _) = self.o.eval(c.index(), ks[0])?;
                    let inner = self.self_type(ks[1])?;
                    let (w, _, f) = inner.integral?;
                    return Some(ranged((n.max(0) as u32).checked_mul(w)?, false, f));
                }
                let mut w = 0u32;
                let mut f = false;
                for k in ks {
                    let t = self.self_type(k)?;
                    let (kw, _, k4) = t.integral?;
                    w = w.checked_add(kw)?;
                    f |= k4;
                }
                Some(ranged(w, false, f))
            }
            SelectExpr => self.select_type(e),
            MemberExpr => self.member_type(e),
            CastExpr => self.cast_type(e),
            CallExpr => self.call_type(e),
            _ => None,
        }
    }

    fn decl_info(&mut self, d: crate::support::slang_ast::Decl) -> Option<ETy> {
        let c = self.c;
        let ix = c.index();
        if d.kind == DeclKind::Parameter {
            if c.kind(d.node) == ParamDecl {
                // A type parameter is not a value.
                return None;
            }
            if d.dtype.is_none()
                || c.tree.child(d.dtype?, Dim).is_none()
                    && c.toks(d.dtype?).all(|t| matches!(c.text_of(t), "signed" | "unsigned"))
            {
                // Untyped: the value's type.
                let v = kids(c, d.node).into_iter().find(|&k| !matches!(c.kind(k), Name | Dim))?;
                if d.dtype.is_none() {
                    return Some(self.o.value_type(ix, v));
                }
            }
        }
        Some(self.o.decl_type(ix, d.dtype, Some(d.node)))
    }

    fn name_type(&mut self, e: NodeId) -> Option<ETy> {
        let c = self.c;
        if !kids(c, e).is_empty() || c.toks(e).count() != 1 {
            return None;
        }
        let name = c.text_of(c.first_tok(e)?);
        if matches!(name, "this" | "super" | "null") {
            return None;
        }
        if let Some(d) = lookup(c, e, name) {
            return self.decl_info(d);
        }
        // A generate loop's genvar: integer.
        let genvar = crate::support::slang_ast::ancestors(c, e).filter(|&a| c.kind(a) == LoopGenerate).any(|a| {
            c.tree.children(a).next().is_some_and(|k| match c.kind(k) {
                Name => c.tree.bare_text(k) == name,
                BinaryExpr => kids(c, k).first().is_some_and(|&l| c.tree.bare_text(l) == name),
                _ => false,
            })
        });
        if genvar {
            return Some(vector(32, true, true).named("integer"));
        }
        self.o.enum_member(c.index(), e, name)
    }

    fn member_type(&mut self, e: NodeId) -> Option<ETy> {
        let c = self.c;
        // A struct, union or interface member: from the base's type.
        let base = kids(c, e).first().and_then(|&b| self.self_type(b));
        if let Some(b) = base.as_ref().filter(|b| b.aggregate || b.integral.is_some()) {
            return self.o.member(b, c.last_text(e));
        }
        if let Some(d) = crate::support::handles::expr_decl(c, e) {
            return self.decl_info(d);
        }
        self.o.member(base.as_ref()?, c.last_text(e))
    }

    fn select_type(&mut self, e: NodeId) -> Option<ETy> {
        let c = self.c;
        let ks = kids(c, e);
        let base = self.self_type(*ks.first()?)?;
        let own: Vec<&str> = c.toks(e).map(|t| c.text_of(t)).collect();
        // An unpacked array's element: its type without the outer dimension.
        if let Some(at) = base.text.find("$[") {
            let rest = &base.text[at + 1..];
            let close = rest.find(']')?;
            let inner_rest = &rest[close + 1..];
            let elem = if let Some(more) = inner_rest.strip_prefix('[') {
                format!("{}$[{more}", &base.text[..at])
            } else {
                base.text[..at].to_string()
            };
            if own.iter().any(|t| matches!(*t, ":" | "+:" | "-:")) {
                return None;
            }
            return Some(self.element_info(e, elem));
        }
        if base.text == "string" {
            return Some(vector(8, true, false).named("byte"));
        }
        let (bw, _, f) = base.integral?;
        let part = own.iter().position(|t| matches!(*t, ":" | "+:" | "-:"));
        match part {
            None => {
                // A packed array's element, else one bit.
                if let Some(t) = self.packed_element(e, &base) {
                    return Some(t);
                }
                let _ = bw;
                // One bit of a `reg` vector is a `reg`.
                if base.text.starts_with("reg") {
                    return Some(vector(1, false, f).named("reg"));
                }
                Some(vector(1, false, f))
            }
            Some(_) => {
                let w = if own.contains(&":") {
                    let (a, _, _) = self.o.eval(c.index(), *ks.get(1)?)?;
                    let (b, _, _) = self.o.eval(c.index(), *ks.get(2)?)?;
                    (a - b).unsigned_abs() as u32 + 1
                } else {
                    let (n, _, _) = self.o.eval(c.index(), *ks.get(2)?)?;
                    n.max(1) as u32
                };
                let ew = self.packed_element(e, &base).and_then(|t| t.integral).map_or(1, |x| x.0);
                if ew == 1 {
                    // A constant part-select keeps its bounds.
                    let lo_hi = if own.contains(&":") {
                        let a = self.o.eval(c.index(), ks[1]).map(|x| x.0);
                        let b = self.o.eval(c.index(), ks[2]).map(|x| x.0);
                        a.zip(b)
                    } else {
                        let base_ix = self.o.eval(c.index(), ks[1]).map(|x| x.0);
                        let up = own.contains(&"+:");
                        base_ix.map(|b| if up { (b + w as i128 - 1, b) } else { (b, b - w as i128 + 1) })
                    };
                    if let Some((a, b)) = lo_hi {
                        // slang keeps a `reg` vector's keyword (`reg[11:0]`).
                        let kw = if base.text.starts_with("reg") {
                            "reg"
                        } else if f {
                            "logic"
                        } else {
                            "bit"
                        };
                        return Some(ETy {
                            text: format!("{kw}[{a}:{b}]"),
                            integral: Some((w, false, f)),
                            src: None,
                            aggregate: false,
                        });
                    }
                }
                Some(vector(w.checked_mul(ew)?, false, f))
            }
        }
    }

    /// The element type of an unpacked array element: its integral shape
    /// from the declaration's data type.
    fn element_info(&mut self, e: NodeId, text: String) -> ETy {
        let c = self.c;
        let mut base = *kids(c, e).first().unwrap_or(&e);
        while c.kind(base) == SelectExpr {
            base = match kids(c, base).first() {
                Some(&b) => b,
                None => break,
            };
        }
        let d = match c.kind(base) {
            NameExpr => c.first_tok(base).and_then(|t| lookup(c, base, c.text_of(t))),
            MemberExpr => crate::support::handles::expr_decl(c, base),
            _ => None,
        };
        let integral = d.and_then(|d| d.dtype).and_then(|dt| self.o.data_type(c.index(), dt).integral);
        // Only the outermost unpacked dimension is selected away here.
        let integral = if text.contains("$[") { None } else { integral };
        ETy { text, integral, src: None, aggregate: false }
    }

    /// `x[i]` of a multi-dimensional packed array: the element's vector.
    fn packed_element(&mut self, e: NodeId, base: &ETy) -> Option<ETy> {
        let text = &base.text;
        // `logic[3:0][7:0]`: drop the first range.
        let open = text.find('[')?;
        let close = open + text[open..].find(']')?;
        if !text[close + 1..].starts_with('[') {
            return None;
        }
        let head = &text[..open];
        let rest = &text[close + 1..];
        let (bw, s, f) = base.integral?;
        let dim: Vec<i128> = text[open + 1..close].split(':').filter_map(|x| x.parse().ok()).collect();
        let n = if dim.len() == 2 { (dim[0] - dim[1]).unsigned_abs() as u32 + 1 } else { return None };
        let _ = (e, s);
        Some(ETy {
            text: format!("{head}{rest}"),
            integral: Some((bw / n.max(1), false, f)),
            src: None,
            aggregate: false,
        })
    }

    fn cast_type(&mut self, e: NodeId) -> Option<ETy> {
        let c = self.c;
        let ks = kids(c, e);
        let target = *ks.first()?;
        let inner = *ks.get(1)?;
        let x = self.self_type(inner);
        match c.kind(target) {
            Literal => {
                let (w, _, _) = self.o.eval(c.index(), target)?;
                let (_, s, f) = x?.integral?;
                Some(ranged(w as u32, s, f))
            }
            NameExpr if matches!(c.tree.bare_text(target).as_str(), "signed" | "unsigned") => {
                let (w, _, f) = x?.integral?;
                Some(ranged(w, c.tree.bare_text(target) == "signed", f))
            }
            NameExpr if c.tree.bare_text(target) == "const" => x,
            NameExpr | DataType | ScopedExpr => Some(self.o.data_type(c.index(), target)),
            _ => None,
        }
    }

    fn call_type(&mut self, e: NodeId) -> Option<ETy> {
        let c = self.c;
        let callee = *kids(c, e).first()?;
        let name = match c.kind(callee) {
            NameExpr => c.tree.bare_text(callee),
            MemberExpr | ScopedExpr => c.last_text(callee).to_string(),
            _ => return None,
        };
        let args = call_args(c, e);
        if name.starts_with('$') {
            return match name.as_str() {
                "$signed" | "$unsigned" => {
                    let (w, _, f) = self.self_type(*args.first()?)?.integral?;
                    let s = name == "$signed";
                    Some(vector(w, s, f))
                }
                "$clog2"
                | "$bits"
                | "$size"
                | "$high"
                | "$low"
                | "$left"
                | "$right"
                | "$increment"
                | "$dimensions"
                | "$unpacked_dimensions" => Some(vector(32, true, true).named("integer")),
                "$countones" | "$test$plusargs" | "$value$plusargs" => Some(vector(32, true, false).named("int")),
                "$random" => Some(vector(32, true, false).named("int")),
                "$urandom" | "$urandom_range" => Some(vector(32, false, false).named("int unsigned")),
                "$isunknown" | "$onehot" | "$onehot0" => Some(vector(1, false, false)),
                "$time" | "$stime" => Some(vector(64, false, true).named("time")),
                "$realtime" | "$itor" | "$bitstoreal" => Some(ETy::other("real")),
                _ => None,
            };
        }
        // Built-in methods of strings, arrays and enums.
        if c.kind(callee) == MemberExpr {
            if let Some(t) = kids(c, callee).first().and_then(|&b| self.self_type(b)) {
                let int = || Some(vector(32, true, false).named("int"));
                if t.text == "string" {
                    return match name.as_str() {
                        "len" | "compare" | "icompare" => int(),
                        "getc" => Some(vector(8, true, false).named("byte")),
                        "toupper" | "tolower" | "substr" => Some(ETy::other("string")),
                        "atoi" | "atohex" | "atooct" | "atobin" => Some(vector(32, true, true).named("integer")),
                        "atoreal" => Some(ETy::other("real")),
                        _ => Some(ETy::other("void")),
                    };
                }
                if t.text.contains("$[")
                    && matches!(name.as_str(), "size" | "num" | "exists" | "first" | "last" | "next" | "prev")
                {
                    return int();
                }
                if t.aggregate && t.text.contains("enum")
                    || (t.aggregate && t.integral.is_some() && !t.text.contains("struct"))
                {
                    match name.as_str() {
                        "num" => return int(),
                        "name" => return Some(ETy::other("string")),
                        "first" | "last" | "next" | "prev" => return Some(t),
                        _ => {}
                    }
                }
            }
        }
        // Array reductions: the element type (without a `with` clause).
        if c.kind(callee) == MemberExpr && matches!(name.as_str(), "sum" | "product" | "and" | "or" | "xor") {
            if c.tree.child(e, WithClause).is_some() {
                return None;
            }
            let base = self.self_type(*kids(c, callee).first()?)?;
            if let Some(at) = base.text.find("$[") {
                let rest = &base.text[at + 1..];
                let close = rest.find(']')?;
                let elem = match rest[close + 1..].strip_prefix('[') {
                    Some(more) => format!("{}$[{more}", &base.text[..at]),
                    None => base.text[..at].to_string(),
                };
                return Some(self.element_info(callee, elem));
            }
        }
        let decl = match crate::support::slang_ast::callee_decl(c, e) {
            Some(d) => d,
            None => self.method_via_type(e)?,
        };
        let rt = c.tree.child(decl, ReturnType).and_then(|r| c.tree.child(r, DataType));
        if c.toks(decl).any(|t| c.text_of(t) == "task") {
            return None;
        }
        match rt {
            Some(dt) if c.tree.bare_text(dt) == "void" => Some(ETy::other("void")),
            Some(dt) => {
                if let Some(t) = self.param_result(e, decl, dt) {
                    return Some(t);
                }
                Some(self.o.data_type(c.index(), dt))
            }
            None => Some(vector(1, false, true)),
        }
    }
}

impl<'a, 't> Typer<'a, 't> {
    /// The method a call on a value names, found through the value's
    /// declared type (a typedef of a class or of a specialization).
    fn method_via_type(&self, call: NodeId) -> Option<NodeId> {
        let c = self.c;
        let (_, name, recv) = crate::support::slang_ast::call_parts(c, call)?;
        let rd = crate::support::handles::expr_decl(c, recv?)?;
        let (tree, cls) = self.o.class_decl(c.index(), rd.dtype?)?;
        if tree != c.index() {
            return None;
        }
        crate::support::slang_ast::class_chain(c, cls).into_iter().find_map(|k| {
            kids(c, k).into_iter().find(|&m| {
                matches!(c.kind(m), FunctionDecl | TaskDecl | MethodPrototype)
                    && crate::support::slang_ast::last_name(c, m) == name
            })
        })
    }

    /// `q.get(i)` with `T get(...)` in `class queue #(type T)`: what the
    /// receiver's declared specialization binds `T` to.
    fn param_result(&self, call: NodeId, decl: NodeId, ret: NodeId) -> Option<ETy> {
        let c = self.c;
        if c.toks(ret).count() != 1 || c.tree.children(ret).next().is_some() {
            return None;
        }
        let (_, _, recv) = crate::support::slang_ast::call_parts(c, call)?;
        let rd = crate::support::handles::expr_decl(c, recv?)?;
        let cls = crate::support::slang_ast::ancestors(c, decl).find(|&a| c.kind(a) == ClassDecl)?;
        self.o.method_result(c.index(), rd.dtype?, (c.index(), cls), ret)
    }
}

trait Named {
    fn named(self, n: &str) -> Self;
}

impl Named for ETy {
    fn named(mut self, n: &str) -> Self {
        self.text = n.to_string();
        self
    }
}

fn four(t: &ETy) -> bool {
    t.integral.is_some_and(|x| x.2)
}

/// An integer, real or string literal's type.
/// The same type, an enum member's type printed whole or by its name.
fn same_type(a: &ETy, b: &ETy) -> bool {
    let name = |t: &ETy| t.text.rsplit_once('}').map_or(t.text.clone(), |(_, n)| n.to_string());
    a.integral == b.integral && name(a) == name(b)
}

/// `'0`, `'1`, `'x`, `'z`.
fn unbased(c: &Cst, e: NodeId) -> bool {
    c.first_tok(e).is_some_and(|t| matches!(c.text_of(t), "'0" | "'1" | "'x" | "'z" | "'X" | "'Z"))
}

fn literal(text: &str) -> Option<ETy> {
    if let Some(body) = text.strip_prefix('"') {
        let body = body.strip_suffix('"').unwrap_or(body);
        let mut n = 0u32;
        let mut it = body.chars();
        while let Some(ch) = it.next() {
            if ch == '\\' {
                match it.next() {
                    Some('x') => {
                        for _ in 0..2 {
                            if it.clone().next().is_some_and(|c| c.is_ascii_hexdigit()) {
                                it.next();
                            }
                        }
                    }
                    Some('0'..='7') => {
                        for _ in 0..2 {
                            if it.clone().next().is_some_and(|c| ('0'..='7').contains(&c)) {
                                it.next();
                            }
                        }
                    }
                    // A line continuation adds nothing.
                    Some('\n') => continue,
                    _ => {}
                }
            }
            n += 1;
        }
        return Some(ranged(8 * n.max(1), false, false));
    }
    let t = text.replace('_', "");
    if let Some(rest) = t.strip_prefix('\'') {
        if rest.len() == 1 && matches!(rest, "0" | "1" | "x" | "X" | "z" | "Z") {
            // Four states only for x and z.
            return Some(vector(1, false, !matches!(rest, "0" | "1")));
        }
    }
    if let Some(q) = t.find('\'') {
        let size: u32 = if q == 0 { 32 } else { t[..q].trim().parse().ok()? };
        let rest = t[q + 1..].to_ascii_lowercase();
        let signed = rest.starts_with('s');
        let digits = rest.trim_start_matches('s');
        let digits = &digits[1.min(digits.len())..];
        let f = digits.contains(['x', 'z', '?']);
        return Some(ranged(size, signed, f));
    }
    if t.contains('.') || (t.contains(['e', 'E']) && t.parse::<f64>().is_ok()) {
        return Some(ETy::other("real"));
    }
    if t.chars().all(|ch| ch.is_ascii_digit()) {
        return Some(vector(32, true, false).named("int"));
    }
    if t.ends_with('s') {
        return Some(ETy::other("realtime"));
    }
    None
}

/// A call's argument expressions, in order.
pub(crate) fn call_args(c: &Cst, e: NodeId) -> Vec<NodeId> {
    c.tree
        .child(e, ArgList)
        .map(|al| kids(c, al).into_iter().flat_map(|a| if c.kind(a) == Arg { kids(c, a) } else { vec![a] }).collect())
        .unwrap_or_default()
}

/// The type of every expression of the compile's trees, by (tree, node):
/// what rules read as `Expression::type` after stripping conversions.
pub(crate) fn expr_types(c: &Cst) -> Arc<HashMap<(usize, NodeId), ETy>> {
    facts(c).0
}

/// Expression types per instance, where instances of a module differ.
pub(crate) fn inst_types(c: &Cst) -> Arc<InstTypes> {
    facts(c).5
}

/// The routine formals (their declarators) of one bit.
pub(crate) fn one_bit_formals(c: &Cst) -> Arc<svlang::semantic::fx::FxSet<(usize, NodeId)>> {
    facts(c).2
}

/// The matching-type form (`TypeOracle::canonical`) of the type of each
/// cast and `$cast` argument.
pub(crate) fn cast_canon(c: &Cst) -> Arc<HashMap<(usize, NodeId), String>> {
    facts(c).1
}

/// Whether the routine body around `n` fails to bind in this
/// specialization: it compares a value that is integral here with `null`,
/// assigns `null` to one, or selects a member of one (`x.put(t)` with
/// `type IMP = int`). slang then marks the whole body bad, and the AST
/// rules do not see it (see [`crate::support::default_spec`]).
fn null_compare_fails(c: &Cst, ty: &mut Typer, n: NodeId) -> bool {
    let Some(r) = crate::support::slang_ast::ancestors(c, n)
        .take_while(|&a| c.kind(a) != ClassDecl)
        .find(|&a| matches!(c.kind(a), FunctionDecl | TaskDecl))
    else {
        return false;
    };
    let Some(body) = c.tree.child(r, Body) else { return false };
    let null = |x: NodeId| matches!(c.kind(x), Literal | NameExpr) && c.tree.bare_text(x) == "null";
    for e in crate::support::slang_ast::subtree(c, body) {
        match c.kind(e) {
            BinaryExpr if c.keyword(e).is_some_and(|k| matches!(c.text_of(k), "==" | "!=" | "===" | "!==" | "=")) => {
                let ks = kids(c, e);
                let [l, r] = ks[..] else { continue };
                let other = if null(r) {
                    l
                } else if null(l) {
                    r
                } else {
                    continue;
                };
                if ty.self_type(other).is_some_and(|t| t.integral.is_some()) {
                    return true;
                }
            }
            MemberExpr => {
                let Some(&b) = kids(c, e).first() else { continue };
                if c.kind(b) == NameExpr && ty.self_type(b).is_some_and(|t| t.integral.is_some()) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// [`CfgDbTypes`] of the compile.
pub(crate) fn cfgdb_types(c: &Cst) -> Arc<CfgDbTypes> {
    facts(c).4
}

/// [`SpecTypes`] of the compile.
pub(crate) fn spec_types(c: &Cst) -> Arc<SpecTypes> {
    facts(c).3
}

type Facts = (
    Arc<HashMap<(usize, NodeId), ETy>>,
    Arc<HashMap<(usize, NodeId), String>>,
    Arc<svlang::semantic::fx::FxSet<(usize, NodeId)>>,
    Arc<SpecTypes>,
    Arc<CfgDbTypes>,
    Arc<InstTypes>,
);

/// Module bodies typed per instance (see [`type_instances`]): the
/// expressions whose types differ from the defaults, with their type (and
/// matching-type form, for casts) in each group of instances that type
/// alike; and for each definition with such expressions, each group's
/// copies of its body and generate items.
/// A type and, for a cast or its operand, its matching-type form.
pub(crate) type Typed = Option<(ETy, Option<String>)>;

/// Instances of a definition that type alike: (representative, members).
type InstGroups = Vec<(usize, Vec<usize>)>;

#[derive(Default)]
pub(crate) struct InstTypes {
    pub types: HashMap<(usize, NodeId), Vec<Typed>>,
    pub defs: HashMap<(usize, NodeId), Vec<HashMap<NodeId, usize>>>,
}

/// For each `uvm_config_db#(T)::set/get` call: the value argument's own
/// type and matching form, and the matching form of `T`.
/// The value's type, its canonical text, and whether Slang converts it to
/// `T`: one entry, or (flagged true) one per specialization of the generic
/// class around.
pub(crate) type CfgDbTypes = HashMap<(usize, NodeId), (bool, Vec<(ETy, String, bool)>)>;

/// Per specialization of the generic class around them: the type and its
/// matching form of each cast and `$cast` argument and each `new` there.
pub(crate) type SpecTypes = HashMap<(usize, NodeId), Vec<Option<(ETy, String)>>>;

/// A `uvm_config_db#(T)::set/get` call `n`: None when it is not one, else
/// its value's type, canonical text and whether slang converts it to `T`
/// (None when the value does not type).
fn cfgdb_entry(
    tc: &Cst,
    ty: &mut Typer,
    o: &TypeOracle,
    n: NodeId,
    ifaces: &mut Option<svlang::semantic::fx::FxSet<String>>,
) -> Option<Option<(ETy, String, bool)>> {
    let i = tc.index();
    let &callee = kids(tc, n).first()?;
    if tc.kind(callee) != ScopedExpr {
        return None;
    }
    let &prefix = kids(tc, callee).first()?;
    let (cname, targ) = o.class_arg(i, prefix, "T")?;
    if !cname.contains("uvm_config_db") {
        return None;
    }
    let args = call_args(tc, n);
    let Some(&v) = args.get(3) else { return Some(None) };
    let vt = match ty.self_type(v) {
        Some(x) => x,
        // An interface instance handed to a virtual interface `T`: its
        // type prints as the interface's specialization.
        None if tc.kind(v) == NameExpr => {
            let Some(t) = targ.as_ref() else { return Some(None) };
            let ct = o.canonical(t);
            let base = ct.split('#').next().unwrap_or("");
            if !ifaces.get_or_insert_with(|| interface_names(tc.all)).contains(base) {
                return Some(None);
            }
            svlang::semantic::oracle::TypeInfo::other(ct)
        }
        None => return Some(None),
    };
    let vc = o.canonical(&vt);
    // Slang converts the actual unless its type is equivalent to T.
    let converted = targ.is_some_and(|t| match (t.integral, vt.integral) {
        (Some(a), Some(b)) => a != b,
        _ => o.canonical(&t) != vc,
    });
    Some(Some((vt, vc, converted)))
}

/// The names of the interfaces declared in the compile.
fn interface_names(all: &[svlang::syntax::Tree]) -> svlang::semantic::fx::FxSet<String> {
    let mut out = svlang::semantic::fx::FxSet::default();
    for (i, t) in all.iter().enumerate() {
        let c = Cst::new(t, i, svlang::semantic::Locator::default());
        for n in c.nodes(InterfaceDecl) {
            out.insert(crate::support::slang_ast::decl_name(&c, n));
        }
    }
    out
}

fn facts(c: &Cst) -> Facts {
    (*crate::shared::get("facts", c.compile_key(), || build_facts(c))).clone()
}

/// Expressions in a tree from which [`build_facts`] types it on threads.
const TYPE_PART: usize = 20_000;

fn build_facts(c: &Cst) -> Facts {
    let mut o = TypeOracle::new(c.all);
    o.elaborate(&crate::tops());
    let o = o;
    let mut out = HashMap::default();
    let mut canon = HashMap::default();
    let mut one_bit = svlang::semantic::fx::FxSet::default();
    let mut in_generic: HashMap<(usize, usize, usize), Vec<NodeId>> = HashMap::default();
    let mut cfgdb: CfgDbTypes = HashMap::default();
    let mut ifaces: Option<svlang::semantic::fx::FxSet<String>> = None;
    let mut cfg_generic: HashMap<(usize, usize, usize), Vec<NodeId>> = HashMap::default();
    let mut scoped_calls: Vec<(usize, NodeId)> = Vec::new();
    for (i, t) in c.all.iter().enumerate() {
        let mut own;
        let tc = if std::ptr::eq(t, c.tree) {
            c
        } else {
            own = Cst::new(t, i, svlang::semantic::Locator::default());
            own.all = c.all;
            &own
        };
        let exprs: Vec<NodeId> = (0..t.nodes.len() as u32).map(NodeId).filter(|&n| is_expr(tc.kind(n))).collect();
        // A large tree is typed in parts on threads of their own, each with
        // its own oracle (answers do not depend on what was asked before).
        let parts = if exprs.len() >= TYPE_PART { 3 } else { 1 };
        let chunk = exprs.len().div_ceil(parts).max(1);
        let (all, gen) = (c.all, crate::shared::current());
        std::thread::scope(|s| {
            let rest: Vec<_> = exprs
                .chunks(chunk)
                .skip(1)
                .map(|ch| {
                    std::thread::Builder::new()
                        .stack_size(256 << 20)
                        .spawn_scoped(s, move || {
                            crate::shared::enter(gen);
                            let o = TypeOracle::new(all);
                            let mut tc = Cst::new(t, i, svlang::semantic::Locator::default());
                            tc.all = all;
                            let mut ty = Typer::new(&tc, &o);
                            ch.iter().filter_map(|&n| ty.final_type(n).map(|x| (n, x))).collect::<Vec<_>>()
                        })
                        .expect("spawn a typing thread")
                })
                .collect();
            let mut ty = Typer::new(tc, &o);
            for &n in exprs.chunks(chunk).next().unwrap_or(&[]) {
                if let Some(x) = ty.final_type(n) {
                    out.insert((i, n), x);
                }
            }
            for h in rest {
                for (n, x) in h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)) {
                    out.insert((i, n), x);
                }
            }
        });
        // `X::set/get(...)` calls, typed as config_db calls at the end.
        for n in 0..t.nodes.len() {
            let n = NodeId(n as u32);
            if tc.kind(n) == CallExpr && kids(tc, n).first().is_some_and(|&f| tc.kind(f) == ScopedExpr) {
                scoped_calls.push((i, n));
            }
        }
        let mut want = Vec::new();
        for n in 0..t.nodes.len() {
            let n = NodeId(n as u32);
            match tc.kind(n) {
                CastExpr => {
                    want.push(n);
                    let mut x = kids(tc, n).get(1).copied();
                    while let Some(p) = x.filter(|&p| tc.kind(p) == ParenExpr) {
                        want.push(p);
                        x = kids(tc, p).first().copied();
                    }
                    want.extend(x);
                }
                CallExpr if kids(tc, n).first().is_some_and(|&f| tc.tree.bare_text(f) == "$cast") => {
                    want.extend(call_args(tc, n).into_iter().take(2));
                }
                // `x = new(...)`: the class it makes, per specialization
                // (UVM-FAC-001 prints it).
                NewExpr => {
                    if let Some(p) = tc.parent(n).filter(|&p| tc.kind(p) == BinaryExpr && tc.tok(p, "=").is_some()) {
                        want.extend(kids(tc, p).first().copied());
                    }
                }
                _ => {}
            }
        }
        for n in want {
            if let Some(x) = out.get(&(i, n)) {
                canon.insert((i, n), o.canonical(x));
            }
            if let Some((g, k)) = o.specs_at(i, n) {
                in_generic.entry((i, g, k)).or_default().push(n);
            }
        }
        // Formals: a port without a type takes the previous one's.
        for n in 0..t.nodes.len() {
            let pl = NodeId(n as u32);
            if tc.kind(pl) != PortList
                || !tc.parent(pl).is_some_and(|f| matches!(tc.kind(f), FunctionDecl | TaskDecl | MethodPrototype))
            {
                continue;
            }
            let mut dt: Option<NodeId> = None;
            for p in tc.tree.children(pl).filter(|&p| tc.kind(p) == Port) {
                if let Some(d) = tc.tree.child(p, DataType) {
                    dt = Some(d);
                } else if tc.toks(p).next().is_some() {
                    dt = None;
                }
                for d in tc.tree.children(p).filter(|&d| tc.kind(d) == Declarator) {
                    if o.decl_type(i, dt, Some(d)).integral.is_some_and(|(w, ..)| w == 1) {
                        one_bit.insert((i, d));
                    }
                }
            }
        }
    }
    let inst = type_instances(c, &o, &mut out, &mut canon);
    let mut specs: SpecTypes = HashMap::default();
    let mut groups: Vec<_> = in_generic.into_iter().collect();
    groups.sort_by_key(|g| g.0);
    for ((i, g, k), nodes) in groups {
        let t = &c.all[i];
        let mut own;
        let tc = if std::ptr::eq(t, c.tree) {
            c
        } else {
            own = Cst::new(t, i, svlang::semantic::Locator::default());
            own.all = c.all;
            &own
        };
        for spec in 0..k {
            o.set_spec(Some((g, spec)));
            let mut ty = Typer::new(tc, &o);
            for &n in &nodes {
                let x = if null_compare_fails(tc, &mut ty, n) {
                    None
                } else {
                    ty.final_type(n).map(|x| {
                        let m = o.canonical(&x);
                        (x, m)
                    })
                };
                specs.entry((i, n)).or_default().push(x);
            }
        }
    }
    // After all other typing: resolving the class names can create
    // specializations, which would change how earlier answers print.
    o.set_spec(None);
    let mut by_tree: Vec<(usize, Vec<NodeId>)> = Vec::new();
    for &(i, n) in &scoped_calls {
        match by_tree.last_mut() {
            Some(x) if x.0 == i => x.1.push(n),
            _ => by_tree.push((i, vec![n])),
        }
    }
    for (i, nodes) in by_tree {
        let t = &c.all[i];
        let mut own;
        let tc = if std::ptr::eq(t, c.tree) {
            c
        } else {
            own = Cst::new(t, i, svlang::semantic::Locator::default());
            own.all = c.all;
            &own
        };
        let mut ty = Typer::new(tc, &o);
        for n in nodes {
            match o.specs_at(i, n) {
                // Typed per specialization below.
                Some((g, k)) if cfgdb_entry(tc, &mut ty, &o, n, &mut ifaces).is_some() => {
                    cfg_generic.entry((i, g, k)).or_default().push(n);
                }
                _ => {
                    if let Some(x) = cfgdb_entry(tc, &mut ty, &o, n, &mut ifaces) {
                        cfgdb.insert((i, n), (false, x.into_iter().collect()));
                    }
                }
            }
        }
    }
    let mut groups: Vec<_> = cfg_generic.into_iter().collect();
    groups.sort_by_key(|g| g.0);
    for ((i, g, k), nodes) in groups {
        let t = &c.all[i];
        let mut own;
        let tc = if std::ptr::eq(t, c.tree) {
            c
        } else {
            own = Cst::new(t, i, svlang::semantic::Locator::default());
            own.all = c.all;
            &own
        };
        for spec in 0..k {
            o.set_spec(Some((g, spec)));
            let mut ty = Typer::new(tc, &o);
            for &n in &nodes {
                let e = cfgdb.entry((i, n)).or_insert((true, Vec::new()));
                if let Some(Some(x)) = cfgdb_entry(tc, &mut ty, &o, n, &mut ifaces) {
                    e.1.push(x);
                }
            }
        }
    }
    o.set_spec(None);
    (Arc::new(out), Arc::new(canon), Arc::new(one_bit), Arc::new(specs), Arc::new(cfgdb), Arc::new(inst))
}

/// The types of a module body's expressions per instance, where they
/// differ from those its parameters' defaults give (see [`InstTypes`]).
/// Where every instance sees one type, `out` (the default answers) and
/// `canon` take it instead.
fn type_instances<'t>(
    c: &Cst<'t>,
    o: &TypeOracle<'t>,
    out: &mut HashMap<(usize, NodeId), ETy>,
    canon: &mut HashMap<(usize, NodeId), String>,
) -> InstTypes {
    let mut res = InstTypes::default();
    // Instances that type alike, by definition: (representative, members).
    let mut defs: HashMap<(usize, NodeId), InstGroups> = HashMap::default();
    let mut by_key: HashMap<u64, usize> = HashMap::default();
    for i in 0..o.instance_count() {
        let groups = defs.entry(o.instance_decl(i)).or_default();
        match by_key.get(&o.instance_key(i)) {
            Some(&g) => groups[g].1.push(i),
            None => {
                by_key.insert(o.instance_key(i), groups.len());
                groups.push((i, vec![i]));
            }
        }
    }
    let mut defs: Vec<_> = defs.into_iter().collect();
    defs.sort_by_key(|((t, n), _)| (*t, n.0));
    for ((ti, decl), groups) in defs {
        let t = &c.all[ti];
        let mut own;
        let tc = if std::ptr::eq(t, c.tree) {
            c
        } else {
            own = Cst::new(t, ti, svlang::semantic::Locator::default());
            own.all = c.all;
            &own
        };
        let exprs: Vec<NodeId> = t.subtree(decl).into_iter().filter(|&n| is_expr(tc.kind(n))).collect();
        let casts: svlang::semantic::fx::FxSet<NodeId> = cast_nodes(tc).into_iter().collect();
        let mut per: Vec<Vec<Typed>> = Vec::new();
        let mut copies = Vec::new();
        for (rep, members) in &groups {
            o.set_instance(Some(*rep));
            let mut ty = Typer::new(tc, o);
            per.push(
                exprs
                    .iter()
                    .map(|&n| {
                        let x = ty.final_type(n)?;
                        let m = casts.contains(&n).then(|| o.canonical(&x));
                        Some((x, m))
                    })
                    .collect(),
            );
            // Copies of each body and generate item, over the members.
            let mut k: HashMap<NodeId, usize> = HashMap::default();
            for &m in members {
                for n in t.subtree(decl) {
                    let x = o.instance_copies(m, ti, n);
                    if x > 0 {
                        *k.entry(n).or_default() += x;
                    }
                }
            }
            copies.push(k);
        }
        let mut differ = false;
        for (k, &n) in exprs.iter().enumerate() {
            let dflt = out.get(&(ti, n));
            if per.iter().all(|ts| ts[k].as_ref().map(|x| &x.0) == dflt) {
                continue;
            }
            differ = true;
            let first = &per[0][k];
            if per.iter().all(|ts| ts[k] == *first) {
                match first {
                    Some((x, m)) => {
                        out.insert((ti, n), x.clone());
                        if let Some(m) = m {
                            canon.insert((ti, n), m.clone());
                        }
                    }
                    None => {
                        out.remove(&(ti, n));
                    }
                }
            }
            res.types.insert((ti, n), per.iter().map(|ts| ts[k].clone()).collect());
        }
        if differ {
            res.defs.insert((ti, decl), copies);
        }
    }
    o.set_instance(None);
    res
}

/// The casts, their operands and `$cast` arguments of a tree: the
/// expressions whose matching-type form rules compare.
fn cast_nodes(tc: &Cst) -> Vec<NodeId> {
    let mut want = Vec::new();
    for n in 0..tc.tree.nodes.len() {
        let n = NodeId(n as u32);
        match tc.kind(n) {
            CastExpr => {
                want.push(n);
                let mut x = kids(tc, n).get(1).copied();
                while let Some(p) = x.filter(|&p| tc.kind(p) == ParenExpr) {
                    want.push(p);
                    x = kids(tc, p).first().copied();
                }
                want.extend(x);
            }
            CallExpr if kids(tc, n).first().is_some_and(|&f| tc.tree.bare_text(f) == "$cast") => {
                want.extend(call_args(tc, n).into_iter().take(2));
            }
            _ => {}
        }
    }
    want
}

/// The visits slang's AST makes of `e` (as [`ast_visits`] counts them),
/// split by the types `nodes` have in them: (visits, each node's type and
/// matching-type form). One part with the default types outside module
/// bodies, or where every instance types `nodes` alike.
///
/// [`ast_visits`]: crate::support::style_config::ast_visits
pub(crate) fn visit_types(c: &Cst, e: NodeId, nodes: &[NodeId]) -> Vec<(usize, Vec<Typed>)> {
    let ix = c.index();
    let total = crate::support::style_config::ast_visits(c, e);
    let types = expr_types(c);
    let canon = cast_canon(c);
    let dflt = |n: NodeId| types.get(&(ix, n)).map(|t| (t.clone(), canon.get(&(ix, n)).cloned()));
    let whole = || vec![(total, nodes.iter().map(|&n| dflt(n)).collect())];
    let it = inst_types(c);
    if !nodes.iter().any(|&n| it.types.contains_key(&(ix, n))) {
        return whole();
    }
    let Some(groups) = crate::support::slang_ast::ancestors(c, e)
        .find(|&a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl))
        .and_then(|d| it.defs.get(&(ix, d)))
    else {
        return whole();
    };
    let Some(anchor) = std::iter::once(e)
        .chain(crate::support::slang_ast::ancestors(c, e))
        .find(|a| groups.iter().any(|g| g.contains_key(a)))
    else {
        return whole();
    };
    let mut parts: Vec<(usize, Vec<Typed>)> = Vec::new();
    for (g, copies) in groups.iter().enumerate() {
        let k = copies.get(&anchor).copied().unwrap_or(0);
        if k == 0 {
            continue;
        }
        let ts: Vec<_> =
            nodes.iter().map(|&n| it.types.get(&(ix, n)).map_or_else(|| dflt(n), |v| v[g].clone())).collect();
        match parts.iter_mut().find(|p| p.1 == ts) {
            Some(p) => p.0 += k,
            None => parts.push((k, ts)),
        }
    }
    // Visits no elaborated instance accounts for (a `bind`): as before.
    let sum: usize = parts.iter().map(|p| p.0).sum();
    if sum != total {
        return whole();
    }
    parts
}

pub(crate) fn is_expr(k: svlang::syntax::SyntaxKind) -> bool {
    matches!(
        k,
        BinaryExpr
            | PrefixExpr
            | PostfixExpr
            | CondExpr
            | InsideExpr
            | ParenExpr
            | Literal
            | NameExpr
            | ScopedExpr
            | MemberExpr
            | SelectExpr
            | CallExpr
            | ConcatExpr
            | CastExpr
    )
}

/// slang's `ExpressionKind` name for an expression node; `None` for nodes
/// slang does not bind as an expression of their own.
pub(crate) fn slang_kind(c: &Cst, e: NodeId) -> Option<&'static str> {
    let ks = kids(c, e);
    if let Some(p) = c.parent(e) {
        // A call's callee is not an expression.
        if c.kind(p) == CallExpr && kids(c, p).first() == Some(&e) {
            return None;
        }
    }
    // Not walked by slang's AST visitor: declared dimensions and parameter
    // values, a cast's width, the base of a select or member access.
    let mut a = c.parent(e);
    while let Some(p) = a {
        if matches!(c.kind(p), Dim | DataType) {
            return None;
        }
        a = c.parent(p);
    }
    if let Some(p) = c.parent(e) {
        let first = kids(c, p).first() == Some(&e);
        if first
            && matches!(c.kind(p), SelectExpr | MemberExpr | CastExpr)
            && std::env::var_os("DV_EXPRS_ALL").is_none()
        {
            return None;
        }
    }
    Some(match c.kind(e) {
        ParenExpr => return None,
        Literal => {
            let t = c.text_of(c.first_tok(e)?);
            if t.starts_with('"') {
                "StringLiteral"
            } else if t == "null" {
                "NullLiteral"
            } else if t == "$" {
                "UnboundedLiteral"
            } else if matches!(t, "'0" | "'1" | "'x" | "'z" | "'X" | "'Z") {
                "UnbasedUnsizedIntegerLiteral"
            } else if t.contains('\'') || t.chars().all(|ch| ch.is_ascii_digit() || ch == '_') {
                "IntegerLiteral"
            } else if t.ends_with('s') {
                "TimeLiteral"
            } else {
                "RealLiteral"
            }
        }
        NameExpr | ScopedExpr => "NamedValue",
        MemberExpr => "MemberAccess",
        BinaryExpr => {
            if is_assign(c, e) {
                "Assignment"
            } else {
                "BinaryOp"
            }
        }
        PrefixExpr | PostfixExpr => "UnaryOp",
        CondExpr => "ConditionalOp",
        InsideExpr => "Inside",
        ConcatExpr => {
            let commas = c.toks(e).filter(|&t| c.text_of(t) == ",").count();
            if commas == 0 && ks.len() == 2 && c.kind(ks[1]) == ConcatExpr {
                "Replication"
            } else {
                "Concatenation"
            }
        }
        SelectExpr => {
            if ks.len() >= 3 {
                "RangeSelect"
            } else {
                "ElementSelect"
            }
        }
        CallExpr => "Call",
        CastExpr => "Conversion",
        _ => return None,
    })
}

/// One line per typed expression, as tools/oracles/exprdump.cpp prints slang's:
/// `<file>:<line>:<col> <kind> <bits> <s|u> <4|2> <type>`, sorted.
pub fn expr_dump(files: Vec<String>, trees: &[svlang::syntax::Tree]) -> Vec<String> {
    let o = TypeOracle::new(trees);
    let mut locator = svlang::semantic::Locator::new(files);
    let mut rows = Vec::new();
    for (i, t) in trees.iter().enumerate() {
        let mut cst = Cst::new(t, i, locator);
        cst.all = trees;
        {
            let mut ty = Typer::new(&cst, &o);
            for n in 0..t.nodes.len() {
                let n = NodeId(n as u32);
                if !is_expr(cst.kind(n)) {
                    continue;
                }
                let Some(kind) = slang_kind(&cst, n) else { continue };
                let Some(tok) = cst.first_tok(n) else { continue };
                let l = cst.loc(tok);
                if l.expansion.is_some() {
                    continue;
                }
                let Some(x) = ty.final_type(n) else {
                    if std::env::var_os("DV_EXPR_WHY").is_some() {
                        let chain: Vec<String> = crate::support::slang_ast::ancestors(&cst, n)
                            .map(|a| format!("{:?}", cst.kind(a)))
                            .collect();
                        eprintln!("{}:{} {} untyped: {}", l.line, l.column, cst.tree.bare_text(n), chain.join(" < "));
                    }
                    continue;
                };
                let (w, s, f) = x.integral.unwrap_or((0, false, false));
                rows.push((l.file, l.line, l.column, kind, w, s, f, x.text));
            }
        }
        locator = cst.into_locator();
    }
    let files = locator.into_files();
    let mut out: Vec<String> = rows
        .into_iter()
        .map(|(f, line, col, kind, w, s, four, text)| {
            let base = files.get(f as usize).map(|p| p.rsplit('/').next().unwrap_or("")).unwrap_or("");
            format!(
                "{base}:{line}:{col} {kind} {w} {} {} {text}",
                if s { "s" } else { "u" },
                if four { "4" } else { "2" }
            )
        })
        .collect();
    out.sort();
    out.dedup();
    out
}
