//! UVM-CFG-008 (ScatteredConfigKeyLiteral): Flags a uvm_config_db field name
//! typed as a string literal in two or more files.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use dv_model::{Design, Loc};
use sv_syntax::NodeId;
use crate::support::routine_bodies::{bodies, calls_in, is_config_db_call, Calls};
use crate::support::class_roles::{key, Index, Typedefs};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

/// `uvm_config_db#(T)::<method>(...)` written outside a class, where the
/// model has no calls: the scope names uvm_config_db (or a typedef of it).
fn syntactic_config_db_call(c: &Cst, td: &Typedefs, call: NodeId, method: &str) -> bool {
    if c.callee_name(call) != method {
        return false;
    }
    let Some(callee) = c.tree.children(call).next() else { return false };
    if c.kind(callee) != ScopedExpr {
        return false;
    }
    let Some(scope) = c.tree.children(callee).next() else { return false };
    let mut name = c.tree.bare_text(scope);
    for _ in 0..8 {
        let base = name.split('#').next().unwrap_or("").to_string();
        if base == "uvm_config_db" || base == "uvm_pkg::uvm_config_db" {
            return true;
        }
        match td.get(&base) {
            Some(t) => name = t.clone(),
            None => return false,
        }
    }
    false
}

/// Calls outside class methods: in free routines and module-level code.
fn outside_class_calls(c: &Cst) -> Vec<NodeId> {
    c.nodes(CallExpr)
        .into_iter()
        .filter(|&n| {
            let mut p = c.parent(n);
            while let Some(x) = p {
                if c.kind(x) == ClassDecl {
                    return false;
                }
                p = c.parent(x);
            }
            true
        })
        .collect()
}

pub(crate) fn scattered_config_key_literal(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    // key -> (file, offset, loc)
    let mut keys: BTreeMap<String, Vec<(String, u32, Loc)>> = BTreeMap::new();
    let mut seen = HashSet::new();
    let mut add = |c: &Cst, call: NodeId, keys: &mut BTreeMap<String, Vec<(String, u32, Loc)>>| {
        let args = c.call_args(call);
        let Some(k) = args.get(2).and_then(|a| c.arg_expr(*a)).and_then(|e| c.string_value(e)) else { return };
        let Some(first) = c.first_tok(call) else { return };
        let tok = &c.tree.tokens[first as usize];
        let loc = c.loc(first);
        if !seen.insert(key(loc)) || k.is_empty() || k == "*" {
            return;
        }
        // Where the call was written, after macro expansion.
        let (file, line) = loc.expansion.unwrap_or((loc.file, loc.line));
        let name = d.files().get(file as usize).cloned().unwrap_or_default();
        if name.contains("uvm_mock.svh") || name.rsplit(['/', '\\']).next().is_some_and(|b| b.starts_with("uvm_")) {
            return;
        }
        let _ = line;
        keys.entry(k).or_default().push((name, tok.origin.offset, loc));
    };
    for (_, m, c, body) in bodies(d, &ix) {
        let calls = Calls::new(m);
        for call in calls_in(c, body) {
            if is_config_db_call(&calls, c, call, "set") || is_config_db_call(&calls, c, call, "get") {
                add(c, call, &mut keys);
            }
        }
    }
    for c in csts {
        for call in outside_class_calls(c) {
            if syntactic_config_db_call(c, &ix.typedefs, call, "set") || syntactic_config_db_call(c, &ix.typedefs, call, "get") {
                add(c, call, &mut keys);
            }
        }
    }
    for (k, mut uses) in keys {
        let files: BTreeSet<String> = uses.iter().map(|u| u.0.clone()).collect();
        if files.len() < 2 {
            continue;
        }
        uses.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
        let mut others = String::new();
        let mut n = 0;
        for f in &files {
            if *f == uses[0].0 {
                continue;
            }
            if n == 3 {
                others.push_str(", ...");
                break;
            }
            n += 1;
            if !others.is_empty() {
                others.push_str(", ");
            }
            others.push_str(f.rsplit(['/', '\\']).next().unwrap_or(f));
        }
        out.add(
            "UVM-CFG-008",
            "ScatteredConfigKeyLiteral",
            Severity::Warning,
            format!(
                "The config_db field name \"{k}\" is typed as a literal {} times in {} files (also in {others}). If one \
spelling changes, get() quietly stops finding the value. Declare it once, for example as a localparam string in the agent's package, and use that name.",
                uses.len(),
                files.len()
            ),
            uses[0].2,
        );
    }
}
