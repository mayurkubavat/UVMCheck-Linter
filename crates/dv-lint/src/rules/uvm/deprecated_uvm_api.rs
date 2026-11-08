//! UVM-API-003 (DeprecatedUvmApi): Flags calls to UVM methods, and classes
//! extending UVM classes, that the UVM version being compiled keeps only as
//! deprecated API: in UVM 1.1d/1.2 what UVM_NO_DEPRECATED removes (for
//! example uvm_component::set_config_int, uvm_root::stop_request,
//! uvm_sequence_base::get_sequence_by_name, uvm_random_sequence). The list is
//! the UVM API pack's (tools/codegen/gen_uvm_api_pack.py). No uvmcheck twin:
//! a dv-lint rule.

use crate::support::defines_packages::{api_pack, user_tok, Files};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Design, Loc};
use std::collections::HashSet;
use sv_syntax::SyntaxKind::*;

pub(crate) fn deprecated_uvm_api(_: &Design, csts: &[Cst], out: &mut Sink) {
    let Some(pack) = api_pack(csts) else { return };
    if pack.deprecated_members.is_empty() && pack.deprecated_classes.is_empty() {
        return;
    }
    let why = if pack.removed_by_define() {
        "the library drops it when UVM_NO_DEPRECATED is defined, and later UVM releases do not have it"
    } else {
        "the library compiles it only when UVM_ENABLE_DEPRECATED_API is set"
    };
    let report = |out: &mut Sink, what: String, where_: &str, loc: Loc| {
        out.add(
            "UVM-API-003",
            "DeprecatedUvmApi",
            Severity::Warning,
            format!(
                "{what} ({where_}) is deprecated in UVM {}: {why}. Move to the current API (uvm_config_db, objections, \
uvm_sequence::start) before upgrading UVM.",
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
            let Some(cls) = crate::support::slang_ast::ancestors(c, decl).find(|&a| c.kind(a) == ClassDecl) else {
                continue;
            };
            let cname = crate::support::slang_ast::decl_name(c, cls);
            let member = crate::support::slang_ast::last_name(c, decl);
            // `super.new` of a deprecated class: the class finding says it.
            if member == "new"
                || !cname.starts_with("uvm_")
                || !pack.deprecated_members.contains(&(cname.clone(), member.clone()))
            {
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
            if !base.starts_with("uvm_") || !pack.deprecated_classes.contains(&base) {
                continue;
            }
            let o = c.tree.tokens[nt as usize].origin;
            if !seen.insert((fs.path(o.file), o.offset)) {
                continue;
            }
            let name = c.tree.bare_text(nm);
            report(out, format!("class '{base}'"), &format!("'{name}' extends it"), c.loc(nt));
        }
    }
}
