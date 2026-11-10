//! SV-RND-005 (NonRandFieldConstrained): Detects class member fields that are
//! constrained in a constraint block but not declared 'rand' or 'randc'.

use crate::support::class_items::{class_items, decl_name, declarators, model_classes, qualifiers};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn non_rand_field_constrained(c: &Cst, s: &mut Sink) {
    for (cls, cname) in model_classes(c) {
        let items = class_items(c, cls);
        let mut props: Vec<(String, u32)> = Vec::new();
        for &it in &items {
            if c.kind(it) != DataDecl || c.tree.child(it, DataType).is_none() {
                continue;
            }
            let q = qualifiers(c, it);
            if q.iter().any(|t| matches!(*t, "rand" | "randc" | "static" | "typedef")) {
                continue;
            }
            for (_, n, t) in declarators(c, it) {
                if !props.iter().any(|(p, _)| *p == n) {
                    props.push((n, t));
                }
            }
        }
        if props.is_empty() {
            continue;
        }
        for &it in &items {
            if c.kind(it) != ConstraintDecl {
                continue;
            }
            let Some((block_name, _)) = decl_name(c, it) else { continue };
            let text = c.text(it);
            for (p, at) in &props {
                let hit = [" inside", " ==", " <", " >", " <=", " >="]
                    .iter()
                    .any(|suffix| text.contains(&format!("{p}{suffix}")));
                if hit {
                    s.add(
                        "SV-RND-005",
                        "NonRandFieldConstrained",
                        Severity::Error,
                        format!(
                            "Class property '{p}' in class '{cname}' is constrained in constraint block '{block_name}' but is not declared with the 'rand' or 'randc' keyword. In SystemVerilog \
                             (IEEE 1800 § 18.4), non-rand fields are treated as fixed constants by the solver \
                             and will never be randomized. Add 'rand' to the declaration of '{p}'."
                        ),
                        c.loc(*at),
                    );
                }
            }
        }
    }
}
