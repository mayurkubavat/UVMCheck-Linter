//! SV-TYP-008 (RedundantCastToSameType): Flags dynamic '$cast' calls or static
//! type casts where the source and destination already have identical static
//! types.

use crate::expr_types::{expr_types, ETy};
use crate::support::config_db_calls::{in_generate_header, strip_parens};
use crate::support::slang_ast::kids;
use crate::support::style_config::ast_visits;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// `$cast(a, b)` on matching types, or a cast to the operand's own type:
/// once per AST visit.
pub(crate) fn redundant_cast(c: &Cst, out: &mut Sink) {
    let types = expr_types(c);
    let canon = crate::expr_types::cast_canon(c);
    let specs = crate::expr_types::spec_types(c);
    let ix = c.index();
    let matching = |a: (NodeId, &ETy), b: (NodeId, &ETy)| {
        a.1.text == b.1.text || canon.get(&(ix, a.0)).is_some_and(|x| canon.get(&(ix, b.0)) == Some(x))
    };
    // The type `a` has in each copy where `a` and `b` match: per
    // specialization in a generic class, else every AST visit.
    let matches = |e: NodeId, a: NodeId, b: NodeId| -> Vec<String> {
        if let (Some(x), Some(y)) = (specs.get(&(ix, a)), specs.get(&(ix, b))) {
            return x
                .iter()
                .zip(y)
                .filter_map(|p| match p {
                    (Some((at, ac)), Some((bt, bc))) if at.text == bt.text || ac == bc => Some(at.text.clone()),
                    _ => None,
                })
                .collect();
        }
        let (Some(at), Some(bt)) = (types.get(&(ix, a)), types.get(&(ix, b))) else { return Vec::new() };
        if !matching((a, at), (b, bt)) {
            return Vec::new();
        }
        vec![at.text.clone(); ast_visits(c, e)]
    };
    for e in c.nodes(CallExpr) {
        let ks = kids(c, e);
        let Some(&callee) = ks.first() else { continue };
        if c.kind(callee) != NameExpr || c.tree.bare_text(callee) != "$cast" {
            continue;
        }
        let args = crate::expr_types::call_args(c, e);
        let (Some(&d), Some(&s)) = (args.first(), args.get(1)) else { continue };
        if in_generate_header(c, e) {
            continue;
        }
        let Some(t) = c.first_tok(e) else { continue };
        for text in matches(e, d, s) {
            out.add(
                "SV-TYP-008",
                "RedundantCastToSameType",
                Severity::Warning,
                format!(
                    "Redundant dynamic cast '$cast(...)': destination and source both share identical static type '{}'. In SystemVerilog typing rules (IEEE 1800 \u{a7} 6.24), dynamic casting between identical types is unnecessary; use standard assignment '=' instead.",
                    text
                ),
                c.loc(t),
            );
        }
    }
    for e in c.nodes(CastExpr) {
        let ks = kids(c, e);
        let Some(&inner) = ks.get(1) else { continue };
        let inner = strip_parens(c, inner);
        if in_generate_header(c, e) {
            continue;
        }
        let Some(t) = c.first_tok(e) else { continue };
        for text in matches(e, e, inner) {
            out.add(
                "SV-TYP-008",
                "RedundantCastToSameType",
                Severity::Info,
                format!(
                    "Redundant static cast to identical type '{}'. In SystemVerilog typing rules (IEEE 1800 \u{a7} 6.24), casting an expression to its already established static type adds unnecessary verbosity.",
                    text
                ),
                c.loc(t),
            );
        }
    }
}
