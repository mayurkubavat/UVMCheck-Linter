//! UVM-RUS-006 (IncompleteItemContract): Flags concrete sequence items with
//! fields that implement neither field macros nor do_copy(), do_compare() and
//! convert2string()/do_print().

use crate::support::class_roles::{class_methods, is_library, role, user_classes, Index};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use std::collections::HashSet;
use svlang::syntax::NodeId;

/// The class's text as written, from `class` to `endclass`.
fn class_text<'t>(c: &Cst<'t>, n: NodeId) -> &'t str {
    let Some((a, b)) = c.tree.token_bounds(n) else { return "" };
    let (ta, tb) = (&c.tree.tokens[a as usize], &c.tree.tokens[b as usize]);
    if ta.from_macro || tb.from_macro || ta.origin.file != tb.origin.file {
        return "";
    }
    let src = &c.tree.sources[ta.origin.file as usize];
    let end = (tb.origin.offset as usize + tb.text.len()).min(src.len());
    src.get(ta.origin.offset as usize..end).unwrap_or("")
}

pub(crate) fn incomplete_item_contract(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for cls in user_classes(d) {
        if role(d, cls) != crate::support::class_roles::Role::Item || cls.is_virtual {
            continue;
        }
        let (mut has_fields, mut macros) = (false, false);
        let mut methods = HashSet::new();
        let chain = std::iter::once(cls).chain(d.ancestors(cls.id));
        for k in chain {
            if is_library(d, k) {
                break;
            }
            has_fields |= k.properties.iter().any(|p| !p.is_static);
            if let Some((c, n)) = ix.decl(k.loc) {
                macros |= class_text(c, n).contains("`uvm_field_");
            }
            methods.extend(class_methods(k).map(|m| m.name.as_str()));
        }
        if !has_fields || macros {
            continue;
        }
        let mut missing = Vec::new();
        if !methods.contains("do_copy") {
            missing.push("do_copy()");
        }
        if !methods.contains("do_compare") {
            missing.push("do_compare()");
        }
        if !methods.contains("convert2string") && !methods.contains("do_print") {
            missing.push("convert2string() or do_print()");
        }
        if missing.is_empty() {
            continue;
        }
        let mut list = String::new();
        for (i, m) in missing.iter().enumerate() {
            list.push_str(if i == 0 {
                ""
            } else if i + 1 == missing.len() {
                " and "
            } else {
                ", "
            });
            list.push_str(m);
        }
        out.add(
            "UVM-RUS-006",
            "IncompleteItemContract",
            Severity::Warning,
            format!(
                "Sequence item '{}' has fields but no {list}. Without them copy(), clone() and compare() ignore the fields, so a scoreboard \
comparing two items always passes, and logs print nothing useful.",
                cls.name
            ),
            cls.loc,
        );
    }
}
