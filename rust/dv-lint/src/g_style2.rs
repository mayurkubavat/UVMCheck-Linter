//! Style rules that the C++ checks on slang's AST (builder slice N6.3,
//! group F): typedef name suffixes. They read the
//! syntax tree and report once per elaborated copy of the code (see
//! [`crate::g_deprecated::copies`]). Messages are verbatim.

use std::collections::HashMap;

use sv_syntax::NodeId;

use crate::g_deprecated::copies;
use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-STY-004", name: "TypedefNameSuffix", check: Syntax(typedef_name_suffix) },
    ]
}

/// What a typedef names: an enum, a struct, or another type by name.
enum Target {
    Enum,
    Struct,
    Named(String),
    Other,
}

fn target(c: &Cst, ty: NodeId) -> Target {
    let Some(first) = c.first_tok(ty) else { return Target::Other };
    match c.text_of(first) {
        "enum" => Target::Enum,
        "struct" => Target::Struct,
        _ => {
            // A plain or package-scoped type name, without dimensions.
            if c.tree.children(ty).next().is_some() {
                return Target::Other;
            }
            let text = c.tree.bare_text(ty);
            let name = text.rsplit("::").next().unwrap_or("").to_string();
            if name.is_empty() || !name.chars().all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '$') {
                return Target::Other;
            }
            Target::Named(name)
        }
    }
}

fn typedef_name_suffix(c: &Cst, s: &mut Sink) {
    let mut defs: Vec<(NodeId, String, Target)> = Vec::new();
    for n in c.nodes(TypedefDecl) {
        let Some(ty) = c.tree.child(n, DataType) else { continue };
        let Some(name) = c.tree.child(n, Name) else { continue };
        defs.push((n, c.tree.bare_text(name), target(c, ty)));
    }
    let by_name: HashMap<&str, &Target> = defs.iter().map(|(_, n, t)| (n.as_str(), t)).collect();
    let resolve = |t: &Target| -> Option<bool> {
        // Some(true) enum, Some(false) struct.
        let mut t = t;
        for _ in 0..32 {
            match t {
                Target::Enum => return Some(true),
                Target::Struct => return Some(false),
                Target::Other => return None,
                Target::Named(n) => t = by_name.get(n.as_str())?,
            }
        }
        None
    };
    for (n, name, t) in &defs {
        if name.is_empty() {
            continue;
        }
        let Some(is_enum) = resolve(t) else { continue };
        let msg = if is_enum {
            if name.ends_with("_e") {
                continue;
            }
            format!("Enumeration typedef '{name}' does not end with standard suffix '_e'. By convention, enum types must end with '_e'.")
        } else {
            if name.ends_with("_t") {
                continue;
            }
            format!("Structure typedef '{name}' does not end with standard suffix '_t'. By convention, struct and data types must end with '_t'.")
        };
        let Some(at) = c.tree.child(*n, Name).and_then(|x| c.first_tok(x)) else { continue };
        for _ in 0..copies(c, *n) {
            s.add("SV-STY-004", "TypedefNameSuffix", Severity::Warning, msg.clone(), c.loc(at));
        }
    }
    // slang gives each cover cross an implicit struct typedef, CrossValType,
    // located at the cross.
    for n in c.nodes(Cross) {
        let at = c.tree.child(n, StmtLabel).and_then(|l| c.first_tok(l)).or_else(|| c.tok(n, "cross"));
        let Some(at) = at else { continue };
        for _ in 0..copies(c, n) {
            s.add(
                "SV-STY-004",
                "TypedefNameSuffix",
                Severity::Warning,
                "Structure typedef 'CrossValType' does not end with standard suffix '_t'. By convention, struct and data types must end with '_t'.".into(),
                c.loc(at),
            );
        }
    }
}
