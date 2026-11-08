//! SV-LANG-002 (BitSelectOutOfBounds): Detects static out-of-bounds bit or
//! element selects on vectors and arrays with fixed ranges.

use crate::support::literal_values::{decl_of_expr, int_value};
use crate::support::literals::ast_times;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// `[left:right]` of the first dimension a select on the declaration
/// indexes: its first unpacked dimension, else its first packed one, else
/// the bits of a built-in integer type.
fn fixed_range(c: &Cst, d: &crate::support::slang_ast::Decl) -> Option<(i64, i64)> {
    let bounds = |dim: NodeId| -> Option<(i64, i64)> {
        let k: Vec<NodeId> = c.tree.children(dim).collect();
        match k[..] {
            [n] if c.tok(dim, ":").is_none() => {
                let n = int_value(c, n)?;
                (n > 0).then_some((0, n - 1))
            }
            [a, b] if c.tok(dim, ":").is_some() && c.tok(dim, "+:").is_none() && c.tok(dim, "-:").is_none() => {
                Some((int_value(c, a)?, int_value(c, b)?))
            }
            _ => None,
        }
    };
    if c.kind(d.node) == Declarator {
        if let Some(dim) = c.tree.children(d.node).find(|&x| c.kind(x) == Dim) {
            return bounds(dim);
        }
    }
    let mut dt = d.dtype?;
    // `typedef logic [3:0] nib_t;`, `typedef int arr_t[4];`
    for _ in 0..4 {
        let toks: Vec<u32> = c.toks(dt).collect();
        let [t] = toks[..] else { break };
        if c.tree.tokens[t as usize].kind != sv_syntax::TokenKind::Identifier {
            break;
        }
        let name = c.text_of(t);
        let td = c.first_named(TypedefDecl, name)?;
        if let Some(dim) = c.tree.children(td).find(|&x| c.kind(x) == Dim) {
            return bounds(dim);
        }
        dt = c.tree.child(td, DataType)?;
    }
    if let Some(dim) = c.tree.children(dt).find(|&x| c.kind(x) == Dim) {
        return bounds(dim);
    }
    let kw = c.first_tok(dt).map(|t| c.text_of(t))?;
    let bits = match kw {
        "byte" => 8,
        "shortint" => 16,
        "int" | "integer" => 32,
        "longint" => 64,
        _ => return None,
    };
    // Only the keyword (and signing) is written.
    if c.toks(dt).count() > 2 {
        return None;
    }
    Some((bits - 1, 0))
}

pub(crate) fn bit_select_out_of_bounds(c: &Cst, s: &mut Sink) {
    for sel in c.nodes(SelectExpr) {
        if c.tok(sel, ":").is_some() || c.tok(sel, "+:").is_some() || c.tok(sel, "-:").is_some() {
            continue;
        }
        let k: Vec<NodeId> = c.tree.children(sel).collect();
        let [base, index] = k[..] else { continue };
        let Some(idx) = int_value(c, index) else { continue };
        let Some((d, name)) = decl_of_expr(c, c.strip(base)) else { continue };
        let Some((left, right)) = fixed_range(c, &d) else { continue };
        if idx >= left.min(right) && idx <= left.max(right) {
            continue;
        }
        for _ in 0..ast_times(c, sel) {
            s.add(
                "SV-LANG-002",
                "BitSelectOutOfBounds",
                Severity::Error,
                format!(
                    "Static out-of-bounds bit/element select: index [{idx}] is outside declared range [{left}:{right}] of '{name}'. In SystemVerilog (IEEE 1800 \u{a7} 7.4.5), reading out-of-bounds returns 'x without crashing, silently propagating uninitialized state into downstream verification logic."
                ),
                c.loc_of(sel),
            );
        }
    }
}
