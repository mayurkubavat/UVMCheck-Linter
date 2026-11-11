//! Type strings as slang prints them (`Type::toString`), and the constant
//! evaluation they need: packed and unpacked bounds, parameter values of
//! class specializations and enum values.

use super::*;

pub(crate) use super::bigint::Int;

/// Four-state logic value of a single bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BitState {
    /// Logic `0`.
    Zero,
    /// Logic `1`.
    One,
    /// Unknown `x`.
    X,
    /// High-impedance `z`.
    Z,
}

/// A constant integer: its width, signedness and four-state value, of any
/// width. `v` is kept normalized: below 2^w when unsigned, and in
/// -2^(w-1) .. 2^(w-1) when signed. `u` marks unknown bits (below 2^w);
/// at those bits `v` holds 0 for x and 1 for z, as in slang.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CVal {
    /// Bit width (`>= 1` for sized values).
    pub w: u32,
    /// Whether the constant is signed (`2'sb11`).
    pub s: bool,
    /// Value bits (`0`/`1` when known; `0` for `x` and `1` for `z` where `u` is `1`).
    pub v: Int,
    /// Unknown mask (`1` where the bit is `x` or `z`).
    pub u: Int,
}

/// How deep constant evaluation may nest (expressions, calls), so a
/// constant defined by itself gives up.
pub(crate) const MAX_EVAL_DEPTH: usize = 200;

/// Maximum packed vector bit width accepted during elaboration and binding.
pub(crate) const MAX_BIT_WIDTH: u32 = 1 << 24;

/// Maximum element count accepted for unpacked dimensions, replications, and patterns.
pub(crate) const MAX_ARRAY_ELEMS: u32 = 1 << 20;

/// Maximum cycle count accepted in SVA delay and repetition bounds.
pub(crate) const MAX_SVA_CYCLES: u32 = 1 << 20;

/// Computes `|a - b| + 1` as a `u32` bounded by `max_len`, returning `None` on overflow.
pub(crate) fn range_len_u32(a: i128, b: i128, max_len: u32) -> Option<u32> {
    let span = a.abs_diff(b).checked_add(1)?;
    let cnt = u32::try_from(span).ok()?;
    (cnt <= max_len).then_some(cnt)
}

/// slang abbreviates a value with more active bits than this.
const ABBREVIATE_BITS: u32 = 128;

impl CVal {
    /// Constructs a two-state constant of width `w` and signedness `s`, normalized to `w` bits.
    pub fn new(w: u32, s: bool, v: impl Into<Int>) -> CVal {
        CVal { w, s, v: v.into(), u: Int::from(0) }.norm()
    }

    /// Every bit x.
    pub fn all_x(w: u32, s: bool) -> CVal {
        CVal { w, s, v: Int::from(0), u: Int::from(-1) }.norm()
    }

    /// Every bit z.
    pub fn all_z(w: u32, s: bool) -> CVal {
        CVal { w, s, v: Int::from(-1), u: Int::from(-1) }.norm()
    }

    /// Four-state logic value of bit `i` (`0` = LSB).
    pub fn bit_state(&self, i: u32) -> BitState {
        if i >= self.w {
            return BitState::Zero;
        }
        let v = self.bits_v().bit(i);
        let u = self.u.bit(i);
        match (u, v) {
            (false, false) => BitState::Zero,
            (false, true) => BitState::One,
            (true, false) => BitState::X,
            (true, true) => BitState::Z,
        }
    }

    /// Parses a standalone SystemVerilog integer or string literal (`8'hff`, `-4`, `'x`, `"abc"`).
    pub fn parse_literal(raw: &str) -> Option<CVal> {
        let s = raw.trim();
        if s.starts_with('"') {
            return string_value(s);
        }
        if let Some(rest) = s.strip_prefix('-') {
            return unary("-", Self::parse_literal(rest)?);
        }
        if let Some(rest) = s.strip_prefix('+') {
            return Self::parse_literal(rest);
        }
        literal(&[s])
    }

    /// Evaluates a binary operator on two 4-state constants using IEEE 1800 rules.
    pub fn eval_binary(op: &str, a: CVal, b: CVal) -> Option<CVal> {
        if matches!(op, "==?" | "!=?") {
            let w = a.w.max(b.w);
            let s = a.s && b.s;
            let (ca, cb) = (a.cast(w, s), b.cast(w, s));
            // IEEE 11.4.6: unknown bits (x/z/?) in RHS act as don't-care wildcards;
            // unknown bits in LHS at non-wildcard positions make the comparison 1'bx unless
            // another non-wildcard bit mismatches!
            let mask = cb.u.not().trunc(w, false);
            let diff_known = ca.bits_v().xor(&cb.bits_v()).and(&mask).and(&ca.u.not());
            if !diff_known.is_zero() {
                return Some(CVal::bool(op == "!=?"));
            }
            let lhs_unk = ca.u.and(&mask);
            if !lhs_unk.is_zero() {
                return Some(CVal::all_x(1, false));
            }
            return Some(CVal::bool(op == "==?"));
        }
        binary(op, a, b)
    }

    /// Evaluates a unary or reduction operator on a 4-state constant.
    pub fn eval_unary(op: &str, a: CVal) -> Option<CVal> {
        if let Some(r) = unary(op, a.clone()) {
            return Some(r);
        }
        let w = a.w.max(1);
        let full = Int::from(-1).trunc(w, false);
        let a1 = a.ones();
        let a0 = a.bits_v().or(&a.u).not().trunc(w, false);
        match op {
            "&" | "~&" => {
                let res = if !a0.is_zero() {
                    CVal::bool(false)
                } else if a1 == full {
                    CVal::bool(true)
                } else {
                    CVal::all_x(1, false)
                };
                Some(if op == "~&" { unary("!", res)? } else { res })
            }
            "|" | "~|" => {
                let res = if !a1.is_zero() {
                    CVal::bool(true)
                } else if a0 == full {
                    CVal::bool(false)
                } else {
                    CVal::all_x(1, false)
                };
                Some(if op == "~|" { unary("!", res)? } else { res })
            }
            "^" | "~^" | "^~" => {
                if !a.known() {
                    return Some(CVal::all_x(1, false));
                }
                let mut parity = false;
                for i in 0..w {
                    parity ^= a1.bit(i);
                }
                if op != "^" {
                    parity = !parity;
                }
                Some(CVal::bool(parity))
            }
            _ => None,
        }
    }

    /// Value and unknown bits as unsigned integers of the width.
    pub fn four(w: u32, s: bool, v: Int, u: Int) -> CVal {
        let u = u.trunc(w.max(1), false);
        CVal { w, s, v, u }.norm()
    }

    /// Returns `true` when every bit is `0` or `1` with no `x` or `z` bits.
    pub fn known(&self) -> bool {
        self.u.is_zero()
    }

    /// The value bits as an unsigned integer of the width.
    pub fn bits_v(&self) -> Int {
        self.v.trunc(self.w.max(1), false)
    }

    /// Known one bits.
    fn ones(&self) -> Int {
        self.bits_v().and(&self.u.not())
    }

    /// True, false, or unknown (x) as a condition.
    pub fn truth(&self) -> Option<bool> {
        if !self.ones().is_zero() {
            Some(true)
        } else if self.known() {
            Some(false)
        } else {
            None
        }
    }

    /// Bits where `a` and `b` agree, x elsewhere (same width and sign).
    pub fn merge(a: &CVal, b: &CVal) -> CVal {
        let (av, bv) = (a.bits_v(), b.bits_v());
        let u = a.u.or(&b.u).or(&av.xor(&bv));
        CVal::four(a.w, a.s, av.and(&u.not()), u)
    }

    /// Bits `lo..lo+w` as a value of width `w`.
    pub fn slice(&self, lo: u32, w: u32, s: bool) -> CVal {
        CVal::four(w, s, self.bits_v().bits(lo, w), self.u.bits(lo, w))
    }

    /// `hi` above `lo` (`{hi, lo}`), unsigned.
    pub fn concat(hi: &CVal, lo: &CVal) -> CVal {
        let w = hi.w + lo.w;
        let v = hi.bits_v().shl(lo.w).or(&lo.bits_v());
        let u = hi.u.shl(lo.w).or(&lo.u);
        CVal::four(w.max(1), false, v, u)
    }

    /// The bits of `x` written at `lo..lo+x.w` of this value.
    pub fn with_bits(&self, lo: u32, x: &CVal) -> CVal {
        let ones = Int::from(1).shl(x.w).sub(&Int::from(1)).shl(lo);
        let keep = ones.not();
        let v = self.bits_v().and(&keep).or(&x.bits_v().trunc(x.w, false).shl(lo));
        let u = self.u.and(&keep).or(&x.u.trunc(x.w, false).shl(lo));
        CVal::four(self.w, self.s, v, u)
    }

    /// Into a two-state type: unknown bits become 0.
    pub fn two_state(&self) -> CVal {
        if self.known() {
            return self.clone();
        }
        CVal::new(self.w, self.s, self.ones())
    }

    /// Constructs a 32-bit signed two-state integer constant (`int`).
    pub fn int(v: i128) -> CVal {
        CVal::new(32, true, v)
    }

    /// Constructs a 1-bit unsigned two-state boolean constant (`1'b0` or `1'b1`).
    pub fn bool(b: bool) -> CVal {
        CVal::new(1, false, b as i128)
    }

    /// The value as an index or count (saturating beyond i128).
    pub fn i(&self) -> i128 {
        self.v.low_i128()
    }

    /// Returns `true` when at least one bit is known `1` under IEEE 1800 condition truth rules.
    pub fn is_true(&self) -> bool {
        self.truth() == Some(true)
    }

    /// Truncated to the width, sign-extended if signed.
    pub fn norm(self) -> CVal {
        let w = self.w.max(1);
        let v = self.v.trunc(w, self.s);
        let u = self.u.trunc(w, false);
        CVal { w: self.w, s: self.s, v, u }
    }

    /// Resizes and reinterprets this constant to width `w` and signedness `s`.
    ///
    /// Widening sign-extends both value and top-bit `x`/`z` states when `self.s` is `true`,
    /// and zero-extends when `self.s` is `false`.
    pub fn cast(&self, w: u32, s: bool) -> CVal {
        // A signed value's unknown top bit extends.
        let u = if self.s && w > self.w { self.u.trunc(self.w.max(1), true) } else { self.u.clone() };
        // `v` is sign-normalized, so x (0) extends as x and z (1) as z.
        CVal { w, s, v: self.v.clone(), u }.norm()
    }

