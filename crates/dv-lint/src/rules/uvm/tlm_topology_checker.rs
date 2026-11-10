//! UVM-TLM-004 (TlmTopologyChecker): Performs cross-component elaboration
//! topology checks to verify that mandatory TLM ports (such as driver
//! seq_item_pull_port and monitor analysis_port) are bound in connect_phase
//! across the verification environment hierarchy.

use crate::support::uvm_classes::{is_component, Ix};
use crate::support::uvm_syntax::{callee, decl_name};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Design, Loc};
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// `swe::normalizedText`: tokens with any trivia between them as one space.
fn normalized(c: &Cst, n: NodeId) -> String {
    let mut out = String::new();
    for t in c.tree.token_range(n) {
        let tok = &c.tree.tokens[t as usize];
        if tok.text.is_empty() {
            continue;
        }
        let has_trivia = if tok.from_macro { tok.space_before } else { !c.tree.trivia(t).is_empty() };
        if !out.is_empty() && has_trivia {
            out.push(' ');
        }
        out.push_str(&tok.text);
    }
    out
}

pub(crate) fn tlm_topology(d: &Design, csts: &[Cst], s: &mut Sink) {
    struct Port<'a> {
        class: &'a str,
        name: &'a str,
        pull: bool,
        loc: Loc,
    }
    let ix = Ix::new(d, csts);
    let mut ports = Vec::new();
    for cls in ix.model_classes() {
        if !is_component(d, cls.id) {
            continue;
        }
        for p in &cls.properties {
            let pull = p.ty.contains("seq_item_pull_port");
            let analysis = p.ty.contains("analysis_port");
            if pull || analysis {
                ports.push(Port { class: &cls.name, name: &p.name, pull, loc: p.loc });
            }
        }
    }
    if ports.is_empty() {
        return;
    }
    let conns: Vec<(String, String)> = connections(csts).into_iter().map(|k| (k.source, k.target)).collect();
    for p in ports {
        if conns.iter().any(|(a, b)| a.contains(p.name) || b.contains(p.name)) {
            continue;
        }
        let kind = if p.pull { "Driver pull-port" } else { "Analysis port" };
        s.add(
            "UVM-TLM-004",
            "TlmTopologyChecker",
            Severity::Warning,
            format!(
                "{kind} '{}' in component '{}' is declared but never connected to any export or sequencer in connect_phase across the environment. \
                 Unconnected ports will cause null-pointer dereferences or dropped verification transactions.",
                p.name, p.class
            ),
            p.loc,
        );
    }
}

/// A `connect()` call outside the UVM library: `a.b.port.connect(c.imp)`
/// is source `a.b.port`, target `c.imp`, as uvmcheck's TlmTopologyGraph.
pub(crate) struct Connection {
    /// The class the call is in ("" outside a class).
    pub(crate) class: String,
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) loc: Loc,
}

pub(crate) fn connections(csts: &[Cst]) -> Vec<Connection> {
    let mut out = Vec::new();
    for c in csts {
        for n in c.nodes(CallExpr) {
            if callee(c, n) != "connect" || in_library_class(c, n) {
                continue;
            }
            let text = normalized(c, n);
            let (mut src, mut dst) = (text.clone(), String::new());
            if let Some(at) = text.rfind(".connect") {
                src = text[..at].to_string();
                if let (Some(open), Some(close)) = (text[at..].find('(').map(|i| i + at), text.rfind(')')) {
                    if close > open {
                        dst = text[open + 1..close].to_string();
                    }
                }
            }
            let class = crate::support::slang_ast::enclosing_class(c, n)
                .map(|k| crate::support::slang_ast::decl_name(c, k))
                .unwrap_or_default();
            let loc = c.first_tok(n).map(|t| c.loc(t)).unwrap_or_default();
            out.push(Connection {
                class,
                source: src.trim_matches(' ').to_string(),
                target: dst.trim_matches(' ').to_string(),
                loc,
            });
        }
    }
    out
}

/// Inside a class of the UVM library (or the test stub): `isLibrarySymbol`
/// on the innermost class.
fn in_library_class(c: &Cst, n: NodeId) -> bool {
    let mut cur = Some(n);
    let mut in_class = false;
    while let Some(p) = cur {
        match c.kind(p) {
            ClassDecl => {
                in_class = true;
                if decl_name(c, p).starts_with("uvm_") {
                    return true;
                }
            }
            PackageDecl if decl_name(c, p) == "uvm_pkg" => return in_class,
            _ => {}
        }
        cur = c.parent(p);
    }
    false
}
