//! UVM-TLM-007 (UncoveredAnalysisPort): Flags a transaction type that
//! analysis ports publish but that no coverage collector receives.
//!
//! A collector is a class that has a covergroup (itself, a class it extends,
//! or a class one of its properties holds) and receives the type: it extends
//! `uvm_subscriber #(T)`, or holds an analysis imp, export or FIFO, or a get
//! or peek port, of `T`. When a testbench collects coverage at all (some
//! covergroup is declared in user code) and nothing like that exists for a
//! type its monitors publish, that interface is not covered: its bins are
//! never fed, and the hole does not show in the coverage report as a 0% group,
//! since no group exists. Only types a monitor publishes count (a driver's
//! or agent's copy of what it sent is not observed traffic). One finding per
//! type, at the monitor's port, naming every port of that type.

use std::collections::{BTreeMap, HashSet};

use crate::support::slang_ast::{decl_name, enclosing_class};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Class, Design, UvmKind};
use sv_syntax::SyntaxKind::*;

/// `pkg::uvm_analysis_port#(pkg::item_c)` -> (`uvm_analysis_port`, `item_c`).
fn tlm_type(ty: &str) -> Option<(String, String)> {
    let ty: String = ty.chars().filter(|ch| !ch.is_whitespace()).collect();
    let (base, rest) = ty.split_once("#(")?;
    let base = base.rsplit("::").next()?.to_string();
    let mut depth = 0;
    let mut end = rest.len();
    for (i, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' if depth == 0 => {
                end = i;
                break;
            }
            ')' => depth -= 1,
            ',' if depth == 0 => {
                end = i;
                break;
            }
            _ => {}
        }
    }
    let arg = &rest[..end];
    let arg = arg.split_once('=').map_or(arg, |(name, v)| if name.contains(['(', '#']) { arg } else { v });
    Some((base, norm(arg)))
}

/// A type name without its package (`pkg::item_c` -> `item_c`).
fn norm(t: &str) -> String {
    let t: String = t.chars().filter(|ch| !ch.is_whitespace()).collect();
    match t.split_once('#') {
        Some((name, params)) => format!("{}#{params}", name.rsplit("::").next().unwrap_or(name)),
        None => t.rsplit("::").next().unwrap_or(&t).to_string(),
    }
}

/// The TLM classes a collector receives a transaction through.
fn receives(base: &str) -> bool {
    base == "uvm_analysis_imp"
        || base.starts_with("uvm_analysis_imp_")
        || matches!(base, "uvm_analysis_export" | "uvm_tlm_analysis_fifo" | "uvm_tlm_fifo")
        || (base.starts_with("uvm_") && (base.ends_with("get_port") || base.ends_with("peek_port")))
}

/// `k` and the classes it extends that are in the model.
fn chain<'d>(d: &'d Design, k: &'d Class) -> impl Iterator<Item = &'d Class> {
    let mut cur = Some(k);
    let mut n = 0;
    std::iter::from_fn(move || {
        let x = cur?;
        n += 1;
        cur = x.base.map(|b| d.class(b)).filter(|_| n < 64);
        Some(x)
    })
}

pub(crate) fn uncovered_analysis_ports(d: &Design, csts: &[Cst], out: &mut Sink) {
    let user: HashSet<&str> = d.classes().iter().filter(|k| !k.is_library).map(|k| k.name.as_str()).collect();
    // Classes with a covergroup; and whether user code declares any.
    let mut with_cg: HashSet<String> = HashSet::new();
    let mut any = false;
    // `class c extends uvm_subscriber #(T)`: c -> T.
    let mut subscribes: BTreeMap<String, String> = BTreeMap::new();
    for c in csts {
        for g in c.nodes(CovergroupDecl) {
            match enclosing_class(c, g) {
                Some(k) => {
                    let name = decl_name(c, k);
                    if user.contains(name.as_str()) {
                        any = true;
                        with_cg.insert(name);
                    }
                }
                None => any |= !c.tree.library,
            }
        }
        for k in c.nodes(ClassDecl) {
            let Some(ext) = c.tree.child(k, Header).and_then(|h| c.tree.child(h, Extends)) else { continue };
            let text: String = c.trimmed(ext).trim_start_matches("extends").to_string();
            if let Some((base, arg)) = tlm_type(&text) {
                if base == "uvm_subscriber" {
                    subscribes.insert(decl_name(c, k), arg);
                }
            }
        }
    }
    if !any {
        return;
    }
    let has_cg = |k: &Class| {
        chain(d, k).any(|x| {
            with_cg.contains(&x.name)
                || x.properties.iter().any(|p| p.type_class.is_some_and(|t| with_cg.contains(&d.class(t).name)))
        })
    };
    let mut covered: HashSet<String> = HashSet::new();
    for k in d.classes().iter().filter(|k| has_cg(k)) {
        for x in chain(d, k) {
            covered.extend(subscribes.get(&x.name).cloned());
            for p in &x.properties {
                if let Some((base, arg)) = tlm_type(&p.ty) {
                    if receives(&base) {
                        covered.insert(arg);
                    }
                }
            }
        }
    }
    // Transaction type -> its ports: (not a monitor, class, port, loc). Not
    // the UVM library's own classes, nor a parameterized class's ports (their
    // type is a parameter's), nor a type that is no class of the model.
    let classes: HashSet<&str> = d.classes().iter().map(|k| k.name.as_str()).collect();
    let mut ports: BTreeMap<String, Vec<(bool, String, String, dv_model::Loc)>> = BTreeMap::new();
    for k in d.classes().iter().filter(|k| !k.is_library && !k.is_parameterized && !k.name.starts_with("uvm_")) {
        for p in &k.properties {
            let Some((base, arg)) = tlm_type(&p.ty) else { continue };
            if base != "uvm_analysis_port"
                || arg.starts_with("uvm_")
                || covered.contains(&arg)
                || !classes.contains(arg.split('#').next().unwrap_or(&arg))
            {
                continue;
            }
            ports.entry(arg).or_default().push((k.uvm != UvmKind::Monitor, k.name.clone(), p.name.clone(), p.loc));
        }
    }
    // What a monitor publishes: a driver's or agent's copy of what it sent is
    // not the interface's observed traffic.
    ports.retain(|_, list| list.iter().any(|p| !p.0));
    for (ty, mut list) in ports {
        list.sort_by(|a, b| (a.0, &a.1, &a.2).cmp(&(b.0, &b.1, &b.2)));
        let names: Vec<String> = list.iter().map(|(_, k, p, _)| format!("{k}.{p}")).collect();
        out.add(
            "UVM-TLM-007",
            "UncoveredAnalysisPort",
            Severity::Warning,
            format!(
                "Transactions of type '{ty}' are published on {} but no coverage collector receives them: no class with a \
                 covergroup subscribes to '{ty}' (uvm_subscriber, analysis imp, export or FIFO). Its transactions have no \
                 functional coverage, and the hole does not show as a 0% covergroup in the report. (Coverage sampled from \
                 the interface's signals by a bound module does not count here; waive the finding if that is the plan.)",
                names.join(", ")
            ),
            list[0].3,
        );
    }
}
