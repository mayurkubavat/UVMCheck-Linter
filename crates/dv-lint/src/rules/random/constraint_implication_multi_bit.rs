//! SV-RND-019 (ConstraintImplicationMultiBit): Flags constraint implication
//! expressions 'A -> B' where condition 'A' is a multi-bit vector without an
//! explicit boolean comparison.

use crate::support::class_items::{declarators, kids, model_classes, qualifiers, subtree};
use crate::support::constraints::{const_int, constraint_blocks, param_values, type_width, typedefs};
use crate::support::data_items::unparen;
use crate::support::type_text::base_name;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashMap;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

struct Widths<'a> {
    params: HashMap<String, NodeId>,
    typedefs: HashMap<String, NodeId>,
    classes: &'a HashMap<String, NodeId>,
}

impl Widths<'_> {
    /// Width of a scalar property of `cls` or its bases.
    fn prop(&self, c: &Cst, cls: NodeId, name: &str) -> Option<i64> {
        let mut cur = Some(cls);
        let mut guard = 0;
        while let Some(k) = cur {
            for d in c.tree.children(k).filter(|&d| c.kind(d) == DataDecl) {
                if qualifiers(c, d).contains(&"typedef") {
                    continue;
                }
                for (dcl, v, _) in declarators(c, d) {
                    if v == name {
                        if c.tree.children(dcl).any(|x| c.kind(x) == Dim) {
                            return None;
                        }
                        return type_width(c, c.tree.child(d, DataType)?, &self.params, &self.typedefs, 0);
                    }
                }
            }
            guard += 1;
            cur = if guard < 64 { base_name(c, k).and_then(|b| self.classes.get(&b).copied()) } else { None };
        }
        None
    }

    /// Self-determined width of an integral expression, when the syntax
    /// gives it.
    fn expr(&self, c: &Cst, cls: NodeId, e: NodeId) -> Option<i64> {
        let k = kids(c, e);
        match c.kind(e) {
            ParenExpr => self.expr(c, cls, *k.first()?),
            NameExpr => self.prop(c, cls, &c.tree.bare_text(e)),
            Literal => {
                let t = c.tree.bare_text(e);
                match t.find('\'') {
                    _ if t.starts_with('"') => None,
                    Some(0) if t.len() == 2 && !t[1..].starts_with(['h', 'd', 'b', 'o', 'H', 'D', 'B', 'O']) => Some(1),
                    Some(0) => Some(32),
                    Some(q) => t[..q].parse().ok(),
                    None if t.chars().all(|x| x.is_ascii_digit() || x == '_') => Some(32),
                    None => None,
                }
            }
            PrefixExpr => match c.text_of(c.first_tok(e)?) {
                "!" | "&" | "|" | "^" | "~&" | "~|" | "~^" | "^~" => Some(1),
                "~" | "-" | "+" => self.expr(c, cls, *k.first()?),
                _ => None,
            },
            BinaryExpr => match crate::support::class_items::op(c, e) {
                "==" | "!=" | "===" | "!==" | "==?" | "!=?" | "<" | "<=" | ">" | ">=" | "&&" | "||" | "->" | "<->" => {
                    Some(1)
                }
                "+" | "-" | "*" | "/" | "%" | "&" | "|" | "^" | "~^" | "^~" => {
                    Some(self.expr(c, cls, *k.first()?)?.max(self.expr(c, cls, *k.get(1)?)?))
                }
                "<<" | ">>" | "<<<" | ">>>" | "**" => self.expr(c, cls, *k.first()?),
                _ => None,
            },
            SelectExpr => {
                let w = self.expr(c, cls, *k.first()?)?;
                match (k.len(), c.tok(e, ":").is_some(), c.tok(e, "+:").or(c.tok(e, "-:")).is_some()) {
                    (2, false, false) if w > 1 => Some(1),
                    (3, true, false) => {
                        let (a, b) = (const_int(c, k[1], &self.params, 0)?, const_int(c, k[2], &self.params, 0)?);
                        Some((a - b).abs() + 1)
                    }
                    (3, false, true) => const_int(c, k[2], &self.params, 0),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

pub(crate) fn implication_multi_bit(c: &Cst, s: &mut Sink) {
    let classes: HashMap<String, NodeId> = model_classes(c).into_iter().map(|(k, n)| (n, k)).collect();
    let w = Widths { params: param_values(c), typedefs: typedefs(c), classes: &classes };
    for (cls, _) in model_classes(c) {
        for block in constraint_blocks(c, cls) {
            for item in subtree(c, block).into_iter().filter(|&x| c.kind(x) == ConstraintItem) {
                if c.tok(item, "->").is_none() {
                    continue;
                }
                let Some(pred) = c.tree.children(item).next() else { continue };
                let Some(width) = w.expr(c, cls, pred).filter(|&x| x > 1) else { continue };
                let Some(at) = c.first_tok(unparen(c, pred)) else { continue };
                s.add(
                    "SV-RND-019",
                    "ConstraintImplicationMultiBit",
                    Severity::Warning,
                    format!(
                        "Constraint implication operator '->' has a multi-bit condition expression of width {width} bits \
                         without an explicit boolean comparison. In SystemVerilog constraint solving (IEEE 1800 § 18.5.6), \
                         multi-bit implication is often confused with bitwise masking; use an explicit comparison (e.g. \
                         'expr != 0 -> ...') to make constraint intent unambiguous."
                    ),
                    c.loc(at),
                );
            }
        }
    }
}
