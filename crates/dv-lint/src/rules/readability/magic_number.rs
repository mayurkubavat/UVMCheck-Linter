//! SV-RDB-002 (MagicNumber): Flags numeric literals other than 0, 1 and -1 in
//! class method bodies: conditions, loop bounds and sizes; delays are left to
//! the magic-delay rules (Checkstyle MagicNumber, Sonar S109).

use sv_syntax::{NodeId, SyntaxKind};
use crate::support::class_index::Classes;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::literal_values::{literal_value};

fn is_display_call(n: &str) -> bool {
    n.starts_with("$display")
        || n.starts_with("$write")
        || n.starts_with("$sformat")
        || n.starts_with("$fdisplay")
        || n.starts_with("$fwrite")
        || n == "$psprintf"
        || n == "$swrite"
}

pub(crate) fn magic_number(c: &Cst, s: &mut Sink) {
    use crate::support::class_index::{report_level, Level, Role};
    let classes = Classes::new(c);
    let in_module = |n: NodeId| {
        std::iter::successors(c.parent(n), |&a| c.parent(a))
            .any(|a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | CheckerDecl))
    };
    // Default specializations join only when the model has a class at all.
    let any_plain = classes.list.iter().any(|k| !k.generic && !in_module(k.node));
    for i in classes.user() {
        let cls = &classes.list[i];
        if cls.generic && !any_plain {
            continue;
        }
        // The model indexes classes of packages and $unit, not of modules.
        if classes.role(i) == Role::RegModel
            || in_module(cls.node)
        {
            continue;
        }
        for m in &cls.methods {
            let Some(body) = m.body else { continue };
            let where_ = format!("{}::{}", cls.name, m.name);
            let mut reported = std::collections::HashSet::new();
            let mut stack = vec![body];
            while let Some(x) = stack.pop() {
                match c.kind(x) {
                    CallExpr => {
                        let name = crate::support::class_items::call_name(c, x).unwrap_or("");
                        if report_level(c, x) != Level::None || is_display_call(name) {
                            continue;
                        }
                    }
                    TimingControl if c.tok(x, "#").is_some() => continue,
                    // Local declarations are symbols the statement visitor
                    // does not enter.
                    DataDecl => continue,
                    Literal => {
                        let Some(v) = literal_value(c, x) else { continue };
                        if matches!(v, 0 | 1 | -1) {
                            continue;
                        }
                        let Some(t) = c.first_tok(x) else { continue };
                        if c.tree.tokens[t as usize].from_macro || !crate::support::sema_facts::ast_ok(c, x) {
                            continue;
                        }
                        if !reported.insert(v) {
                            continue;
                        }
                        s.add(
                            "SV-RDB-002",
                            "MagicNumber",
                            Severity::Info,
                            format!(
                                "Magic number {v} in {where_}. Give it a name (localparam, enum or config field) so its meaning and every place that depends on it are visible."
                            ),
                            c.loc(t),
                        );
                        continue;
                    }
                    _ => {}
                }
                let kids: Vec<NodeId> = c.tree.children(x).collect();
                stack.extend(kids.into_iter().rev());
            }
        }
    }
}
