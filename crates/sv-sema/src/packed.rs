//! Packed aggregates in constant evaluation (builder phase N6): packed
//! structs and unions, packed arrays, their assignment patterns
//! (`'{XLEN: 64, default: 0}`), member and element selects
//! (`Cfg.PMARegions[2]`), and constant functions that build them field by
//! field (cva6's `build_config`). A packed value is one integer; a member
//! or element is a range of its bits, so all of this is bit arithmetic on
//! [`CVal`] once the type says where the bits are.

use super::types::{CVal, Int};
use super::*;

/// A type as written: a DataType node in a scope, with its first `drop`
/// packed dimensions already selected away (`x[i]` of `logic [3:0][7:0] x`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ty {
    pub dt: NodeRef,
    pub scope: usize,
    pub drop: u32,
}

impl Ty {
    pub fn of(dt: NodeRef, scope: usize) -> Ty {
        Ty { dt, scope, drop: 0 }
    }
}

pub(crate) struct Field {
    pub name: String,
    /// The lowest bit, from the struct's bit 0.
    pub lo: u32,
    pub w: u32,
    pub ty: Ty,
}

pub(crate) enum Shape {
    /// The next packed dimension: its bounds and its element.
    Array {
        left: i128,
        right: i128,
        elem: Ty,
        elem_w: u32,
    },
    Struct {
        fields: Vec<Field>,
        w: u32,
        signed: bool,
    },
    Scalar {
        w: u32,
        s: bool,
    },
}

const MAX_DEPTH: usize = 24;