    /// The value plus a small step, in the same type.
    pub fn bump(&self, d: i128) -> CVal {
        if !self.known() {
            return CVal::all_x(self.w, self.s);
        }
        CVal::new(self.w, self.s, self.v.add(&Int::from(d)))
    }

    /// slang's SVInt::writeTo: an optional `-`, the base, the digits;
    /// over 128 active bits, decimal cut to its leading digits with
    /// `...e<n>`.
    fn write(&self, radix: u32, mut with_base: bool) -> String {
        if !self.known() {
            return self.write_unknown(radix);
        }
        let neg = self.s && self.v.is_neg();
        let mag = if neg { self.v.neg() } else { self.v.clone() };
        let mut radix = radix;
        let active = mag.active_bits();
        if active > ABBREVIATE_BITS {
            with_base = true;
            radix = 10;
        }
        let mut out = String::new();
        if neg {
            out.push('-');
        }
        if with_base {
            out.push_str(&self.w.to_string());
            out.push('\'');
            if self.s {
                out.push('s');
            }
            out.push(match radix {
                2 => 'b',
                8 => 'o',
                10 => 'd',
                _ => 'h',
            });
        }
        if radix == 10 && active > ABBREVIATE_BITS {
            let exp = (((active - ABBREVIATE_BITS) as f64 / std::f64::consts::LOG2_10).ceil() as u32).max(5);
            out.push_str(&mag.div_pow10(exp).digits(10));
            out.push_str(&format!("...e{exp}"));
        } else {
            out.push_str(&mag.digits(radix));
        }
        out
    }

    /// slang's SVInt::writeTo for a value with unknown bits: decimal is
    /// one letter, other bases a digit (or x, z, X, Z) per group of bits.
    fn write_unknown(&self, radix: u32) -> String {
        let mut out = format!(
            "{}'{}{}",
            self.w,
            if self.s { "s" } else { "" },
            match radix {
                2 => 'b',
                8 => 'o',
                10 => 'd',
                _ => 'h',
            }
        );
        let w = self.w.max(1);
        let all_u = self.u == Int::from(-1).trunc(w, false);
        let v = self.bits_v();
        if radix == 10 {
            let any_x = !self.u.and(&v.not()).is_zero();
            out.push(if all_u && v == self.u {
                'z'
            } else if all_u && v.is_zero() {
                'x'
            } else if any_x {
                'X'
            } else {
                'Z'
            });
            return out;
        }
        let shift = match radix {
            2 => 1,
            8 => 3,
            _ => 4,
        };
        let mut digits = Vec::new();
        let (mut tv, mut tu) = (v, self.u.clone());
        let mut left = w as i64;
        while !tv.is_zero() || !tu.is_zero() {
            let n = if left < shift as i64 { left.max(0) as u32 } else { shift };
            let mask = (1u32 << n) - 1;
            let d = tv.bits(0, shift).low_i128() as u32 & mask;
            let du = tu.bits(0, shift).low_i128() as u32 & mask;
            digits.push(if du == 0 {
                char::from_digit(d, 16).unwrap_or('?')
            } else if du == mask && d == 0 {
                'x'
            } else if du == mask && d == mask {
                'z'
            } else if !d & du != 0 {
                'X'
            } else {
                'Z'
            });
            tv = tv.shr(shift);
            tu = tu.shr(shift);
            left -= shift as i64;
        }
        if left > 0 && matches!(digits.last(), Some('x' | 'z')) {
            digits.push('0');
        }
        if digits.is_empty() {
            digits.push('0');
        }
        out.extend(digits.iter().rev());
        out
    }

    /// slang's default form: binary below 8 unsigned bits, decimal up to
    /// 32 bits or signed, hex above; a plain number for a signed 32-bit
    /// decimal.
    pub fn show(&self) -> String {
        if !self.known() {
            let w = self.w.max(1);
            let all_u = self.u == Int::from(-1).trunc(w, false);
            let all_xz = all_u && (self.bits_v().is_zero() || self.bits_v() == self.u);
            let radix = if (self.w < 8 && !self.s) || (self.w <= 64 && !all_xz) {
                2
            } else if self.w <= 32 || self.s || all_xz {
                10
            } else {
                16
            };
            return self.write_unknown(radix);
        }
        let radix = if self.w < 8 && !self.s {
            2
        } else if self.w <= 32 || self.s {
            10
        } else {
            16
        };
        self.write(radix, !(radix == 10 && self.w == 32 && self.s))
    }

    /// Always decimal with its size, as enum values print.
    pub fn show_dec(&self) -> String {
        self.write(10, true)
    }
}

pub(crate) const INT_TYPES: [&str; 9] =
    ["bit", "logic", "reg", "int", "integer", "byte", "shortint", "longint", "time"];

fn int_width(k: &str) -> (u32, bool) {
    match k {
        "int" | "integer" => (32, true),
        "byte" => (8, true),
        "shortint" => (16, true),
        "longint" => (64, true),
        "time" => (64, false),
        _ => (1, false),
    }
}

