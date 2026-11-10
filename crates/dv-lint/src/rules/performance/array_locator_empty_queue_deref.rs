//! SV-LOC-001 (ArrayLocatorEmptyQueueDeref): Disallows immediate element
//! dereferencing on array locator methods (e.g., q.find()[0]).

use crate::support::ast::{is_name, kids};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

const LOCATORS: &[&str] = &[
    "find",
    "find_index",
    "find_first",
    "find_first_index",
    "find_last",
    "find_last_index",
    "min",
    "max",
    "unique",
    "unique_index",
];

/// `getLocatorMethodName`: through parentheses and `with` clauses to the
/// method's name, else the expression's last token.
fn locator_name(c: &Cst, n: NodeId) -> Option<String> {
    match c.kind(n) {
        ParenExpr => {
            return kids(c, n).first().and_then(|&x| locator_name(c, x));
        }
        CallExpr => {
            let callee = kids(c, n).first().copied()?;
            if c.kind(callee) == CallExpr {
                // `x.find(..) with (..)`: slang's ArrayOrRandomizeMethod.
                return locator_name(c, callee);
            }
            let name = c.last_text(callee);
            if LOCATORS.contains(&name) {
                return Some(name.to_string());
            }
        }
        _ => {}
    }
    let name = c.last_text(n);
    LOCATORS.contains(&name).then(|| name.to_string())
}

pub(crate) fn locator_deref(c: &Cst, s: &mut Sink) {
    for n in c.nodes(SelectExpr) {
        let Some(&base) = kids(c, n).first() else { continue };
        if is_name(c, base) {
            continue;
        }
        let Some(name) = locator_name(c, base) else { continue };
        let Some(open) = c.tok(n, "[") else { continue };
        s.add(
            "SV-LOC-001",
            "ArrayLocatorEmptyQueueDeref",
            Severity::Error,
            format!(
                "Array locator method '{name}()' returns an unpacked queue which is immediately indexed with '[...]' without checking for emptiness. If no elements match, an empty queue is returned and accessing an index causes an out-of-bounds error per IEEE 1800 § 7.12.1. Store the locator result in a queue and check 'if (res.size() > 0)' before dereferencing."
            ),
            c.loc(open),
        );
    }
}
