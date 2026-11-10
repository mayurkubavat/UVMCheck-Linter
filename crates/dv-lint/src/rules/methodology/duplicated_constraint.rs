//! SV-DUP-003 (DuplicatedConstraint): Flags a constraint body repeated in two
//! classes that do not inherit from each other.

use crate::support::class_roles::{derives, user_classes, Index};
use crate::support::written_tokens::{tok_where, token_key, written_tokens, Generated};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Class, Design};
use std::collections::HashMap;
use svlang::syntax::SyntaxKind::*;

pub(crate) fn duplicated_constraint(d: &Design, csts: &[Cst], out: &mut Sink) {
    // Option `min_tokens` (rules.SV-DUP-003.options).
    fn min_tokens() -> usize {
        crate::options::get("SV-DUP-003", "min_tokens", 8) as usize
    }
    let ix = Index::new(csts);
    let mut generated = Generated::new(d, csts);
    let mut seen: HashMap<String, Vec<(&Class, String, String)>> = HashMap::new();
    for cls in user_classes(d) {
        if generated.at(cls.loc) {
            continue;
        }
        let Some((c, decl)) = ix.decl(cls.loc) else { continue };
        if c.kind(decl) != ClassDecl {
            continue;
        }
        for x in c.items(decl) {
            if c.kind(x) != ConstraintDecl {
                continue;
            }
            let Some(blk) = c.tree.child(x, ConstraintBlock) else { continue };
            let Some((_, nt)) = c.tree.child(x, Name).and_then(|n| c.tree.token_bounds(n)) else { continue };
            let name = c.text_of(nt).to_string();
            let loc = c.loc(nt);
            let at = tok_where(c, nt);
            let mut toks = Vec::new();
            written_tokens(c, blk, &mut toks);
            if toks.len() < min_tokens() + 2 {
                continue;
            }
            let firsts = seen.entry(token_key(&toks, false)).or_default();
            let found =
                firsts.iter().find(|(k, ..)| k.id != cls.id && !derives(d, cls.id, k.id) && !derives(d, k.id, cls.id));
            match found {
                Some((k, kname, kat)) => {
                    out.add(
                        "SV-DUP-003",
                        "DuplicatedConstraint",
                        Severity::Warning,
                        format!(
                            "Constraint '{name}' in '{}' repeats constraint '{kname}' of '{}' ({}). Declare it once, in a base class both \
extend or in a shared constraint policy, so a change reaches both.",
                            cls.name,
                            k.name, kat
                        ),
                        loc,
                    );
                }
                None => firsts.push((cls, name, at)),
            }
        }
    }
}