impl<'t> Builder<'t> {
    pub(crate) fn toks_of(&self, n: NodeRef) -> Vec<&'t str> {
        let t = self.tree(n);
        match t.token_bounds(n.node) {
            Some((a, b)) if t.spans_known(n.node) => (a..=b).map(|i| &*t.tokens[i as usize].text).collect(),
            Some(_) => self.tokens(n).iter().map(|&i| self.text(n.tree, i)).collect(),
            None => Vec::new(),
        }
    }

    /// `pkg::cls::` for a scope (empty for the compilation unit).
    pub(crate) fn scope_path(&self, s: usize) -> String {
        let mut parts = Vec::new();
        let mut cur = Some(s);
        while let Some(c) = cur {
            match self.scopes[c].kind {
                ScopeKind::Unit => {}
                // slang names a design unit's members hierarchically: `m.t`.
                ScopeKind::Design => parts.push(format!("{}.", self.scopes[c].name)),
                _ => parts.push(format!("{}::", self.scopes[c].name)),
            }
            cur = self.scopes[c].parent;
        }
        parts.reverse();
        parts.concat()
    }

    // ---- constants ----------------------------------------------------------

    /// Width and signedness of an integral data type, if it is one.
    pub(crate) fn int_shape(&self, dt: NodeRef, scope: usize, env: &Env, depth: usize) -> Option<(u32, bool)> {
        if depth > MAX_EVAL_DEPTH {
            return None;
        }
        let own = self.own_tokens(dt);
        let kw = own.iter().find(|t| INT_TYPES.contains(t)).copied();
        let implicit = kw.is_none()
            && own.iter().all(|t| matches!(*t, "signed" | "unsigned" | "parameter" | "localparam"))
            && (!own.is_empty() || self.child(dt, Dim).is_some());
        if kw.is_none() && !implicit {
            let toks = self.toks_of(dt);
            // A type parameter: bound, or its default in a design unit.
            if let [name] = toks.as_slice() {
                if let Some(bd) = env.get(*name) {
                    return self.int_shape(bd.node, bd.scope, &bd.env, depth + 1);
                }
                let mut cur = Some(scope);
                while let Some(c) = cur {
                    if let Some(d) = self.scopes[c].type_params.get(*name) {
                        return self.int_shape((*d)?, c, env, depth + 1);
                    }
                    cur = self.scopes[c].parent;
                }
            }
            // A typedef of an integral type.
            if toks.len() == 1 || toks.contains(&"::") {
                if let Some(Found::Typedef(t, s)) = self.lookup_type(dt, scope) {
                    let inner = self.child(t, DataType)?;
                    if self.child(t, Dim).is_some() {
                        return None;
                    }
                    let (mut w, sg) = self.int_shape(inner, s, env, depth + 1)?;
                    // `pkg::t [N-1:0]`: the use site's own packed dimensions.
                    for d in self.children(dt) {
                        if self.kind(d) == Dim {
                            let (a, b) = self.dim_range(d, scope, env)?;
                            let cnt = range_len_u32(a, b, MAX_BIT_WIDTH)?;
                            w = w.checked_mul(cnt).filter(|&tw| tw <= MAX_BIT_WIDTH)?;
                        }
                    }
                    return Some((w, sg));
                }
            }
            if matches!(own.first(), Some(&"struct" | &"union")) && own.contains(&"packed") {
                // Members' widths: summed, or the widest for a union.
                let union = own.first() == Some(&"union");
                let mut w: u32 = 0;
                for m in self.children(dt) {
                    if self.kind(m) != DataDecl {
                        continue;
                    }
                    let (mw, _) = self.int_shape(self.child(m, DataType)?, scope, env, depth + 1)?;
                    let n = self.children(m).into_iter().filter(|d| self.kind(*d) == Declarator).count() as u32;
                    w = if union { w.max(mw) } else { w.checked_add(mw.checked_mul(n)?)? };
                    if w > MAX_BIT_WIDTH {
                        return None;
                    }
                }
                let mut w = w.max(1);
                for d in self.children(dt) {
                    if self.kind(d) == Dim {
                        let (a, b) = self.dim_range(d, scope, env)?;
                        let cnt = range_len_u32(a, b, MAX_BIT_WIDTH)?;
                        w = w.checked_mul(cnt).filter(|&tw| tw <= MAX_BIT_WIDTH)?;
                    }
                }
                return Some((w, own.contains(&"signed")));
            }
            if own.first() == Some(&"enum") {
                return Some(match self.child(dt, DataType) {
                    Some(b) => self.int_shape(b, scope, env, depth + 1)?,
                    None => (32, true),
                });
            }
            return None;
        }
        let (mut w, mut s) = kw.map_or((1, false), int_width);
        if own.contains(&"signed") {
            s = true;
        }
        if own.contains(&"unsigned") {
            s = false;
        }
        for d in self.children(dt) {
            if self.kind(d) == Dim {
                let (a, b) = self.dim_range(d, scope, env)?;
                let cnt = range_len_u32(a, b, MAX_BIT_WIDTH)?;
                w = w.checked_mul(cnt).filter(|&tw| tw <= MAX_BIT_WIDTH)?;
            }
        }
        Some((w, s))
    }

    pub(crate) fn lookup_type_pub(&self, dt: NodeRef, scope: usize) -> Option<Found> {
        self.lookup_type(dt, scope)
    }

    pub(crate) fn lookup_type(&self, dt: NodeRef, scope: usize) -> Option<Found> {
        let toks = self.toks_of(dt);
        let path: Vec<&str> =
            toks.iter().take_while(|t| **t != "#" && **t != "[").filter(|t| **t != "::").copied().collect();
        match path.len() {
            0 => None,
            1 => self.lookup(path[0], scope, 0),
            _ => self.lookup_path(&path, scope, 0),
        }
    }

    /// The bounds of a `[a:b]` or `[n]` dimension.
    /// A dimension slang rejects, which makes the declaration's type an
    /// error: a division or modulo by a constant zero in a bound, or (when
    /// `unpacked`) a size `[N]` that is not positive.
    pub(crate) fn dim_is_error(&self, d: NodeRef, scope: usize, env: &Env, unpacked: bool) -> bool {
        let exprs: Vec<NodeRef> = self.children(d).into_iter().filter(|c| body_is_expr(self.kind(*c))).collect();
        if unpacked && !self.own_tokens(d).contains(&":") {
            if let [n] = exprs.as_slice() {
                if self.eval(*n, scope, env, 0).and_then(dim_bound).is_some_and(|v| v <= 0) {
                    return true;
                }
            }
        }
        let mut todo = exprs;
        while let Some(e) = todo.pop() {
            let kids = self.children(e);
            if self.kind(e) == BinaryExpr && self.own_tokens(e).iter().any(|t| matches!(*t, "/" | "%")) {
                if let [_, r] = kids.as_slice() {
                    if self.eval(*r, scope, env, 0).is_some_and(|v| v.known() && v.i() == 0) {
                        return true;
                    }
                }
            }
            todo.extend(kids);
        }
        false
    }

    pub(crate) fn dim_range(&self, d: NodeRef, scope: usize, env: &Env) -> Option<(i128, i128)> {
        let exprs: Vec<NodeRef> = self.children(d).into_iter().filter(|c| body_is_expr(self.kind(*c))).collect();
        let own = self.own_tokens(d);
        match exprs.as_slice() {
            [a, b] if own.contains(&":") => {
                Some((dim_bound(self.eval(*a, scope, env, 0)?)?, dim_bound(self.eval(*b, scope, env, 0)?)?))
            }
            [n] if !own.contains(&":") => Some((0, dim_bound(self.eval(*n, scope, env, 0)?)? - 1)),
            _ => None,
        }
    }

    /// A named constant seen from `scope`: a parameter or localparam, with
    /// type parameters' and value parameters' bindings from `env`.
    fn const_value(
        &self,
        name: &str,
        s: usize,
        dt: Option<NodeRef>,
        val: Option<NodeRef>,
        env: &Env,
        depth: usize,
    ) -> Option<CVal> {
        let v = match (env.get(name), self.scopes[s].kind) {
            (Some(b), ScopeKind::Class(_)) => self.eval(b.node, b.scope, &b.env, depth + 1)?,
            _ => match dt {
                Some(d) => return self.eval_typed(val?, packed::Ty::of(d, s), s, env, depth + 1),
                None => self.eval(val?, s, env, depth + 1)?,
            },
        };
        Some(match dt.and_then(|d| self.int_shape(d, s, env, depth + 1)) {
            Some((w, sg)) => v.cast(w, sg),
            None => v,
        })
    }

    fn const_named(&self, name: &str, scope: usize, env: &Env, depth: usize) -> Option<CVal> {
        let mut cur = Some(scope);
        while let Some(s) = cur {
            if let Some(&(dt, val)) = self.scopes[s].consts.get(name) {
                // Outside a specialization the value is the scope's own.
                let plain = env.is_empty() && !matches!(self.scopes[s].kind, ScopeKind::Class(_));
                if plain {
                    if let Some(v) = self.const_cache.borrow().get(&(s, name.to_string())) {
                        return v.clone();
                    }
                }
                let v = self.const_value(name, s, dt, val, env, depth);
                if plain {
                    self.const_cache.borrow_mut().insert((s, name.to_string()), v.clone());
                }
                return v;
            }
            if let ScopeKind::Class(c) = self.scopes[s].kind {
                // Inherited parameters.
                if let Some((b, be)) = self.base_env(c, env, depth + 1) {
                    if let Some(v) = self.const_named(name, self.classes[b].scope, &be, depth + 1) {
                        return Some(v);
                    }
                }
            } else {
                for (pkg, item) in &self.scopes[s].imports {
                    if item.as_deref().is_some_and(|i| i != name) {
                        continue;
                    }
                    if let Some(&p) = self.packages.get(pkg) {
                        if self.scopes[p].consts.contains_key(name) {
                            return self.const_named(name, p, env, depth + 1);
                        }
                    }
                }
            }
            cur = self.scopes[s].parent;
        }
        // An enum member.
        self.enum_member(name, scope, depth)
    }

    fn enum_member(&self, name: &str, scope: usize, depth: usize) -> Option<CVal> {
        let mut cur = Some(scope);
        while let Some(s) = cur {
            // Imported: `import p::*;`, `import p::NAME;`.
            for (pkg, item) in &self.scopes[s].imports {
                if item.as_deref().is_some_and(|i| i != name) {
                    continue;
                }
                if let Some(&p) = self.packages.get(pkg) {
                    if p != s {
                        if let Some(v) = self.enum_in(name, p, depth) {
                            return Some(v);
                        }
                    }
                }
            }
            if let Some(v) = self.enum_in(name, s, depth) {
                return Some(v);
            }
            cur = self.scopes[s].parent;
        }
        None
    }

    /// The enum typedef (and its scope) that declares member `name`, seen
    /// from `scope`.
    pub(crate) fn enum_typedef(&self, name: &str, scope: usize) -> Option<(NodeRef, usize)> {
        let own = |s: usize| {
            let m = self.enum_typedefs.borrow().get(&s).cloned();
            let m = m.unwrap_or_else(|| {
                let mut m = HashMap::default();
                for t in self.scopes[s].typedefs.values().copied() {
                    let Some(dt) = self.child(t, DataType) else { continue };
                    if self.own_tokens(dt).first() != Some(&"enum") {
                        continue;
                    }
                    for d in self.children(dt) {
                        if self.kind(d) == Declarator {
                            if let Some(n) = self.name_of(d) {
                                m.entry(n.0).or_insert(t);
                            }
                        }
                    }
                }
                let m = std::rc::Rc::new(m);
                self.enum_typedefs.borrow_mut().insert(s, m.clone());
                m
            });
            m.get(name).copied()
        };
        let mut cur = Some(scope);
        while let Some(s) = cur {
            for (pkg, item) in &self.scopes[s].imports {
                if item.as_deref().is_some_and(|i| i != name) {
                    continue;
                }
                if let Some(&p) = self.packages.get(pkg) {
                    if let Some(t) = own(p) {
                        return Some((t, p));
                    }
                }
            }
            if let Some(t) = own(s) {
                return Some((t, s));
            }
            cur = self.scopes[s].parent;
        }
        None
    }

    /// A member of an enum typedef declared in scope `s` itself.
    fn enum_in(&self, name: &str, s: usize, depth: usize) -> Option<CVal> {
        if let Some(m) = self.enum_cache.borrow().get(&s) {
            return m.get(name).cloned();
        }
        let mut all: HashMap<String, CVal> = HashMap::default();
        for &t in self.scopes[s].typedefs.values() {
            if let Some(dt) = self.child(t, DataType) {
                if self.own_tokens(dt).first() == Some(&"enum") {
                    for (n, v) in self.enum_values(dt, s, &Env::default(), depth + 1) {
                        all.entry(n).or_insert(v);
                    }
                }
            }
        }
        let v = all.get(name).cloned();
        self.enum_cache.borrow_mut().insert(s, std::rc::Rc::new(all));
        v
    }

    /// Each enum member with its value.
    pub(crate) fn enum_values(&self, dt: NodeRef, scope: usize, env: &Env, depth: usize) -> Vec<(String, CVal)> {
        let mut out = Vec::new();
        if depth > MAX_EVAL_DEPTH {
            return out;
        }
        let (w, s) =
            self.child(dt, DataType).and_then(|b| self.int_shape(b, scope, env, depth + 1)).unwrap_or((32, true));
        let mut next = CVal::new(w, s, 0i128);
        for d in self.children(dt) {
            if self.kind(d) != Declarator {
                continue;
            }
            let Some((name, _)) = self.name_of(d) else {
                continue;
            };
            let val = self
                .children(d)
                .into_iter()
                .find(|c| body_is_expr(self.kind(*c)))
                .and_then(|e| self.eval(e, scope, env, depth + 1));
            let v = match val {
                Some(v) => v.cast(w, s),
                None => next.clone(),
            };
            // `C[2]` names C0 and C1.
            match self.child(d, Dim).and_then(|dim| self.dim_range(dim, scope, env)) {
                Some((a, b)) => {
                    let Some(cnt) = range_len_u32(a, b, MAX_ARRAY_ELEMS) else { continue };
                    let mut cur = v;
                    let n = i128::from(cnt);
                    // `C[2]` names C0, C1; `C[3:5]` names C3, C4, C5.
                    let two = self.child(d, Dim).is_some_and(|dim| {
                        self.children(dim).into_iter().filter(|c| body_is_expr(self.kind(*c))).count() == 2
                    });
                    let (start, step) = if two { (a, if b >= a { 1 } else { -1 }) } else { (0, 1) };
                    for k in 0..n {
                        let k = start + k * step;
                        let after = cur.bump(1);
                        out.push((format!("{name}{k}"), cur));
                        cur = after;
                    }
                    next = cur;
                }
                None => {
                    next = v.bump(1);
                    out.push((name, v));
                }
            }
        }
        out
    }

    /// The value of a constant expression.
    pub(crate) fn eval(&self, e: NodeRef, scope: usize, env: &Env, depth: usize) -> Option<CVal> {
        if depth > MAX_EVAL_DEPTH {
            return None;
        }
        match self.kind(e) {
            Literal => {
                if let Some(v) = self.lit_cache.borrow().get(&e) {
                    return v.clone();
                }
                let toks = self.toks_of(e);
                let v = match toks.as_slice() {
                    [lit] if lit.starts_with('"') => string_value(lit),
                    _ => literal(&toks),
                };
                self.lit_cache.borrow_mut().insert(e, v.clone());
                return v;
            }
            NameExpr => {
                let n = self.single_tok(e)?;
                if let Some(v) = self.get_local(n) {
                    return Some(v);
                }
                let here = self.inst.borrow().get(n).cloned();
                return match here {
                    Some(v) => v,
                    None => self.const_named(n, scope, env, depth + 1),
                };
            }
            _ => {}
        }
        // The first three operands (all `eval` reads), without allocating.
        let mut kids: [Option<NodeRef>; 3] = [None; 3];
        let t = self.tree(e);
        for (slot, n) in kids.iter_mut().zip(t.children(e.node).filter(|&c| body_is_expr(t.kind(c)))) {
            *slot = Some(NodeRef { tree: e.tree, node: n });
        }
        let kids = Kids(kids);
        match self.kind(e) {
            Literal | NameExpr => None,
            ScopedExpr => {
                let toks = self.toks_of(e);
                if toks.len() == 3 && toks[1] == "::" {
                    let p = *self.packages.get(toks[0])?;
                    return self.const_named(toks[2], p, &Env::default(), depth + 1);
                }
                None
            }
            ParenExpr => self.eval(*kids.first()?, scope, env, depth + 1),
            CastExpr => {
                let target = *kids.first()?;
                let a = self.eval(*kids.get(1)?, scope, env, depth + 1)?;
                let toks = self.toks_of(target);
                match toks.as_slice() {
                    ["signed"] => Some(a.cast(a.w, true)),
                    ["unsigned"] => Some(a.cast(a.w, false)),
                    ["const"] => Some(a),
                    [kw] if INT_TYPES.contains(kw) => {
                        let (w, s) = int_width(kw);
                        Some(a.cast(w, s))
                    }
                    _ if matches!(self.kind(target), NameExpr | ScopedExpr)
                        && self.int_shape(target, scope, env, depth + 1).is_some() =>
                    {
                        let (w, s) = self.int_shape(target, scope, env, depth + 1)?;
                        Some(a.cast(w, s))
                    }
                    // `8'(x)`, `(W)'(x)`
                    _ => {
                        let n = self.eval(target, scope, env, depth + 1)?.i();
                        if !(1..=1 << 24).contains(&n) {
                            self.invalid.set(n <= 0);
                            return None;
                        }
                        Some(a.cast(n as u32, a.s))
                    }
                }
            }
            ConcatExpr => self.eval_concat(e, scope, env, depth + 1),
            SelectExpr => {
                self.eval_select(e, scope, env, depth + 1).or_else(|| self.eval_packed_select(e, scope, env, depth + 1))
            }
            MemberExpr => self.eval_packed_select(e, scope, env, depth + 1),
            PrefixExpr => {
                let op = self.first_own_tok(e)?;
                let a = self.eval(*kids.first()?, scope, env, depth + 1)?;
                Some(unary(op, a)?)
            }
            BinaryExpr => {
                let op = self.first_own_tok(e)?;
                let (l, r) = (*kids.first()?, *kids.get(1)?);
                let a = self.eval(l, scope, env, depth + 1)?;
                let b = self.eval(r, scope, env, depth + 1)?;
                // Operands sized to each other (IEEE 11.6.1): a narrower
                // one is evaluated again in the wider context.
                if is_context_op(op) && (a.w != b.w || a.s != b.s) {
                    let (w, s) = (a.w.max(b.w), a.s && b.s);
                    let a = if a.w < w || a.s != s { self.eval_ctx(l, w, s, scope, env, depth + 1)? } else { a };
                    let b = if b.w < w || b.s != s { self.eval_ctx(r, w, s, scope, env, depth + 1)? } else { b };
                    return binary(op, a, b);
                }
                binary(op, a, b)
            }
            CondExpr => {
                let c = self.eval(*kids.first()?, scope, env, depth + 1)?;
                let (l, r) = (*kids.get(1)?, *kids.get(2)?);
                if c.truth().is_none() {
                    // An unknown condition: the branches merged bit by bit.
                    let (a, b) = (self.eval(l, scope, env, depth + 1)?, self.eval(r, scope, env, depth + 1)?);
                    let (w, s) = (a.w.max(b.w), a.s && b.s);
                    let a = if a.w < w { self.eval_ctx(l, w, s, scope, env, depth + 1)? } else { a.cast(w, s) };
                    let b = if b.w < w { self.eval_ctx(r, w, s, scope, env, depth + 1)? } else { b.cast(w, s) };
                    return Some(CVal::merge(&a, &b));
                }
                let (pick, other) = if c.is_true() { (l, r) } else { (r, l) };
                let v = self.eval(pick, scope, env, depth + 1)?;
                // The branch not taken only sizes the result (and may not
                // evaluate, as in `vec ? w / f(cfg) : 1`).
                let (w, s) = match self.eval(other, scope, env, depth + 1) {
                    Some(o) => (v.w.max(o.w), v.s && o.s),
                    None => (v.w, v.s),
                };
                if v.w < w {
                    return self.eval_ctx(pick, w, s, scope, env, depth + 1);
                }
                Some(v.cast(w, s))
            }
            CallExpr => {
                let callee = *self.children(e).first()?;
                let name = self.toks_of(callee).concat();
                let args: Vec<NodeRef> = self
                    .child(e, ArgList)
                    .map(|al| {
                        self.children(al)
                            .into_iter()
                            .filter(|a| self.kind(*a) == Arg)
                            .filter_map(|a| self.children(a).into_iter().next())
                            .collect()
                    })
                    .unwrap_or_default();
                match name.as_str() {
                    "$clog2" => {
                        let a = self.eval(*args.first()?, scope, env, depth + 1)?;
                        // Unknown bits count as 0, as in slang.
                        let m = a.two_state().v.trunc(a.w, false).sub(&Int::from(1));
                        Some(CVal::int(if m.is_neg() { 0 } else { m.active_bits() as i128 }))
                    }
                    "$bits" => {
                        let a = *args.first()?;
                        let w = match self.kind(a) {
                            DataType => self.int_shape(a, scope, env, depth + 1)?.0,
                            // A type name (typedef, type parameter) or a value.
                            NameExpr | ScopedExpr => match self.int_shape(a, scope, env, depth + 1) {
                                Some((w, _)) => w,
                                None => self.eval(a, scope, env, depth + 1)?.w,
                            },
                            _ => self.eval(a, scope, env, depth + 1)?.w,
                        };
                        Some(CVal::int(w as i128))
                    }
                    _ => self.call_const(&name, &args, scope, env, depth + 1),
                }
            }
            _ => None,
        }
    }

    // ---- type strings -------------------------------------------------------

    /// How slang prints a declaration's type: its data type (`None` for an
    /// implicit one) and the unpacked dimensions after the declared name.
    pub(crate) fn type_string(
        &self,
        dt: Option<NodeRef>,
        item: Option<NodeRef>,
        scope: usize,
        env: &Env,
        canonical: bool,
    ) -> String {
        let dims: Vec<NodeRef> =
            item.map(|i| self.children(i).into_iter().filter(|c| self.kind(*c) == Dim).collect()).unwrap_or_default();
        if dims.iter().any(|&d| self.dim_is_error(d, scope, env, true)) {
            return "<error>".into();
        }
        if dims.is_empty() {
            return match dt {
                Some(d) => self.ts(d, scope, env, canonical, 0),
                None => "logic".into(),
            };
        }
        // Only the type itself is canonical: an array keeps its element's
        // alias, unless that alias is an unpacked array itself, whose
        // dimensions then follow ours.
        let mut text = self.unpacked(&dims, scope, env);
        let mut cur = dt.map(|d| (d, scope));
        for _ in 0..16 {
            let Some((d, s)) = cur else { break };
            match self.lookup_type(d, s) {
                Some(Found::Typedef(t, ts)) if self.child(t, Dim).is_some() => {
                    let tdims: Vec<NodeRef> = self.children(t).into_iter().filter(|c| self.kind(*c) == Dim).collect();
                    let tenv = self.scope_env(ts, env);
                    text.push_str(&self.unpacked(&tdims, ts, &tenv));
                    cur = self.child(t, DataType).map(|i| (i, ts));
                }
                _ => break,
            }
        }
        let base = match cur {
            Some((d, s)) => self.ts(d, s, env, false, 0),
            None => "logic".into(),
        };
        format!("{base}${text}")
    }

    fn unpacked(&self, dims: &[NodeRef], scope: usize, env: &Env) -> String {
        let mut s = String::new();
        for &d in dims {
            let inner = {
                let t = self.toks_of(d);
                t[1..t.len().saturating_sub(1)].to_vec()
            };
            let text = match inner.as_slice() {
                [] => String::new(),
                ["$"] => "$".into(),
                ["*"] => "*".into(),
                ["$", ":", ..] => match self
                    .children(d)
                    .into_iter()
                    .find(|c| body_is_expr(self.kind(*c)))
                    .and_then(|e| self.eval(e, scope, env, 0))
                {
                    Some(v) => format!("$:{}", v.i()),
                    None => inner.concat(),
                },
                _ => {
                    if let Some(t) = self.child(d, DataType) {
                        self.ts(t, scope, env, false, 0)
                    } else if self.dim_shape(d, scope, 0) == TypeShape::AssociativeArray {
                        // `[key_t]` parsed as a name.
                        let e = self.children(d).into_iter().next();
                        e.map_or(inner.concat(), |e| self.ts(e, scope, env, false, 0))
                    } else {
                        match self.dim_range(d, scope, env) {
                            Some((a, b)) => format!("{a}:{b}"),
                            None => inner.concat(),
                        }
                    }
                }
            };
            s.push('[');
            s.push_str(&text);
            s.push(']');
        }
        s
    }

    /// The packed dimensions written after a type name (`fu_data_t [N-1:0]`),
    /// as slang prints them after the alias.
    fn alias_dims(&self, dt: NodeRef, scope: usize, env: &Env) -> String {
        let mut s = String::new();
        for d in self.children(dt) {
            if self.kind(d) == Dim {
                match self.dim_range(d, scope, env) {
                    Some((a, b)) => s.push_str(&format!("[{a}:{b}]")),
                    None => s.push_str(&self.toks_of(d).concat()),
                }
            }
        }
        s
    }

    /// `v_t [1:0]` with `v_t` a packed vector (`logic [3:0]`): slang makes
    /// one packed array of both and prints it without the alias,
    /// `logic[1:0][3:0]`. None when there are no dimensions after the
    /// alias or it is not such a vector (a struct, `int`, a scalar keep it).
    fn merged_vector(&self, dt: NodeRef, scope: usize, env: &Env, outer: &str, depth: usize) -> Option<String> {
        if outer.is_empty() {
            return None;
        }
        let inner = self.ts(dt, scope, env, true, depth + 1);
        let at = inner.find('[')?;
        let base = &inner[..at];
        let simple = matches!(
            base,
            "logic" | "bit" | "reg" | "logic signed" | "bit signed" | "reg signed" | "logic unsigned" | "bit unsigned"
        );
        (simple && !inner.contains('$')).then(|| format!("{base}{outer}{}", &inner[at..]))
    }

    /// A data type (or a name standing for one) as slang prints it.
    pub(crate) fn ts(&self, dt: NodeRef, scope: usize, env: &Env, canonical: bool, depth: usize) -> String {
        if depth > 24 {
            return String::new();
        }
        let toks = self.toks_of(dt);
        let own = self.own_tokens(dt);
        let Some(&first) = toks.first() else {
            return "logic".into();
        };
        let kw = own.iter().find(|t| INT_TYPES.contains(t)).copied();
        let implicit = kw.is_none() && matches!(first, "signed" | "unsigned" | "[");
        if kw.is_some() || implicit {
            let k = kw.unwrap_or("logic");
            let default_signed = int_width(k).1;
            let mut s = k.to_string();
            if own.contains(&"signed") && !default_signed {
                s.push_str(" signed");
            }
            if own.contains(&"unsigned") && default_signed {
                s.push_str(" unsigned");
            }
            for d in self.children(dt) {
                if self.kind(d) == Dim {
                    if self.dim_is_error(d, scope, env, false) {
                        return "<error>".into();
                    }
                    match self.dim_range(d, scope, env) {
                        Some((a, b)) => s.push_str(&format!("[{a}:{b}]")),
                        None => s.push_str(&self.toks_of(d).concat()),
                    }
                }
            }
            return s;
        }
        match first {
            "string" | "event" | "chandle" | "real" | "shortreal" | "realtime" | "void" => {
                return first.to_string();
            }
            "virtual" => {
                let names: Vec<&str> =
                    toks.iter().skip(1).filter(|t| **t != "interface").take_while(|t| **t != "#").copied().collect();
                let name = names.first().copied().unwrap_or("");
                let params = self.interface_params(name, dt, scope, env);
                return format!("{name}{params}{}", names[1.min(names.len())..].concat());
            }
            "enum" => return self.enum_string(dt, scope, env, None),
            "struct" | "union" => return self.struct_string(dt, scope, env, None),
            _ => {}
        }
        let path: Vec<&str> =
            toks.iter().take_while(|t| **t != "#" && **t != "[").filter(|t| **t != "::").copied().collect();
        // A type parameter bound by the specialization.
        if path.len() == 1 {
            if let Some(b) = env.get(first) {
                if canonical {
                    return self.ts(b.node, b.scope, &b.env, true, depth + 1);
                }
            }
        }
        let mut found = match path.len() {
            0 => None,
            1 => self.lookup(path[0], scope, depth + 1),
            _ => self.lookup_path(&path, scope, depth + 1),
        };
        // A specialization's identity sees typedefs local to a block.
        if found.is_none() && path.len() == 1 && self.keying.get() {
            found = self.block_typedef(path[0], dt).map(|t| Found::Typedef(t, scope));
        }
        match found {
            Some(Found::Class(c)) => {
                let args = self.bind_args(c, dt, scope, env);
                if self.keying.get() {
                    return self.class_key_at(c, &args, depth + 1);
                }
                self.class_string(c, &args, depth + 1)
            }
            Some(Found::Typedef(t, s)) => {
                let name = self.name_of(t).map(|n| n.0).unwrap_or_default();
                if !canonical {
                    let outer = self.alias_dims(dt, scope, env);
                    if let Some(v) = self.merged_vector(dt, scope, env, &outer, depth) {
                        return v;
                    }
                    return format!("{}{name}{outer}", self.scope_path(s));
                }
                let tenv = self.scope_env(s, env);
                let inner = self.child(t, DataType);
                let dims: Vec<NodeRef> = self.children(t).into_iter().filter(|c| self.kind(*c) == Dim).collect();
                let alias = format!("{}{name}", self.scope_path(s));
                let base = match inner {
                    Some(i) if dims.is_empty() => match self.own_tokens(i).first().copied() {
                        Some("enum") => self.enum_string(i, s, &tenv, Some(&alias)),
                        Some("struct" | "union") => self.struct_string(i, s, &tenv, Some(&alias)),
                        _ => self.ts(i, s, &tenv, true, depth + 1),
                    },
                    Some(i) => self.ts(i, s, &tenv, false, depth + 1),
                    None => "logic".into(),
                };
                if dims.is_empty() {
                    base
                } else {
                    format!("{base}${}", self.unpacked(&dims, s, &tenv))
                }
            }
            Some(Found::Param(default, s)) => {
                let name = path[path.len() - 1];
                if !canonical {
                    let outer = self.alias_dims(dt, scope, env);
                    if let Some(v) = self.merged_vector(dt, scope, env, &outer, depth) {
                        return v;
                    }
                    return format!("{}{name}{outer}", self.scope_path(s));
                }
                match env.get(name) {
                    Some(b) => self.ts(b.node, b.scope, &b.env, true, depth + 1),
                    None => default.map_or(String::new(), |d| {
                        let p = self.scopes[s].parent.unwrap_or(s);
                        self.ts(d, p, &Env::default(), true, depth + 1)
                    }),
                }
            }
            // No name before `#` or `[`: not a type this can name; give
            // it back as written.
            None if path.is_empty() => toks.concat(),
            None => {
                let name = if path.len() == 2 && path[0] == "std" { path[1] } else { path[0] };
                match name {
                    "mailbox" => {
                        let arg = self
                            .child(dt, ParamList)
                            .and_then(|pl| self.child(pl, ArgList))
                            .and_then(|al| self.children(al).into_iter().find(|a| self.kind(*a) == Arg))
                            .and_then(|a| self.children(a).into_iter().next());
                        match arg {
                            Some(a) => {
                                format!("mailbox#({})", self.ts(a, scope, env, false, depth + 1))
                            }
                            None => "mailbox#(untyped)".into(),
                        }
                    }
                    "semaphore" | "process" => name.to_string(),
                    _ if self.undeclared_type(dt) => "<error>".into(),
                    _ => self.tree(dt).bare_text(dt.node),
                }
            }
        }
    }

    /// `#(W=8)` for an interface with parameters: the vif's arguments or
    /// the defaults.
    fn interface_params(&self, name: &str, dt: NodeRef, scope: usize, env: &Env) -> String {
        let Some(&decl) = self.interfaces.get(name) else {
            return String::new();
        };
        let Some(pl) = self.child(decl, Header).and_then(|h| self.child(h, ParamList)) else {
            return String::new();
        };
        let args: Vec<NodeRef> = self
            .child(dt, ParamList)
            .and_then(|p| self.child(p, ArgList))
            .map(|al| self.children(al).into_iter().filter(|a| self.kind(*a) == Arg).collect())
            .unwrap_or_default();
        let mut out = Vec::new();
        let mut pos = 0;
        for p in self.children(pl) {
            if self.kind(p) != ParamDecl || self.own_tokens(p).contains(&"type") {
                continue;
            }
            let Some(d) = self.child(p, Declarator) else {
                continue;
            };
            let Some((pname, _)) = self.name_of(d) else {
                continue;
            };
            let default = self.children(d).into_iter().find(|c| body_is_expr(self.kind(*c)));
            let actual = args
                .iter()
                .find(|a| self.own_tokens(**a).get(1) == Some(&pname.as_str()))
                .or_else(|| args.get(pos).filter(|a| self.own_tokens(**a).first() != Some(&".")))
                .and_then(|a| self.children(*a).into_iter().find(|c| body_is_expr(self.kind(*c))));
            pos += 1;
            let v = match actual {
                Some(a) => self.eval(a, scope, env, 0),
                None => default.and_then(|e| self.eval(e, scope, &Env::default(), 0)),
            };
            let v = match (v, self.child(p, DataType).and_then(|t| self.int_shape(t, scope, env, 0))) {
                (Some(v), Some((w, s))) => Some(v.cast(w, s)),
                (v, _) => v,
            };
            out.push(format!("{pname}={}", v.map_or(String::new(), |v| v.show())));
        }
        if out.is_empty() {
            String::new()
        } else {
            format!("#({})", out.join(","))
        }
    }

    /// The environment typedefs of class scope `s` are read in: the one
    /// given if it is that class's, else its representative.
    pub(crate) fn scope_env(&self, s: usize, env: &Env) -> Env {
        match self.scopes[s].kind {
            ScopeKind::Class(c) => {
                if env.is_empty() {
                    self.rep_env.get(&c).cloned().unwrap_or_default()
                } else {
                    env.clone()
                }
            }
            _ => Env::default(),
        }
    }

    /// A specialization's identity: its name and its arguments with every
    /// typedef and parameter resolved (what slang compares, unlike the
    /// printed form, where `C::T` stands for whatever T is bound to).
    pub(crate) fn class_key(&self, c: usize, env: &Env) -> String {
        let was = self.keying.replace(true);
        let k = self.class_key_at(c, env, 0);
        self.keying.set(was);
        k
    }

    fn class_key_at(&self, c: usize, env: &Env, depth: usize) -> String {
        let def = &self.classes[c];
        let mut s = format!("{}@{}:{}", def.name, def.scope, def.params.len());
        if !def.generic || depth > 24 {
            return s;
        }
        s.push_str("#(");
        let env = &self.complete_env(c, env);
        for (k, (name, is_type, default)) in def.params.iter().enumerate() {
            if k > 0 {
                s.push(',');
            }
            if *is_type {
                let printed = match env.get(name) {
                    Some(b) => self.ts(b.node, b.scope, &b.env, true, depth + 1),
                    None => default.map_or(String::new(), |d| self.ts(d, def.scope, env, true, depth + 1)),
                };
                s.push_str(&printed);
            } else {
                if let Some(b) = env.get(name) {
                    if let [lit] = self.toks_of(b.node).as_slice() {
                        if lit.starts_with('"') {
                            s.push_str(lit);
                            continue;
                        }
                    }
                }
                match self.const_named(name, def.scope, env, depth + 1) {
                    Some(v) => {
                        let shown = v.show();
                        // A string parameter holds a string, whatever
                        // integral form the value was computed in.
                        let is_string = self.scopes[def.scope]
                            .consts
                            .get(name)
                            .and_then(|(dt, _)| *dt)
                            .is_some_and(|dt| self.own_tokens(dt).first() == Some(&"string"));
                        match is_string.then(|| string_of(&v)).flatten() {
                            Some(lit) => s.push_str(&lit),
                            None => s.push_str(&shown),
                        }
                    }
                    None => {
                        if let Some(d) = default {
                            s.push_str(&self.tree(*d).bare_text(d.node));
                        }
                    }
                }
            }
        }
        s.push(')');
        s
    }

    /// `name#(args)` for a specialization, `name` otherwise.
    pub(crate) fn class_string(&self, c: usize, env: &Env, depth: usize) -> String {
        let def = &self.classes[c];
        let mut s = def.name.clone();
        if !def.generic || depth > 24 {
            return s;
        }
        s.push_str("#(");
        for (k, (name, is_type, default)) in def.params.iter().enumerate() {
            if k > 0 {
                s.push(',');
            }
            if *is_type {
                let printed = match env.get(name) {
                    Some(b) => {
                        // `C#(T)` inside C itself names the specialization
                        // being printed: its T stands for what T is bound to.
                        let mut b = b.clone();
                        let toks = self.toks_of(b.node);
                        if let [n] = toks.as_slice() {
                            if *default == Some(b.node) && def.params.iter().any(|p| p.0 == *n) {
                                // A default naming an earlier parameter
                                // (`RSP = REQ`) prints as that parameter.
                                s.push_str(&format!("{}{n}", self.scope_path(def.scope)));
                                continue;
                            }
                            if let Some(next) = b.env.get(*n).cloned() {
                                b = next;
                            }
                        }
                        self.ts(b.node, b.scope, &b.env, false, depth + 1)
                    }
                    None => default.map_or(String::new(), |d| match self.toks_of(d).as_slice() {
                        // `RSP = REQ` prints as that parameter.
                        [n] if def.params.iter().any(|p| p.0 == *n) => {
                            format!("{}{n}", self.scope_path(def.scope))
                        }
                        _ => self.ts(d, def.scope, env, false, depth + 1),
                    }),
                };
                s.push_str(&printed);
            } else {
                // A string argument prints as written.
                if let Some(b) = env.get(name) {
                    if let [lit] = self.toks_of(b.node).as_slice() {
                        if lit.starts_with('"') {
                            s.push_str(lit);
                            continue;
                        }
                    }
                }
                let v = self.const_named(name, def.scope, env, depth + 1);
                match v {
                    Some(v) => s.push_str(&v.show()),
                    None => {
                        if let Some(d) = default {
                            s.push_str(&self.tree(*d).bare_text(d.node));
                        }
                    }
                }
            }
        }
        s.push(')');
        s
    }

    /// The value of a parameter, specparam or enum value, as slang prints it.
    pub(crate) fn symbol_value(&self, y: &crate::scopes::Symbol, scope: usize, env: &Env) -> Option<String> {
        use crate::scopes::SymKind as K;
        let t = &self.trees[y.tree];
        let r = |n: NodeId| NodeRef { tree: y.tree, node: n };
        if y.kind == K::EnumValue {
            let dt = y.dtype?;
            // An all-x value of a four-state enum prints as x.
            let init = Some(y.node).and_then(|d| t.children(d).find(|&c| !matches!(t.kind(c), Name | Dim)));
            if let Some(i) = init {
                let mut sub = vec![i];
                let mut k = 0;
                while k < sub.len() {
                    sub.extend(t.children(sub[k]));
                    k += 1;
                }
                let based: Vec<String> = sub
                    .into_iter()
                    .filter(|&n| t.kind(n) == Literal)
                    .map(|n| self.toks_of(r(n)).concat())
                    .filter(|x| x.contains('\''))
                    .collect();
                if !based.is_empty() && based.iter().all(|x| all_x(x)) && !self.two_state(r(dt)) {
                    let (w, sg) = self
                        .child(r(dt), DataType)
                        .and_then(|b| self.int_shape(b, scope, env, 0))
                        .unwrap_or((32, true));
                    return Some(format!("{w}'{}dx", if sg { "s" } else { "" }));
                }
            }
            let vals = self.enum_values(r(dt), scope, env, 0);
            return vals.iter().find(|(n, _)| *n == y.name).map(|(_, v)| v.show());
        }
        let d = if y.kind == K::Specparam { Some(y.node) } else { y.item };
        let v = d.and_then(|d| t.children(d).find(|&c| !matches!(t.kind(c), Name | Dim)));
        let Some(v) = v else {
            return Some("<unset>".into());
        };
        self.value_text(y.dtype.map(r), r(v), scope, env)
    }

    /// A time literal (`300ps`) in the time unit of its design unit: its
    /// `timeunit`, else the `timescale before it, else 1ns.
    pub(crate) fn time_value(&self, lit_node: NodeRef, lit: &str) -> Option<f64> {
        let t = self.tree(lit_node);
        let split = lit.find(|c: char| c.is_ascii_alphabetic())?;
        let num: f64 = lit[..split].replace('_', "").parse().ok()?;
        let exp = time_exp(&lit[split..])?;
        let first = t.token_bounds(lit_node.node)?.0;
        // The top-level item that holds the literal.
        let unit_node = t.children(t.root).find(|&c| t.token_bounds(c).is_some_and(|(a, b)| a <= first && first <= b));
        let mut unit = None;
        if let Some(u) = unit_node {
            let mut stack = vec![u];
            while let Some(n) = stack.pop() {
                stack.extend(t.children(n));
                if t.kind(n) == TimeunitDecl {
                    let toks = self.toks_of(NodeRef { tree: lit_node.tree, node: n });
                    if toks.first() == Some(&"timeunit") {
                        unit = toks.get(1).and_then(|x| {
                            let sp = x.find(|c: char| c.is_ascii_alphabetic())?;
                            let mag = match &x[..sp] {
                                "1" => 0,
                                "10" => 1,
                                "100" => 2,
                                _ => return None,
                            };
                            Some(time_exp(&x[sp..])? + mag)
                        });
                    }
                }
            }
        }
        let at = unit_node.and_then(|u| t.token_bounds(u)).map_or(first, |b| b.0);
        let unit = unit.unwrap_or_else(|| {
            t.timescales.iter().take_while(|(i, _)| *i <= at).last().map_or(DEFAULT_TIME_UNIT, |x| x.1)
        });
        Some(num * 10f64.powi(exp - unit))
    }

    /// A constant's value as slang prints it: `v` in a declaration of type
    /// `dtype` (`None`: implicit) in `scope`.
    pub(crate) fn value_text(&self, dtype: Option<NodeRef>, v: NodeRef, scope: usize, env: &Env) -> Option<String> {
        let t = &self.trees[v.tree];
        let r = |n: NodeId| NodeRef { tree: v.tree, node: n };
        let v = v.node;
        let toks = self.toks_of(r(v));
        let kw = dtype.and_then(|d| self.own_tokens(d).first().copied());
        if let [lit] = toks.as_slice() {
            if lit.starts_with('"') {
                if kw == Some("string") {
                    return Some(lit.to_string());
                }
                // A string literal is an integral value, 8 bits a character.
                let body = &lit[1..lit.len() - 1];
                let mut val = Int::from(0);
                for c in body.bytes() {
                    val = val.shl(8).or(&Int::from(c as i128));
                }
                let w = (body.len().max(1) * 8) as u32;
                let c = CVal::new(w, false, val);
                let c = match dtype.and_then(|d| self.int_shape(d, scope, env, 0)) {
                    Some((w, s)) => c.cast(w, s),
                    None => c,
                };
                return Some(c.show());
            }
            let tk = t.token_bounds(v).map(|(i, _)| t.tokens[i as usize].kind);
            let real = match tk {
                Some(sv_syntax::TokenKind::TimeLiteral) => self.time_value(r(v), lit),
                Some(sv_syntax::TokenKind::RealLiteral) => lit.replace('_', "").parse::<f64>().ok(),
                _ => None,
            };
            if let Some(f) = real {
                let shape = dtype.and_then(|d| self.int_shape(d, scope, env, 0));
                return match shape {
                    // A real into an integral type rounds half away from zero.
                    Some((w, s)) if !matches!(kw, Some("real" | "shortreal" | "realtime")) => {
                        let i = f.round();
                        (i.abs() < 1e38).then(|| CVal::new(w, s, i as i128).show())
                    }
                    _ => Some(format!("{f}")),
                };
            }
            if matches!(kw, Some("real" | "shortreal" | "realtime")) {
                return lit.parse::<f64>().ok().map(|f| format!("{f}"));
            }
        }
        let shape = dtype.and_then(|d| self.int_shape(d, scope, env, 0));
        match toks.as_slice() {
            ["$"] => return Some("$".into()),
            // `'0`, `'1`: every bit of the target.
            [fill @ ("'0" | "'1")] => {
                let (w, s) = shape?;
                let v: i128 = if *fill == "'1" { -1 } else { 0 };
                return Some(CVal::new(w, s, v).show());
            }
            _ => {}
        }
        if t.kind(v) == AssignPattern {
            // A packed struct or array is one integer.
            if let Some(d) = dtype.filter(|_| !self.unpacked_decl(r(v))) {
                if let Some(c) = self.eval_typed(r(v), packed::Ty::of(d, scope), scope, env, 0) {
                    if self.width(packed::Ty::of(d, scope), env, 0).is_some() && self.packed_aggregate(d, scope, env) {
                        return Some(c.show());
                    }
                }
            }
            return self.aggregate_value(r(v), dtype, shape, scope, env);
        }
        // A constant unpacked array named by another (`= pkg::Rst`).
        if matches!(t.kind(v), NameExpr | ScopedExpr) && self.unpacked_decl(r(v)) {
            if let Some((ddt, dv, ds)) = self.const_decl(r(v), scope) {
                if self.unpacked_decl(dv) {
                    return self.value_text(ddt, dv, ds, &Env::default());
                }
            }
        }
        let two_state = dtype.is_some_and(|d| self.two_state(d));
        self.invalid.set(false);
        let Some(c) = self.eval_xz(r(v), scope, env, two_state) else {
            return self.invalid.get().then(|| "<unset>".into());
        };
        let c = match shape {
            Some((w, s)) if c.w < w => {
                // Sized by the target (IEEE 11.6).
                self.eval_ctx(r(v), w, s, scope, env, 0).unwrap_or_else(|| c.cast(w, s))
            }
            Some((w, s)) => c.cast(w, s),
            None => c,
        };
        Some(if two_state { c.two_state() } else { c }.show())
    }

    /// `eval`, reading x and z digits as 0 only for a two-state target.
    pub(crate) fn eval_xz(&self, v: NodeRef, scope: usize, env: &Env, two_state: bool) -> Option<CVal> {
        let c = self.eval(v, scope, env, 0)?;
        Some(if two_state { c.two_state() } else { c })
    }

    /// Whether a data type is two-state (`bit`, `int` ... and enums of them).
    pub(crate) fn two_state(&self, dt: NodeRef) -> bool {
        let own = self.own_tokens(dt);
        match own.first().copied() {
            Some("bit" | "int" | "byte" | "shortint" | "longint") => true,
            Some("enum") => self.child(dt, DataType).is_none_or(|b| self.two_state(b)),
            _ => false,
        }
    }

    /// A constant unpacked array, `'{a, b, c}`, as slang prints it:
    /// `[a,b,c]`, each element in the element type.
    /// The declaration of a constant named by `e` (`X`, `pkg::X`): its
    /// type, value and scope.
    fn const_decl(&self, e: NodeRef, scope: usize) -> Option<(Option<NodeRef>, NodeRef, usize)> {
        let toks = self.toks_of(e);
        let (name, mut cur) = match toks.as_slice() {
            [n] => (*n, Some(scope)),
            [p, "::", n] => (*n, Some(*self.packages.get(*p)?)),
            _ => return None,
        };
        while let Some(s) = cur {
            if let Some(&(dt, val)) = self.scopes[s].consts.get(name) {
                return Some((dt, val?, s));
            }
            for (pkg, item) in &self.scopes[s].imports {
                if item.as_deref().is_none_or(|i| i == name) {
                    if let Some(&p) = self.packages.get(pkg) {
                        if let Some(&(dt, val)) = self.scopes[p].consts.get(name) {
                            return Some((dt, val?, p));
                        }
                    }
                }
            }
            cur = self.scopes[s].parent;
        }
        None
    }

    fn aggregate_value(
        &self,
        v: NodeRef,
        dtype: Option<NodeRef>,
        shape: Option<(u32, bool)>,
        scope: usize,
        env: &Env,
    ) -> Option<String> {
        let own = self.own_tokens(v);
        // Keyed and default patterns are not plain lists.
        if own.contains(&":") {
            return None;
        }
        let mut out = Vec::new();
        for e in self.children(v).into_iter().filter(|c| body_is_expr(self.kind(*c)) || self.kind(*c) == AssignPattern)
        {
            // Elements of the declared element type (a packed struct too).
            if let Some(d) = dtype.filter(|d| self.packed_aggregate(*d, scope, env) || self.kind(e) == AssignPattern) {
                out.push(self.eval_typed(e, packed::Ty::of(d, scope), scope, env, 0)?.show());
                continue;
            }
            if self.kind(e) == AssignPattern {
                return None;
            }
            let c = self.eval(e, scope, env, 0)?;
            let c = if dtype.is_some_and(|d| self.two_state(d)) { c.two_state() } else { c };
            out.push(match shape {
                Some((w, s)) => c.cast(w, s).show(),
                None => c.show(),
            });
        }
        Some(format!("[{}]", out.join(",")))
    }

    /// The type of a parameter declared without one: its value's.
    pub(crate) fn value_type(&self, v: NodeRef, scope: usize, env: &Env) -> String {
        let toks = self.toks_of(v);
        if let [lit] = toks.as_slice() {
            if lit.starts_with('"') {
                let n = lit.len().saturating_sub(2).max(1) * 8;
                return format!("logic[{}:0]", n - 1);
            }
            if lit.contains('.') || (lit.contains(['e', 'E']) && !lit.contains('\'') && lit.parse::<f64>().is_ok()) {
                return "real".into();
            }
        }
        match self.eval(v, scope, env, 0) {
            Some(c) => {
                let sign = if c.s { " signed" } else { "" };
                format!("logic{sign}[{}:0]", c.w.max(1) - 1)
            }
            None => "logic".into(),
        }
    }

    /// The type a typedef names, as slang prints its target: an enum or
    /// struct carries the typedef's name, anything else is printed as written.
    pub(crate) fn typedef_target(&self, t: NodeRef, s: usize, env: &Env) -> String {
        let name = self.name_of(t).map(|n| n.0).unwrap_or_default();
        let alias = format!("{}{name}", self.scope_path(s));
        let dims: Vec<NodeRef> = self.children(t).into_iter().filter(|c| self.kind(*c) == Dim).collect();
        let Some(i) = self.child(t, DataType) else {
            return "logic".into();
        };
        let base = match self.own_tokens(i).first().copied() {
            Some("enum") if dims.is_empty() => self.enum_string(i, s, env, Some(&alias)),
            Some("struct" | "union") if dims.is_empty() => self.struct_string(i, s, env, Some(&alias)),
            _ => self.ts(i, s, env, false, 0),
        };
        if dims.is_empty() {
            base
        } else {
            format!("{base}${}", self.unpacked(&dims, s, env))
        }
    }

    fn enum_string(&self, dt: NodeRef, scope: usize, env: &Env, alias: Option<&str>) -> String {
        if !env.is_empty() {
            return self.enum_string_of(dt, scope, env, alias);
        }
        let key = (dt.tree, dt.node, scope, alias.map(str::to_string));
        if let Some(v) = self.enum_strings.borrow().get(&key) {
            return v.clone();
        }
        let v = self.enum_string_of(dt, scope, env, alias);
        self.enum_strings.borrow_mut().insert(key, v.clone());
        v
    }

    fn enum_string_of(&self, dt: NodeRef, scope: usize, env: &Env, alias: Option<&str>) -> String {
        let vals = self.enum_values(dt, scope, env, 0);
        let body: Vec<String> = vals.iter().map(|(n, v)| format!("{n}={}", v.show_dec())).collect();
        let name = match alias {
            Some(a) => a.to_string(),
            None => format!("{}e$1", self.scope_path(scope)),
        };
        format!("enum{{{}}}{name}", body.join(","))
    }

    fn struct_string(&self, dt: NodeRef, scope: usize, env: &Env, _alias: Option<&str>) -> String {
        let own = self.own_tokens(dt);
        let kind = own[0];
        let mut s = kind.to_string();
        if own.contains(&"packed") {
            s.push_str(" packed");
        }
        if own.contains(&"tagged") {
            s.push_str(" tagged");
        }
        if own.contains(&"signed") {
            s.push_str(" signed");
        }
        s.push('{');
        for m in self.children(dt) {
            if self.kind(m) != DataDecl {
                continue;
            }
            let mdt = self.child(m, DataType);
            for d in self.children(m) {
                if self.kind(d) == Declarator {
                    if let Some((n, _)) = self.name_of(d) {
                        let t = self.type_string(mdt, Some(d), scope, env, false);
                        s.push_str(&format!("{t} {n};"));
                    }
                }
            }
        }
        s.push('}');
        // slang names anonymous structs `s$N` / `u$N` in their scope, by
        // the order it created them.
        let letter = if kind == "union" { 'u' } else { 's' };
        let n = self.anon_index(dt, scope, kind);
        format!("{s}{}{letter}${n}", self.scope_path(scope))
    }

    /// 1-based position of a struct or union type among those of the same
    /// kind declared directly in its scope's typedefs, in source order.
    fn anon_index(&self, dt: NodeRef, scope: usize, kind: &str) -> usize {
        let mut all: Vec<NodeRef> = self.scopes[scope]
            .typedefs
            .values()
            .filter_map(|&t| self.child(t, DataType))
            .filter(|d| self.own_tokens(*d).first() == Some(&kind))
            .collect();
        all.sort_by_key(|d| (d.tree, self.tokens(*d).first().copied().unwrap_or(0)));
        all.iter().position(|d| *d == dt).map_or(1, |p| p + 1)
    }
}

