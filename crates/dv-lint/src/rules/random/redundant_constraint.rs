//! SV-RND-011 (RedundantConstraint): Detects duplicate or redundant constraint
//! expressions within the same class scope.

use crate::support::class_items::{class_decls, decl_name, expression_constraint, subtree, trim};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use sv_syntax::SyntaxKind::*;

fn strip_comments_and_ws(input: &str) -> String {
    let b = input.as_bytes();
    let mut out = Vec::new();
    let (mut line, mut block) = (false, false);
    let mut i = 0;
    while i < b.len() {
        let ch = b[i];
        if line {
            if ch == b'\n' {
                line = false;
            }
        } else if block {
            if ch == b'*' && b.get(i + 1) == Some(&b'/') {
                block = false;
                i += 1;
            }
        } else if ch == b'/' && b.get(i + 1) == Some(&b'/') {
            line = true;
            i += 1;
        } else if ch == b'/' && b.get(i + 1) == Some(&b'*') {
            block = true;
            i += 1;
        } else if !matches!(ch, b' ' | b'\t' | b'\n' | b'\r') {
            out.push(ch);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn redundant_constraint(c: &Cst, s: &mut Sink) {
    for cls in class_decls(c) {
        let cname = decl_name(c, cls).map(|x| x.0).filter(|x| !x.is_empty()).unwrap_or_else(|| "unnamed".into());
        let mut seen: HashSet<String> = HashSet::new();
        for item in subtree(c, cls) {
            if c.kind(item) != ConstraintItem {
                continue;
            }
            let Some((soft, e)) = expression_constraint(c, item) else { continue };
            let raw = trim(&c.text(e)).to_string();
            let mut clean = raw.as_str();
            if let Some(i) = clean.rfind('\n') {
                clean = clean[i + 1..].trim_start_matches([' ', '\t', '\n', '\r']);
            }
            let key = format!("{}{}", if soft { "soft:" } else { "" }, strip_comments_and_ws(&raw));
            if !seen.insert(key) {
                let Some(at) = c.first_tok(item) else { continue };
                s.add(
                    "SV-RND-011",
                    "RedundantConstraint",
                    Severity::Warning,
                    format!(
                        "Duplicate constraint expression '{clean}' in class '{cname}'. Duplicate constraints bloat the solver constraint graph and increase solver compile overhead without introducing new assertions."
                    ),
                    c.loc(at),
                )
                .fixes
                .extend(c.tree.token_bounds(item).and_then(|(a, b)| {
                    c.token_fix(a, b, "", &format!("Remove duplicate constraint expression '{clean}'"))
                }));
            }
        }
    }
}