impl Builder<'_> {
    fn packed_dims(&self, dt: NodeRef) -> Vec<NodeRef> {
        if self.kind(dt) != DataType {
            return Vec::new();
        }
        self.children(dt).into_iter().filter(|d| self.kind(*d) == Dim).collect()
    }

    /// What a type is, one level down.
    pub(crate) fn shape(&self, ty: Ty, env: &Env, depth: usize) -> Option<Shape> {
        if depth > MAX_DEPTH {
            return None;
        }
        let dims = self.packed_dims(ty.dt);
        if (ty.drop as usize) < dims.len() {
            let (left, right) = self.dim_range(dims[ty.drop as usize], ty.scope, env)?;
            let elem = Ty { drop: ty.drop + 1, ..ty };
            let elem_w = self.width(elem, env, depth + 1)?;
            return Some(Shape::Array { left, right, elem, elem_w });
        }
        let own = self.own_tokens(ty.dt);
        if matches!(own.first(), Some(&"struct" | &"union")) && own.contains(&"packed") {
            let union = own.first() == Some(&"union");
            let mut members: Vec<(String, Ty, u32)> = Vec::new();
            for m in self.children(ty.dt) {
                if self.kind(m) != DataDecl {
                    continue;
                }
                let fdt = self.child(m, DataType)?;
                let fty = Ty::of(fdt, ty.scope);
                let w = self.width(fty, env, depth + 1)?;
                for d in self.children(m) {
                    if self.kind(d) == Declarator {
                        members.push((self.name_of(d)?.0, fty, w));
                    }
                }
            }
            let total: u32 = if union {
                members.iter().map(|m| m.2).max().unwrap_or(1)
            } else {
                members.iter().try_fold(0u32, |a, m| a.checked_add(m.2))?
            };
            let mut top = total;
            let fields = members
                .into_iter()
                .map(|(name, ty, w)| {
                    let lo = if union {
                        0
                    } else {
                        top -= w;
                        top
                    };
                    Field { name, lo, w, ty }
                })
                .collect();
            return Some(Shape::Struct { fields, w: total.max(1), signed: own.contains(&"signed") });
        }
        // A typedef or type parameter: what it names (with no dims of our own left).
        if ty.drop as usize >= dims.len() && !dims.is_empty() {
            // `logic [7:0]` with its dims all selected: one bit; `t_t [3:0]`:
            // a t_t.
            let own = self.own_tokens(ty.dt);
            if own.iter().any(|t| matches!(*t, "bit" | "logic" | "reg"))
                || own.iter().all(|t| matches!(*t, "signed" | "unsigned"))
            {
                return Some(Shape::Scalar { w: 1, s: false });
            }
            let inner = self.named_type(ty.dt, ty.scope, env)?;
            return self.shape(inner, env, depth + 1);
        }
        if let Some(inner) = self.named_type(ty.dt, ty.scope, env) {
            return self.shape(inner, env, depth + 1);
        }
        let (w, s) = self.int_shape(ty.dt, ty.scope, env, depth + 1)?;
        Some(Shape::Scalar { w, s })
    }

    /// The type a type name stands for: a typedef's target or a type
    /// parameter's binding.
    fn named_type(&self, dt: NodeRef, scope: usize, env: &Env) -> Option<Ty> {
        let toks = self.toks_of(dt);
        let name: Vec<&str> = toks.iter().take_while(|t| **t != "[").copied().collect();
        if let [n] = name.as_slice() {
            if let Some(bd) = env.get(*n) {
                return Some(Ty::of(bd.node, bd.scope));
            }
            let mut cur = Some(scope);
            while let Some(c) = cur {
                if let Some(d) = self.scopes[c].type_params.get(*n) {
                    return Some(Ty::of((*d)?, c));
                }
                cur = self.scopes[c].parent;
            }
        }
        if name.len() == 1 || name.contains(&"::") {
            if let Some(Found::Typedef(t, s)) = self.lookup_type_pub(dt, scope) {
                if self.child(t, Dim).is_some() {
                    return None;
                }
                return Some(Ty::of(self.child(t, DataType)?, s));
            }
        }
        None
    }

    pub(crate) fn width(&self, ty: Ty, env: &Env, depth: usize) -> Option<u32> {
        if ty.drop == 0 && depth == 0 {
            if let Some((w, _)) = self.int_shape(ty.dt, ty.scope, env, 0) {
                return Some(w);
            }
        }
        Some(match self.shape(ty, env, depth + 1)? {
            Shape::Array { left, right, elem_w, .. } => {
                elem_w.checked_mul(u32::try_from((left - right).unsigned_abs() + 1).ok()?)?
            }
            Shape::Struct { w, .. } => w,
            Shape::Scalar { w, .. } => w,
        })
    }

    /// The width and signedness of a value of the type.
    fn ty_shape(&self, ty: Ty, env: &Env) -> Option<(u32, bool)> {
        if ty.drop == 0 {
            if let Some(r) = self.int_shape(ty.dt, ty.scope, env, 0) {
                return Some(r);
            }
        }
        match self.shape(ty, env, 0)? {
            Shape::Scalar { w, s } => Some((w, s)),
            Shape::Struct { w, signed, .. } => Some((w, signed)),
            Shape::Array { .. } => Some((self.width(ty, env, 0)?, false)),
        }
    }

    /// A packed struct, union or array type (not a plain integer).
    pub(crate) fn packed_aggregate(&self, dt: NodeRef, scope: usize, env: &Env) -> bool {
        matches!(self.shape(Ty::of(dt, scope), env, 0), Some(Shape::Struct { .. } | Shape::Array { .. }))
    }

    /// The declarator holding value `v` has unpacked dimensions.
    pub(crate) fn unpacked_decl(&self, v: NodeRef) -> bool {
        let t = self.tree(v);
        self.parent_node(v).is_some_and(|d| t.kind(d) == Declarator && t.children(d).any(|c| t.kind(c) == Dim))
    }


    // ---- the static type of an expression -----------------------------------

    /// The declared type of a constant, a function local or argument.
    pub(crate) fn expr_ty(&self, e: NodeRef, scope: usize, env: &Env) -> Option<Ty> {
        match self.kind(e) {
            NameExpr => {
                let toks = self.toks_of(e);
                let [n] = toks.as_slice() else { return None };
                if self.frames.borrow().last().is_some_and(|f| f.contains_key(*n)) {
                    return self.ftypes.borrow().last()?.get(*n).copied();
                }
                self.const_ty(n, scope, 0)
            }
            ScopedExpr => {
                let toks = self.toks_of(e);
                if let [p, "::", n] = toks.as_slice() {
                    return self.const_ty(n, *self.packages.get(*p)?, 0);
                }
                None
            }
            ParenExpr => self.expr_ty(*self.children(e).first()?, scope, env),
            MemberExpr => {
                let base = *self.children(e).first()?;
                let field = *self.own_tokens(e).last()?;
                let ty = self.expr_ty(base, scope, env)?;
                match self.shape(ty, env, 0)? {
                    Shape::Struct { fields, .. } => fields.into_iter().find(|f| f.name == field).map(|f| f.ty),
                    _ => None,
                }
            }
            SelectExpr => {
                if self.own_tokens(e).iter().any(|t| matches!(*t, ":" | "+:" | "-:")) {
                    return None;
                }
                let base = *self.children(e).first()?;
                let ty = self.expr_ty(base, scope, env)?;
                match self.shape(ty, env, 0)? {
                    Shape::Array { elem, .. } => Some(elem),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// A named constant's declared type, up the scopes and through imports.
    fn const_ty(&self, name: &str, scope: usize, depth: usize) -> Option<Ty> {
        if depth > 8 {
            return None;
        }
        let mut cur = Some(scope);
        while let Some(s) = cur {
            if let Some(&(dt, _)) = self.scopes[s].consts.get(name) {
                return Some(Ty::of(dt?, s));
            }
            if !matches!(self.scopes[s].kind, ScopeKind::Class(_)) {
                for (pkg, item) in &self.scopes[s].imports {
                    if item.as_deref().is_some_and(|i| i != name) {
                        continue;
                    }
                    if let Some(&p) = self.packages.get(pkg) {
                        if p != s && self.scopes[p].consts.contains_key(name) {
                            return self.const_ty(name, p, depth + 1);
                        }
                    }
                }
            }
            cur = self.scopes[s].parent;
        }
        None
    }

    // ---- reading members and elements ---------------------------------------

    /// Where `base.field` or `base[i]`, `base[a:b]`, `base[a+:n]` is in
    /// its base's value: (lowest bit, width, signed). `None` when out of
    /// range or not a packed select.
    pub(crate) fn select_bits(&self, e: NodeRef, scope: usize, env: &Env, depth: usize) -> Option<(u32, u32, bool)> {
        let kids = self.children(e);
        let base = *kids.first()?;
        let ty = self.expr_ty(base, scope, env);
        if self.kind(e) == MemberExpr {
            let field = *self.own_tokens(e).last()?;
            let Shape::Struct { fields, .. } = self.shape(ty?, env, 0)? else { return None };
            let f = fields.into_iter().find(|f| f.name == field)?;
            let s = self.ty_shape(f.ty, env).map_or(false, |x| x.1);
            return Some((f.lo, f.w, s));
        }
        // A select: by the declared dimension, else bit `i` of an integer.
        let (left, right, elem_w) = match ty.and_then(|t| self.shape(t, env, 0)) {
            Some(Shape::Array { left, right, elem_w, .. }) => (left, right, elem_w),
            _ => {
                let w = self.eval(base, scope, env, depth + 1)?.w as i128;
                (w - 1, 0, 1)
            }
        };
        let own = self.own_tokens(e);
        let idx = |i: i128| -> Option<u32> {
            // Bit offset of element `i` from the low end.
            let off = if left >= right { i - right } else { right - i };
            let n = (left - right).abs();
            (0..=n).contains(&off).then(|| u32::try_from(off).ok()).flatten()
        };
        let a = self.eval(*kids.get(1)?, scope, env, depth + 1)?.i();
        let (lo_el, n) = if own.contains(&":") {
            let b = self.eval(*kids.get(2)?, scope, env, depth + 1)?.i();
            let (x, y) = (idx(a)?, idx(b)?);
            (x.min(y), x.abs_diff(y) + 1)
        } else if own.contains(&"+:") || own.contains(&"-:") {
            let n = self.eval(*kids.get(2)?, scope, env, depth + 1)?.i();
            if n <= 0 {
                return None;
            }
            let other = if own.contains(&"+:") { a + n - 1 } else { a - n + 1 };
            let (x, y) = (idx(a)?, idx(other)?);
            (x.min(y), n as u32)
        } else {
            (idx(a)?, 1)
        };
        let signed = n == 1
            && ty.and_then(|t| self.shape(t, env, 0)).is_some_and(|sh| match sh {
                Shape::Array { elem, .. } => self.ty_shape(elem, env).is_some_and(|x| x.1),
                _ => false,
            });
        Some((lo_el.checked_mul(elem_w)?, n.checked_mul(elem_w)?, signed))
    }

    /// `base.field`, `base[i]` and part-selects of a packed constant.
    pub(crate) fn eval_packed_select(&self, e: NodeRef, scope: usize, env: &Env, depth: usize) -> Option<CVal> {
        let base = *self.children(e).first()?;
        let (lo, w, s) = self.select_bits(e, scope, env, depth)?;
        let v = self.eval(base, scope, env, depth + 1)?;
        if lo.checked_add(w)? > v.w {
            return None;
        }
        Some(v.slice(lo, w, s))
    }

    // ---- building values of a type -------------------------------------------

    /// The value of `e` assigned to a variable of type `ty`: assignment
    /// patterns are laid out by the type, anything else converted to it.
    pub(crate) fn eval_typed(&self, e: NodeRef, ty: Ty, scope: usize, env: &Env, depth: usize) -> Option<CVal> {
        let v = self.eval_typed4(e, ty, scope, env, depth)?;
        // x and z read as 0 in a two-state type.
        Some(if self.two_state(ty.dt) { v.two_state() } else { v })
    }

    fn eval_typed4(&self, e: NodeRef, ty: Ty, scope: usize, env: &Env, depth: usize) -> Option<CVal> {
        if depth > types::MAX_EVAL_DEPTH {
            return None;
        }
        let Some((w, s)) = self.ty_shape(ty, env) else {
            // A type we cannot size: the value as it is.
            if self.kind(e) == AssignPattern {
                return None;
            }
            return self.eval(e, scope, env, depth + 1);
        };
        let toks = self.toks_of(e);
        match toks.as_slice() {
            ["'1"] => return Some(CVal::new(w, s, -1i128)),
            ["'0"] => return Some(CVal::new(w, s, 0i128)),
            ["'x" | "'X"] => return Some(CVal::all_x(w, s)),
            ["'z" | "'Z"] => return Some(CVal::four(w, s, Int::from(-1), Int::from(-1))),
            _ => {}
        }
        if self.kind(e) != AssignPattern {
            return self.eval_ctx(e, w, s, scope, env, depth + 1);
        }
        let items = self.pattern_items(e);
        let default = items.iter().find(|(k, _)| matches!(k, Key::Default)).map(|x| x.1);
        let mut acc = CVal::new(w, s, 0i128);
        match self.shape(ty, env, 0)? {
            Shape::Struct { fields, .. } => {
                let positional = items.iter().all(|(k, _)| matches!(k, Key::None));
                if positional && items.len() != fields.len() {
                    return None;
                }
                for (i, f) in fields.iter().enumerate() {
                    let x = if positional {
                        Some(items[i].1)
                    } else {
                        items
                            .iter()
                            .find(|(k, _)| matches!(k, Key::Name(n) if *n == f.name))
                            .map(|x| x.1)
                            .or_else(|| self.typed_default(&items, f.ty, env))
                            .or(default)
                    };
                    let fv = self.eval_typed(x?, f.ty, scope, env, depth + 1)?;
                    acc = acc.with_bits(f.lo, &fv.cast(f.w, false));
                }
            }
            Shape::Array { left, right, elem, elem_w } => {
                let n = (left - right).unsigned_abs() as usize + 1;
                let positional = items.iter().all(|(k, _)| matches!(k, Key::None));
                if positional && items.len() != n {
                    return None;
                }
                for k in 0..n {
                    // Element k from the left: `left`, then towards `right`.
                    let index = if left >= right { left - k as i128 } else { left + k as i128 };
                    let x = if positional {
                        Some(items[k].1)
                    } else {
                        let mut hit = None;
                        for (key, val) in &items {
                            if let Key::Index(ix) = key {
                                if self.eval(*ix, scope, env, depth + 1).is_some_and(|c| c.i() == index) {
                                    hit = Some(*val);
                                }
                            }
                        }
                        hit.or(default)
                    };
                    let ev = self.eval_typed(x?, elem, scope, env, depth + 1)?;
                    let at = (n - 1 - k) as u32 * elem_w;
                    acc = acc.with_bits(at, &ev.cast(elem_w, false));
                }
            }
            Shape::Scalar { .. } => {
                // `'{default: x}` of an integer: every bit from x.
                let x = self.eval(default?, scope, env, depth + 1)?;
                return Some(x.cast(w, s));
            }
        }
        Some(acc)
    }

    /// `e` evaluated in a context `w` bits wide (IEEE 1800 11.6.1): the
    /// operands of `+ - * / % & | ^ ~^`, of `?:` and the left of a shift
    /// are widened to the context before the operation, so
    /// `(2 << XLEN-2)` keeps its bits in a 64-bit parameter.
    pub(crate) fn eval_ctx(&self, e: NodeRef, w: u32, s: bool, scope: usize, env: &Env, depth: usize) -> Option<CVal> {
        let v = self.eval(e, scope, env, depth + 1)?;
        if v.w >= w || depth > types::MAX_EVAL_DEPTH {
            return Some(v.cast(w, s));
        }
        let kids: Vec<NodeRef> = self.children(e).into_iter().filter(|c| types::is_expr(self.kind(*c))).collect();
        let own = self.own_tokens(e);
        // The width and signedness the operands take on.
        let (cw, cs) = (w.max(v.w), v.s);
        let sub = |k: NodeRef| self.eval_ctx(k, cw, cs, scope, env, depth + 1);
        let out = match (self.kind(e), own.first().copied()) {
            (ParenExpr, _) => sub(*kids.first()?)?,
            (BinaryExpr, Some(op @ ("+" | "-" | "*" | "/" | "%" | "&" | "|" | "^" | "~^" | "^~"))) => {
                types::binary_op(op, sub(*kids.first()?)?, sub(*kids.get(1)?)?)?
            }
            (BinaryExpr, Some(op @ ("<<" | ">>" | "<<<" | ">>>"))) => {
                let n = self.eval(*kids.get(1)?, scope, env, depth + 1)?;
                types::binary_op(op, sub(*kids.first()?)?, n)?
            }
            (PrefixExpr, Some("-")) => {
                let a = sub(*kids.first()?)?;
types::unary("-", a)?
            }
            (PrefixExpr, Some("~")) => {
                let a = sub(*kids.first()?)?;
types::unary("~", a)?
            }
            (CondExpr, _) => {
                let c = self.eval(*kids.first()?, scope, env, depth + 1)?;
                match c.truth() {
                    Some(t) => sub(*kids.get(if t { 1 } else { 2 })?)?,
                    None => CVal::merge(&sub(*kids.get(1)?)?, &sub(*kids.get(2)?)?),
                }
            }
            // A leaf: extended by its own sign only when the context is signed.
            _ => CVal { s: v.s && cs, ..v }.norm().cast(cw, cs),
        };
        Some(out.cast(w, s))
    }

    /// A `type: value` item that matches a member's type (`int: 0`).
    fn typed_default(&self, items: &[(Key, NodeRef)], ty: Ty, _env: &Env) -> Option<NodeRef> {
        let want = self.toks_of(ty.dt).concat();
        items.iter().find(|(k, _)| matches!(k, Key::Type(t) if *t == want)).map(|x| x.1)
    }

    /// The items of an assignment pattern with their keys.
    fn pattern_items(&self, e: NodeRef) -> Vec<(Key, NodeRef)> {
        let t = self.tree(e);
        let mut out = Vec::new();
        let mut pending: Vec<NodeRef> = Vec::new();
        let mut default = false;
        let mut colon = false;
        let mut key_tok: Option<String> = None;
        let flush = |pending: &mut Vec<NodeRef>,
                     default: &mut bool,
                     colon: &mut bool,
                     key_tok: &mut Option<String>,
                     out: &mut Vec<(Key, NodeRef)>| {
            if *default {
                if let Some(v) = pending.last() {
                    out.push((Key::Default, *v));
                }
            } else if let Some(k) = key_tok.take() {
                if let Some(v) = pending.last() {
                    out.push((Key::Type(k), *v));
                }
            } else if *colon && pending.len() == 2 {
                let key = pending[0];
                let k = if self.kind(key) == NameExpr && self.toks_of(key).len() == 1 {
                    Key::Name(self.toks_of(key)[0].to_string())
                } else {
                    Key::Index(key)
                };
                out.push((k, pending[1]));
            } else if let Some(v) = pending.first() {
                out.push((Key::None, *v));
            }
            pending.clear();
            *default = false;
            *colon = false;
            *key_tok = None;
        };
        for c in &t.node(e.node).children {
            match *c {
                sv_syntax::Child::Token(i) => match &*t.tokens[i as usize].text {
                    "," | "}" => flush(&mut pending, &mut default, &mut colon, &mut key_tok, &mut out),
                    "default" => default = true,
                    ":" => colon = true,
                    "'{" => {}
                    k if pending.is_empty() && !colon => key_tok = Some(k.to_string()),
                    _ => {}
                },
                sv_syntax::Child::Node(n) => {
                    let r = NodeRef { tree: e.tree, node: n };
                    if self.kind(r) == DataType && pending.is_empty() {
                        key_tok = Some(self.toks_of(r).concat());
                    } else {
                        pending.push(r);
                    }
                }
            }
        }
        out
    }

    /// Stores `value` into the part of a function local that `lhs` names
    /// (`cfg.XLEN = ...`, `cfg.Regions[i] = ...`); `None` if `lhs` is not
    /// such a path.
    pub(crate) fn assign_into(
        &self,
        lhs: NodeRef,
        rhs: NodeRef,
        op: &str,
        scope: usize,
        env: &Env,
        depth: usize,
    ) -> Option<()> {
        // The root local and the bits the path selects in it.
        let mut path = Vec::new();
        let mut cur = lhs;
        while matches!(self.kind(cur), MemberExpr | SelectExpr) {
            path.push(cur);
            cur = *self.children(cur).first()?;
        }
        if self.kind(cur) != NameExpr {
            return None;
        }
        let name = self.toks_of(cur).concat();
        let mut lo = 0u32;
        for &p in path.iter().rev() {
            let (l, _, _) = self.select_bits(p, scope, env, depth)?;
            lo = lo.checked_add(l)?;
        }
        let (_, w, _) = self.select_bits(lhs, scope, env, depth)?;
        let ty = self.expr_ty(lhs, scope, env);
        let v = match ty {
            Some(t) if op == "=" => self.eval_typed(rhs, t, scope, env, depth + 1)?,
            _ => self.eval(rhs, scope, env, depth + 1)?,
        };
        // `x[i] op= v` reads the selected bits first.
        let v = match op.strip_suffix('=') {
            Some(bop) if !bop.is_empty() && op != "==" && op != "<=" && op != ">=" && op != "!=" => {
                let cur = self.eval_packed_select(lhs, scope, env, depth + 1)?;
                super::types::binary_op(bop, cur, v)?
            }
            _ => v,
        };
        let root = self.get_local(&name)?;
        if lo.checked_add(w)? > root.w {
            return None;
        }
        let new = root.with_bits(lo, &v.cast(w, false));
        let mut frames = self.frames.borrow_mut();
        let slot = frames.last_mut()?.get_mut(&name)?;
        *slot = new;
        Some(())
    }
}

enum Key {
    None,
    Default,
    Name(String),
    /// An array index (or a member named by an expression).
    Index(NodeRef),
    /// A type key: `int: 0`.
    Type(String),
}