pub(crate) fn is_expr(k: SyntaxKind) -> bool {
    body_is_expr(k)
}

/// A string literal as an integral value: 8 bits a character, the first
/// one highest (IEEE 1800 5.9).
fn string_value(lit: &str) -> Option<CVal> {
    let body = lit.strip_prefix('"')?.strip_suffix('"')?;
    let mut bytes = Vec::new();
    let mut it = body.bytes();
    while let Some(c) = it.next() {
        if c != b'\\' {
            bytes.push(c);
            continue;
        }
        bytes.push(match it.next()? {
            b'n' => b'\n',
            b't' => b'\t',
            b'\\' => b'\\',
            b'"' => b'"',
            b'v' => 0x0b,
            b'f' => 0x0c,
            b'a' => 0x07,
            _ => return None,
        });
    }
    let mut v = Int::from(0);
    for c in &bytes {
        v = v.shl(8).or(&Int::from(*c as i128));
    }
    Some(CVal::new((bytes.len().max(1) * 8) as u32, false, v))
}

/// Binary operators whose operands are sized to each other.
fn is_context_op(op: &str) -> bool {
    matches!(
        op,
        "+" | "-"
            | "*"
            | "/"
            | "%"
            | "&"
            | "|"
            | "^"
            | "~^"
            | "^~"
            | "=="
            | "!="
            | "==="
            | "!=="
            | "<"
            | "<="
            | ">"
            | ">="
    )
}

