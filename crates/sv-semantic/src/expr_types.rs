//! IEEE 1800-2023 Clause 11.6 & 11.8 expression type rules (self-determined
//! and context-determined shapes, literal typing, and system-function return
//! types), shared by `sv-semantic::expr` (`ExprBinder`) and `dv-lint` (`--exprs`).

use crate::oracle::TypeInfo;

/// Arithmetic and bitwise binary operators whose width/sign propagate context (`+`, `-`, `*`, `/`, `%`, `&`, `|`, `^`, `^~`, `~^`).
pub const ARITH: &[&str] = &["+", "-", "*", "/", "%", "&", "|", "^", "^~", "~^"];
/// Relational and equality operators producing a 1-bit unsigned result (`==`, `!=`, `===`, `!==`, `==?`, `!=?`, `<`, `<=`, `>`, `>=`).
pub const COMPARE: &[&str] = &["==", "!=", "===", "!==", "==?", "!=?", "<", "<=", ">", ">="];
/// Logical connectives producing a 1-bit unsigned result (`&&`, `||`, `->`, `<->`).
pub const LOGICAL: &[&str] = &["&&", "||", "->", "<->"];
/// Shift and power operators whose left operand propagates context (`<<`, `>>`, `<<<`, `>>>`, `**`).
pub const SHIFT: &[&str] = &["<<", ">>", "<<<", ">>>", "**"];
/// Assignment and compound-assignment operators (`=`, `+=`, `-=`, ...).
pub const ASSIGN: &[&str] = &["=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=", "<<<=", ">>>="];

/// `bit`/`logic` vector of width `w`, as `slang`'s `getType(width, flags)` prints it.
pub fn vector(w: u32, s: bool, four: bool) -> TypeInfo {
    let base = if four { "logic" } else { "bit" };
    let sign = if s { " signed" } else { "" };
    let text = if w <= 1 { format!("{base}{sign}") } else { format!("{base}{sign}[{}:0]", w - 1) };
    TypeInfo { text, integral: Some((w.max(1), s, four)), src: None, aggregate: false }
}

/// A vector with an explicit range, as literals and casts print (`bit[0:0]`).
pub fn ranged(w: u32, s: bool, four: bool) -> TypeInfo {
    let base = if four { "logic" } else { "bit" };
    let sign = if s { " signed" } else { "" };
    TypeInfo {
        text: format!("{base}{sign}[{}:0]", w.max(1) - 1),
        integral: Some((w.max(1), s, four)),
        src: None,
        aggregate: false,
    }
}

/// Returns `true` when `t` is a plain bit-vector type (not an enum, struct, union, or non-zero-based slice).
pub fn simple(t: &TypeInfo) -> bool {
    !(t.text.starts_with("enum") || t.text.starts_with("struct") || t.text.starts_with("union"))
        && t.integral.is_some()
        && !t.aggregate
        && !(t.text.ends_with(']') && t.text.rsplit_once(':').is_some_and(|(_, lo)| lo != "0]"))
}

/// Returns `true` when `a` and `b` denote the same integral/enum type.
pub fn same_type(a: &TypeInfo, b: &TypeInfo) -> bool {
    let name = |t: &TypeInfo| t.text.rsplit_once('}').map_or(t.text.clone(), |(_, n)| n.to_string());
    a.integral == b.integral && name(a) == name(b)
}

