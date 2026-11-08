//! SV-TYP-008 (RedundantCastToSameType): Flags dynamic '$cast' calls or static
//! type casts where the source and destination already have identical static
//! types.
//!
//! A cast whose type or size depends on parameters is not redundant for the
//! code as written, even where one specialization makes it so (see
//! `support::param_dep`), unless the operand is declared with that type.

use crate::support::config_db_calls::{in_generate_header, strip_parens};
use crate::support::param_dep::{declared_as, depends_on_params};
use crate::support::slang_ast::kids;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// `$cast(a, b)` on matching types, or a cast to the operand's own type:
/// once per AST visit.
pub(crate) fn redundant_cast(c: &Cst, out: &mut Sink) {
    let specs = crate::expr_types::spec_types(c);
    let ix = c.index();
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
        // Per instance where a module's instances type them differently.
        let mut v = Vec::new();
        for (k, ts) in crate::expr_types::visit_types(c, e, &[a, b]) {
            let (Some((at, ac)), Some((bt, bc))) = (&ts[0], &ts[1]) else { continue };
            if at.text == bt.text || (ac.is_some() && ac == bc) {
                v.extend(std::iter::repeat_n(at.text.clone(), k));
            }
        }
        v
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
        // `idx_t'(sel)`, `Width'(x)`: redundant only for some parameter
        // values, unless the operand is declared with that very type.
        let target = ks[0];
        if depends_on_params(c, e, target) && !declared_as(c, inner, target) {
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