pub(crate) fn binary_op(op: &str, a: CVal, b: CVal) -> Option<CVal> {
    binary(op, a, b)
}

/// Up to three operands of an expression (see [`Builder::eval`]).
struct Kids([Option<NodeRef>; 3]);

impl Kids {
    fn first(&self) -> Option<&NodeRef> {
        self.0[0].as_ref()
    }

    fn get(&self, i: usize) -> Option<&NodeRef> {
        self.0.get(i)?.as_ref()
    }
}

fn body_is_expr(k: SyntaxKind) -> bool {
    matches!(
        k,
        BinaryExpr
            | PrefixExpr
            | PostfixExpr
            | CondExpr
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

/// A based literal whose digits are all x.
fn all_x(t: &str) -> bool {
    match t.find('\'') {
        Some(i) => t[i + 1..]
            .trim_start_matches(['s', 'S'])
            .get(1..)
            .is_some_and(|d| !d.is_empty() && d.chars().all(|c| matches!(c, 'x' | 'X' | '_'))),
        None => false,
    }
}

/// `4'hA`, `12`, `'1`, `2'sb11`, `4'b10x1` as a value.
fn literal(toks: &[&str]) -> Option<CVal> {
    let joined: String = toks.concat().replace('_', "");
    if let Some(i) = joined.find('\'') {
        let (size, rest) = joined.split_at(i);
        let rest = &rest[1..];
        let (signed, rest) = match rest.strip_prefix(['s', 'S']) {
            Some(r) => (true, r),
            None => (false, rest),
        };
        let mut chars = rest.chars();
        let b = chars.next()?.to_ascii_lowercase();
        let digits: String = chars.collect();
        let radix = match b {
            'b' => 2,
            'o' => 8,
            'd' => 10,
            'h' => 16,
            '0' | '1' if size.is_empty() => {
                // `'0` / `'1`: all bits, here taken as one.
                return Some(CVal::bool(b == '1'));
            }
            'x' if size.is_empty() && rest.len() == 1 => return Some(CVal::all_x(1, false)),
            'z' if size.is_empty() && rest.len() == 1 => {
                return Some(CVal::four(1, false, Int::from(1), Int::from(1)));
            }
            _ => return None,
        };
        let w = if size.is_empty() { 32 } else { size.parse().ok()? };
        let digits = digits.to_ascii_lowercase();
        if !digits.contains(['x', 'z', '?']) {
            let v = Int::parse_radix(&digits, radix)?;
            return Some(CVal::new(w, signed, v));
        }
        return four_state_literal(&digits, radix, w, signed);
    }
    if joined.is_empty() || !joined.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // An unsized decimal is a 32-bit signed integer (wider if it must be).
    let v = Int::parse_radix(&joined, 10)?;
    let w = (v.active_bits() + 1).max(32);
    Some(CVal::new(w, true, v))
}

/// Digits with x, z or ? (IEEE 5.7.1): each such digit stands for all its
/// bits, and a leading one fills the bits above.
fn four_state_literal(digits: &str, radix: u32, w: u32, signed: bool) -> Option<CVal> {
    if radix == 10 {
        // Only a single x or z digit is legal in decimal.
        return match digits {
            "x" => Some(CVal::all_x(w, signed)),
            "z" | "?" => Some(CVal::four(w, signed, Int::from(-1), Int::from(-1))),
            _ => None,
        };
    }
    let per = match radix {
        2 => 1,
        8 => 3,
        _ => 4,
    };
    let (mut v, mut u) = (Int::from(0), Int::from(0));
    let ones = Int::from((1i128 << per) - 1);
    for c in digits.chars() {
        v = v.shl(per);
        u = u.shl(per);
        match c {
            'x' => u = u.or(&ones),
            'z' | '?' => {
                u = u.or(&ones);
                v = v.or(&ones);
            }
            _ => v = v.or(&Int::from(c.to_digit(radix)? as i128)),
        }
    }
    let n = digits.len() as u32 * per;
    if n < w {
        let top = digits.chars().next()?;
        if matches!(top, 'x' | 'z' | '?') {
            let fill = Int::from(-1).shl(n).trunc(w, false);
            u = u.or(&fill);
            if top != 'x' {
                v = v.or(&fill);
            }
        }
    }
    Some(CVal::four(w, signed, v.trunc(w, false), u))
}

/// Extracts the unsigned shift count from `b` per IEEE 1800 §11.4.10, saturating to `lhs_w + 1`.
fn shift_amount(b: &CVal, lhs_w: u32) -> u32 {
    let ub = b.bits_v();
    let limit = u128::from(lhs_w) + 1;
    match ub.to_i128() {
        Some(v) if v >= 0 && (v as u128) <= limit => v as u32,
        _ => lhs_w.saturating_add(1),
    }
}

fn binary(op: &str, a: CVal, b: CVal) -> Option<CVal> {
    if !a.known() || !b.known() {
        return binary_unknown(op, a, b);
    }
    let w = a.w.max(b.w);
    let s = a.s && b.s;
    let (x, y) = (a.cast(w, s).v, b.cast(w, s).v);
    if matches!(op, "/" | "%") && y.is_zero() {
        return Some(CVal::all_x(w, s));
    }
    let r = match op {
        "+" => x.add(&y),
        "-" => x.sub(&y),
        "*" => x.mul(&y),
        "/" => x.divrem(&y)?.0,
        "%" => x.divrem(&y)?.1,
        "**" => {
            // IEEE 1800 Table 11-4: result has the left operand's width and signedness.
            if b.s && b.v.is_neg() {
                let av = if a.s { a.v.clone() } else { a.bits_v() };
                if av.is_zero() {
                    return Some(CVal::all_x(a.w, a.s));
                }
                if av == Int::from(1) {
                    return Some(CVal::new(a.w, a.s, 1));
                }
                if a.s && av == Int::from(-1) {
                    let r = if b.v.bit(0) { -1 } else { 1 };
                    return Some(CVal::new(a.w, a.s, r));
                }
                return Some(CVal::new(a.w, a.s, 0));
            }
            let exp = b.bits_v();
            if exp.is_zero() {
                return Some(CVal::new(a.w, a.s, 1));
            }
            let w_mod = a.w.max(1);
            if w_mod > 4096 && exp.active_bits() > 16 {
                return None;
            }
            let mut acc = Int::from(1).trunc(w_mod, a.s);
            let mut base = a.v.trunc(w_mod, a.s);
            let bits = exp.active_bits();
            for i in 0..bits {
                if exp.bit(i) {
                    acc = acc.mul(&base).trunc(w_mod, a.s);
                }
                if i + 1 < bits {
                    base = base.mul(&base).trunc(w_mod, a.s);
                }
            }
            return Some(CVal::new(a.w, a.s, acc));
        }
        "&" => x.and(&y),
        "|" => x.or(&y),
        "^" => x.xor(&y),
        "~^" | "^~" => x.xor(&y).not(),
        "<<" | "<<<" => {
            let n = shift_amount(&b, a.w);
            return Some(CVal { v: a.v.shl(n), ..a }.norm());
        }
        ">>" => {
            let n = shift_amount(&b, a.w);
            return Some(CVal { v: a.bits_v().shr(n), ..a }.norm());
        }
        ">>>" => {
            let n = shift_amount(&b, a.w);
            let rv = if a.s { a.v.shr(n) } else { a.bits_v().shr(n) };
            return Some(CVal { v: rv, ..a }.norm());
        }
        "==" | "===" => return Some(CVal::bool(x == y)),
        "!=" | "!==" => return Some(CVal::bool(x != y)),
        "<" => return Some(CVal::bool(x < y)),
        "<=" => return Some(CVal::bool(x <= y)),
        ">" => return Some(CVal::bool(x > y)),
        ">=" => return Some(CVal::bool(x >= y)),
        "&&" => return Some(CVal::bool(a.is_true() && b.is_true())),
        "||" => return Some(CVal::bool(a.is_true() || b.is_true())),
        "->" => return Some(CVal::bool(!a.is_true() || b.is_true())),
        "<->" => return Some(CVal::bool(a.is_true() == b.is_true())),
        _ => return None,
    };
    Some(CVal::new(w, s, r))
}

/// A range bound as slang reads it: a known value as a 32-bit int
/// (`W - 1` with an unsigned `W` of 0 is -1).
fn dim_bound(c: CVal) -> Option<i128> {
    if !c.known() {
        return None;
    }
    Some(if c.w <= 32 { c.cast(32, true).i() } else { c.i() })
}

/// `-a`, `+a`, `~a`, `!a` with four-state bits.
pub(crate) fn unary(op: &str, a: CVal) -> Option<CVal> {
    Some(match op {
        "-" if !a.known() => CVal::all_x(a.w, a.s),
        "-" => CVal::new(a.w, a.s, a.v.neg()),
        "+" => a,
        "~" => {
            let v = a.bits_v().not().and(&a.u.not());
            CVal::four(a.w, a.s, v, a.u.clone())
        }
        "!" => a.truth().map_or_else(|| CVal::all_x(1, false), |t| CVal::bool(!t)),
        _ => return None,
    })
}

/// `binary` with an operand that has unknown bits (IEEE 11.4): bitwise
/// operators work bit by bit, the rest give x.
fn binary_unknown(op: &str, a: CVal, b: CVal) -> Option<CVal> {
    let w = a.w.max(b.w);
    let s = a.s && b.s;
    let (ca, cb) = (a.cast(w, s), b.cast(w, s));
    let (au, bu) = (&ca.u, &cb.u);
    let (a1, b1) = (ca.ones(), cb.ones());
    // Known zero bits.
    let a0 = ca.bits_v().or(au).not().trunc(w, false);
    let b0 = cb.bits_v().or(bu).not().trunc(w, false);
    let x1 = |op: &str| -> CVal { CVal::all_x(if op.is_empty() { 1 } else { w }, s && !op.is_empty()) };
    Some(match op {
        "&" => {
            let zero = a0.or(&b0);
            let u = au.or(bu).and(&zero.not());
            CVal::four(w, s, a1.and(&b1), u)
        }
        "|" => {
            let one = a1.or(&b1);
            let u = au.or(bu).and(&one.not());
            CVal::four(w, s, one, u)
        }
        "^" | "~^" | "^~" => {
            let u = au.or(bu);
            let mut v = ca.bits_v().xor(&cb.bits_v());
            if op != "^" {
                v = v.not();
            }
            CVal::four(w, s, v.and(&u.not()), u)
        }
        "===" | "!==" => {
            let same = ca.bits_v() == cb.bits_v() && ca.u == cb.u;
            CVal::bool(same == (op == "==="))
        }
        "&&" | "||" => {
            let (ta, tb) = (a.truth(), b.truth());
            let r = if op == "&&" {
                match (ta, tb) {
                    (Some(false), _) | (_, Some(false)) => Some(false),
                    (Some(true), Some(true)) => Some(true),
                    _ => None,
                }
            } else {
                match (ta, tb) {
                    (Some(true), _) | (_, Some(true)) => Some(true),
                    (Some(false), Some(false)) => Some(false),
                    _ => None,
                }
            };
            r.map_or_else(|| x1(""), CVal::bool)
        }
        "->" => match (a.truth(), b.truth()) {
            (Some(false), _) | (_, Some(true)) => CVal::bool(true),
            (Some(true), Some(false)) => CVal::bool(false),
            _ => x1(""),
        },
        "<->" => match (a.truth(), b.truth()) {
            (Some(ta), Some(tb)) => CVal::bool(ta == tb),
            _ => x1(""),
        },
        "==" | "!=" | "<" | "<=" | ">" | ">=" => x1(""),
        "<<" | "<<<" | ">>" | ">>>" if b.known() => {
            let n = shift_amount(&b, a.w);
            let (v, u) = if op.starts_with("<<") {
                (a.v.shl(n), a.u.shl(n))
            } else if op == ">>>" && a.s {
                (a.v.shr(n), a.u.trunc(a.w.max(1), true).shr(n))
            } else {
                (a.bits_v().shr(n), a.u.shr(n))
            };
            CVal::four(a.w, a.s, v, u)
        }
        "**" | "<<" | "<<<" | ">>" | ">>>" => CVal::all_x(a.w, a.s),
        "+" | "-" | "*" | "/" | "%" => x1("w"),
        _ => return None,
    })
}

/// 1ns, the time unit without a `timescale, as a power of ten femtoseconds.
const DEFAULT_TIME_UNIT: i32 = 6;

/// A time unit suffix as a power of ten femtoseconds.
fn time_exp(u: &str) -> Option<i32> {
    Some(match u {
        "fs" => 0,
        "ps" => 3,
        "ns" => 6,
        "us" => 9,
        "ms" => 12,
        "s" => 15,
        _ => return None,
    })
}

/// The string literal a value's bytes spell (NULs dropped).
fn string_of(v: &CVal) -> Option<String> {
    if !v.known() {
        return None;
    }
    let bits = v.bits_v();
    let n = v.w.div_ceil(8);
    let mut out = String::from("\"");
    for k in (0..n).rev() {
        let b = bits.bits(k * 8, 8).to_i128()? as u8;
        if b != 0 {
            out.push(b as char);
        }
    }
    out.push('"');
    Some(out)
}
