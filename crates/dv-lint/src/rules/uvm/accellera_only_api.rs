//! UVM-API-001 (AccelleraOnlyApi): Flags calls to, and classes extending, parts
//! of the Accellera UVM library that IEEE 1800.2 does not define (tagged
//! @uvm-accellera in the library source), such as uvm_component::print_config
//! or uvm_port_base::debug_connected_to.

use std::collections::HashSet;
use dv_model::{Design, Loc};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::defines_packages::{Files, api_pack, user_tok};

pub(crate) fn accellera_only_api(_: &Design, csts: &[Cst], out: &mut Sink) {
    let Some(pack) = api_pack(csts) else { return };
    let report = |out: &mut Sink, what: String, where_: &str, loc: Loc| {
        out.add(
            "UVM-API-001",
            "AccelleraOnlyApi",
            Severity::Info,
            format!(
                "{what} ({where_}) is a detail of the Accellera UVM library (tagged @uvm-accellera in UVM {}), not part of IEEE 1800.2. \
Another UVM implementation, or a later Accellera release, need not have it. Keep such calls in one debug helper, or use the standard API.",
                pack.version
            ),
            loc,
        );
    };
    let mut seen = HashSet::new();
    for c in csts {
        let fs = Files::new(c);
        for e in c.nodes(CallExpr) {
            let Some(t) = c.first_tok(e) else { continue };
            if c.text_of(t).starts_with('$') || !user_tok(c, &fs, t) {
                continue;
            }
            let Some(decl) = crate::support::slang_ast::callee_decl(c, e) else { continue };
            if !matches!(c.kind(decl), FunctionDecl | TaskDecl | MethodPrototype) {
                continue;
            }
            let Some(cls) = crate::support::slang_ast::ancestors(c, decl).find(|&a| c.kind(a) == ClassDecl) else { continue };
            let cname = crate::support::slang_ast::decl_name(c, cls);
            let member = crate::support::slang_ast::last_name(c, decl);
            if !cname.starts_with("uvm_") || !pack.accellera_members.contains(&(cname.clone(), member.clone())) {
                continue;
            }
            let o = c.tree.tokens[t as usize].origin;
            if !seen.insert((fs.path(o.file), o.offset)) {
                continue;
            }
            report(out, format!("'{cname}::{member}'"), "called here", c.loc(t));
        }
        for k in c.nodes(ClassDecl) {
            let Some(nm) = c.tree.child(k, Name) else { continue };
            let Some((_, nt)) = c.tree.token_bounds(nm) else { continue };
            if !user_tok(c, &fs, nt) {
                continue;
            }
            // The base class's (generic) name.
            let Some(bt) = crate::support::uvm_syntax::base_name_node(c, k) else { continue };
            let Some(base) = c
                .toks(bt)
                .map(|t| c.text_of(t))
                .take_while(|t| *t != "#")
                .filter(|t| t.chars().next().is_some_and(|ch| ch.is_alphabetic() || ch == '_'))
                .last()
                .map(str::to_string)
            else {
                continue;
            };
            if !pack.accellera_classes.contains(&base) {
                continue;
            }
            let name = c.tree.bare_text(nm);
            for _ in 0..crate::support::style_config::ast_visits(c, nm) {
                report(out, format!("class '{base}'"), &format!("'{name}' extends it"), c.loc(nt));
            }
        }
    }
}