/// IEEE 1800 §11.6/11.8 `binaryOperatorType`: wider width, signed only when both are signed,
/// four-state when either is four-state (or `force_four` is set, e.g. `/` and `%`).
pub fn binary_type(l: &TypeInfo, r: &TypeInfo, force_four: bool) -> Option<TypeInfo> {
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

/// Pure `(width, signed, four_state)` binary operator shape per IEEE 1800 §11.6/11.8.
pub fn binary_shape(lhs: (u32, bool, bool), rhs: (u32, bool, bool), force_four: bool) -> (u32, bool, bool) {
    (lhs.0.max(rhs.0), lhs.1 && rhs.1, lhs.2 || rhs.2 || force_four)
}

/// Context type propagated into RHS `r` when assigned to LHS type `l` (IEEE 1800 §11.6.1 / §10.7).
pub fn assigned_type(l: TypeInfo, r: TypeInfo, r_is_unbased: bool) -> Option<TypeInfo> {
    let (lw, ls, l4) = l.integral?;
    if r_is_unbased {
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

/// Pure `(width, signed, four_state)` context propagated into an assigned RHS expression.
pub fn assigned_shape(lhs: (u32, bool, bool), rhs: (u32, bool, bool), rhs_is_unbased: bool) -> (u32, bool, bool) {
    if rhs_is_unbased {
        (lhs.0, false, lhs.2)
    } else {
        (lhs.0.max(rhs.0), rhs.1, lhs.2 || rhs.2)
    }
}

/// Returns `true` for unbased unsized fill literals (`'0`, `'1`, `'x`, `'z`, `'X`, `'Z`).
pub fn is_unbased_unsized_tok(tok: &str) -> bool {
    matches!(tok, "'0" | "'1" | "'x" | "'z" | "'X" | "'Z")
}

/// Self-determined type of an integer, unbased-unsized, string, real, or time literal token.
pub fn literal_type(text: &str) -> Option<TypeInfo> {
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
                    Some('\n') => continue,
                    _ => {}
                }
            }
            n = n.checked_add(1)?;
        }
        let width = n.max(1).checked_mul(8).filter(|&w| w <= crate::types::MAX_BIT_WIDTH)?;
        return Some(ranged(width, false, false));
    }
    let t = text.replace('_', "");
    if let Some(rest) = t.strip_prefix('\'') {
        if rest.len() == 1 && matches!(rest, "0" | "1" | "x" | "X" | "z" | "Z") {
            return Some(vector(1, false, !matches!(rest, "0" | "1")));
        }
    }
    if let Some(q) = t.find('\'') {
        let size: u32 = if q == 0 { 32 } else { t[..q].trim().parse().ok()? };
        if size == 0 || size > crate::types::MAX_BIT_WIDTH {
            return None;
        }
        let rest = t[q + 1..].to_ascii_lowercase();
        let signed = rest.starts_with('s');
        let digits = rest.trim_start_matches('s');
        let digits = &digits[1.min(digits.len())..];
        let f = digits.contains(['x', 'z', '?']);
        return Some(ranged(size, signed, f));
    }
    if t.contains('.') || (t.contains(['e', 'E']) && t.parse::<f64>().is_ok()) {
        return Some(TypeInfo::other("real"));
    }
    if !t.is_empty() && t.chars().all(|ch| ch.is_ascii_digit()) {
        let mut ti = vector(32, true, false);
        ti.text = "int".to_string();
        return Some(ti);
    }
    if t.ends_with('s') {
        return Some(TypeInfo::other("realtime"));
    }
    None
}

/// Built-in system function return [`TypeInfo`] (given optional first argument's integral shape).
pub fn syscall_type(name: &str, arg0_integral: Option<(u32, bool, bool)>) -> Option<TypeInfo> {
    let named = |w: u32, s: bool, four: bool, n: &str| {
        let mut t = vector(w, s, four);
        t.text = n.to_string();
        t
    };
    match name {
        "$signed" | "$unsigned" => {
            let (w, _, f) = arg0_integral?;
            Some(vector(w, name == "$signed", f))
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
        | "$unpacked_dimensions" => Some(named(32, true, true, "integer")),
        "$countones" | "$test$plusargs" | "$value$plusargs" | "$random" => Some(named(32, true, false, "int")),
        "$urandom" | "$urandom_range" => Some(named(32, false, false, "int unsigned")),
        "$isunknown" | "$onehot" | "$onehot0" => Some(vector(1, false, false)),
        "$rose" | "$fell" | "$stable" | "$changed" => Some(vector(1, false, false)),
        "$past" | "$sampled" => {
            let (w, s, f) = arg0_integral?;
            Some(vector(w, s, f))
        }
        "$time" | "$stime" => Some(named(64, false, true, "time")),
        "$realtime" | "$itor" | "$bitstoreal" => Some(TypeInfo::other("real")),
        _ => None,
    }
}
