//! SV-DEP-006 (NonAnsiPortList): Disallow obsolete non-ANSI port list
//! declarations.

use crate::support::names::DIRECTIONS;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::Child;
use sv_syntax::SyntaxKind::*;

pub(crate) fn non_ansi_ports(c: &Cst, s: &mut Sink) {
    // Module, interface and program headers: `module m(a, b);`.
    for h in c.nodes(Header) {
        let Some(list) = c.tree.child(h, PortList) else { continue };
        let Some(port) = c.tree.children(list).next() else { continue };
        let plain = c.kind(port) == Port
            && c.kids(port).iter().all(|k| matches!(*k, Child::Node(x) if c.kind(x) == Declarator))
            && c.tree.children(port).next().is_some_and(|d| c.kids(d).len() == 1 && c.tree.child(d, Name).is_some());
        let explicit = c.kind(port) == Port && c.keyword(port).is_some_and(|t| matches!(c.text_of(t), "." | "{"));
        if !(plain || explicit) {
            continue;
        }
        let Some(open) = c.tok(list, "(") else { continue };
        s.add(
            "SV-DEP-006",
            "NonAnsiPortList",
            Severity::Info,
            "Non-ANSI port list declaration is obsolete in IEEE 1800 § 13.3. \
             Separate declaration of port names in header and types in body introduces redundant typing and syntax hazards. \
             Use modern ANSI-style inline port declarations: 'module name(input logic [7:0] a, output logic b)'."
                .into(),
            c.loc(open),
        );
    }
    // Tasks and functions with port declarations in the body.
    let mut routines = c.nodes(FunctionDecl);
    routines.extend(c.nodes(TaskDecl));
    for n in routines {
        if c.tree.child(n, PortList).is_some() {
            continue;
        }
        let Some(body) = c.tree.child(n, Body) else { continue };
        let Some(item) = c
            .tree
            .children(body)
            .find(|&x| c.kind(x) == DataDecl && c.keyword(x).is_some_and(|t| DIRECTIONS.contains(&c.text_of(t))))
        else {
            continue;
        };
        let Some(first) = c.first_tok(item) else { continue };
        let name = c.tree.child(n, Name).map(|x| c.text(x)).unwrap_or_default();
        s.add(
            "SV-DEP-006",
            "NonAnsiPortList",
            Severity::Info,
            format!(
                "Routine '{name}' uses obsolete non-ANSI port declarations inside the routine body. \
                 Declare argument directions and types directly in the routine header (e.g. 'task {name}(input bit [7:0] data, output bit err)')."
            ),
            c.loc(first),
        );
    }
}
