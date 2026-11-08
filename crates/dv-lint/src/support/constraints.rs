//! Constraint blocks, parameter values and type widths, for the dist and
//! solve-before rules.

use std::collections::HashMap;

use sv_syntax::NodeId;

use crate::support::class_items::{declarators, is_parameterized, kids, model_classes, qualifiers, user_code};

use crate::support::data_items::{strip_conv, unparen};

use crate::support::sema_facts::ast_ok;

use crate::syntax::Cst;

use sv_syntax::SyntaxKind::*;

/// Classes the UVM model lists, without the library's: (class, name).
pub(crate) fn user_model_classes(c: &Cst) -> Vec<(NodeId, String)> {
    model_classes(c).into_iter().filter(|(k, n)| user_code(c, *k) && !n.starts_with("uvm_")).collect()
}

/// The class's constraint blocks that bound without error.
pub(crate) fn constraint_blocks(c: &Cst, cls: NodeId) -> Vec<NodeId> {
    c.tree
        .children(cls)
        .filter(|&d| c.kind(d) == ConstraintDecl && ast_ok(c, d))
        .filter_map(|d| c.tree.child(d, ConstraintBlock))
        .collect()
}

/// The value of an integer literal (`8'hFF`, `1_000`, `'d5`).
pub(crate) fn literal_value(text: &str) -> Option<i64> {
    let t: String = text.chars().filter(|&x| x != '_' && !x.is_whitespace()).collect();
    let (digits, radix) = match t.find('\'') {
        Some(q) => {
            let rest = t[q + 1..].trim_start_matches(['s', 'S']);
            let mut ch = rest.chars();
            let radix = match ch.next()?.to_ascii_lowercase() {
                'h' => 16,
                'd' => 10,
                'b' => 2,
                'o' => 8,
                _ => return None,
            };
            (ch.as_str().to_string(), radix)
        }
        None => (t, 10),
    };
    i64::from_str_radix(&digits, radix).ok()
}

/// A constant integer: literals, parameters and arithmetic over them.
pub(crate) fn const_int(c: &Cst, e: NodeId, params: &HashMap<String, NodeId>, depth: u32) -> Option<i64> {
    if depth > 16 {
        return None;
    }
    let e = unparen(c, strip_conv(c, e).0);
    match c.kind(e) {
        Literal => literal_value(&c.tree.bare_text(e)),
        NameExpr => const_int(c, *params.get(&c.tree.bare_text(e))?, params, depth + 1),
        PrefixExpr => {
            let v = const_int(c, *kids(c, e).first()?, params, depth + 1)?;
            match c.text_of(c.first_tok(e)?) {
                "-" => Some(-v),
                "+" => Some(v),
                _ => None,
            }
        }
        BinaryExpr => {
            let k = kids(c, e);
            let (a, b) = (const_int(c, *k.first()?, params, depth + 1)?, const_int(c, *k.get(1)?, params, depth + 1)?);
            match crate::support::class_items::op(c, e) {
                "+" => a.checked_add(b),
                "-" => a.checked_sub(b),
                "*" => a.checked_mul(b),
                "/" if b != 0 => Some(a / b),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Parameters with a value: name -> value expression.
pub(crate) fn param_values(c: &Cst) -> HashMap<String, NodeId> {
    let mut out = HashMap::new();
    for d in c.nodes(ParamDecl) {
        for (dcl, v, _) in declarators(c, d) {
            if let Some(&e) = kids(c, dcl).iter().rev().find(|&&k| crate::support::sema_facts::is_expr(c.kind(k))) {
                out.insert(v, e);
            }
        }
    }
    out
}

/// Width of one dimension, `None` for a dynamic one.
pub(crate) fn dim_width(c: &Cst, d: NodeId, params: &HashMap<String, NodeId>) -> Option<i64> {
    let k = kids(c, d);
    match (k.as_slice(), c.tok(d, ":").is_some()) {
        ([lo, hi], true) => Some((const_int(c, *hi, params, 0)? - const_int(c, *lo, params, 0)?).abs() + 1),
        ([n], false) if c.tok(d, "$").is_none() => const_int(c, *n, params, 0),
        _ => None,
    }
}

/// Bit width of a data type (slang's getBitWidth), when the syntax gives it.
pub(crate) fn type_width(
    c: &Cst,
    dt: NodeId,
    params: &HashMap<String, NodeId>,
    typedefs: &HashMap<String, NodeId>,
    depth: u32,
) -> Option<i64> {
    if depth > 8 {
        return None;
    }
    let ft = c.first_tok(dt)?;
    let first = c.text_of(ft);
    let base = match first {
        "bit" | "logic" | "reg" => 1,
        "byte" => 8,
        "shortint" => 16,
        "int" | "integer" => 32,
        "longint" | "time" => 64,
        "enum" => match c.tree.child(dt, DataType) {
            Some(b) => type_width(c, b, params, typedefs, depth + 1)?,
            None => 32,
        },
        _ if c.tree.tokens[ft as usize].kind == sv_syntax::TokenKind::Identifier
            && c.tree.token_range(dt).len() == 1 =>
        {
            type_width(c, *typedefs.get(first)?, params, typedefs, depth + 1)?
        }
        _ => return None,
    };
    let mut w = base;
    for d in c.tree.children(dt).filter(|&x| c.kind(x) == Dim) {
        w *= dim_width(c, d, params)?;
    }
    Some(w)
}

/// Each randc variable with its width: (name token, name, width).
pub(crate) fn randc_vars(c: &Cst) -> Vec<(u32, String, i64)> {
    let params = param_values(c);
    let typedefs = typedefs(c);
    let mut out = Vec::new();
    for d in c.nodes(DataDecl) {
        if !qualifiers(c, d).contains(&"randc") {
            continue;
        }
        if c.parent(d).is_some_and(|p| c.kind(p) == ClassDecl && is_parameterized(c, p)) {
            continue;
        }
        let Some(dt) = c.tree.child(d, DataType) else { continue };
        let Some(w) = type_width(c, dt, &params, &typedefs, 0) else { continue };
        // An unpacked array has no bit width in slang (getBitWidth is 0).
        for (dcl, name, tok) in declarators(c, d) {
            if !c.tree.children(dcl).any(|x| c.kind(x) == Dim) {
                out.push((tok, name, w));
            }
        }
    }
    out
}

/// Typedefs by name (no unpacked dimensions).
pub(crate) fn typedefs(c: &Cst) -> HashMap<String, NodeId> {
    let mut out = HashMap::new();
    for t in c.nodes(TypedefDecl) {
        if let (Some(dt), Some(n)) = (c.tree.child(t, DataType), c.tree.child(t, Name)) {
            if !c.tree.children(t).any(|x| c.kind(x) == Dim) {
                out.insert(c.tree.bare_text(n), dt);
            }
        }
    }
    out
}

/// Class declarations by name (out-of-block names left out).
pub(crate) fn class_names(c: &Cst) -> HashMap<String, NodeId> {
    crate::support::class_items::class_decls(c)
        .into_iter()
        .filter(|&k| !c.tree.child(k, Header).is_some_and(|h| c.tok(h, "::").is_some()))
        .filter_map(|k| Some((crate::support::class_items::decl_name(c, k)?.0, k)))
        .collect()
}
