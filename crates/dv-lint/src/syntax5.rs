//! Syntax rules, fifth batch (builder slice N6.3): UVM call patterns,
//! vendor tasks, streaming slices and class design smells.
//! See [`crate::syntax`].

use sv_syntax::{Child, NodeId};

use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "PORT-PRP-001", name: "ProprietaryVendorCalls", check: Syntax(vendor_calls) },
        Rule { id: "UVM-CFG-003", name: "UvmConfigDbGlobalWildcard", check: Syntax(config_db_wildcard) },
        Rule { id: "UVM-CFG-004", name: "ConfigDbGetUnchecked", check: Syntax(config_db_get_unchecked) },
        Rule { id: "UVM-TLM-002", name: "UvmTlmTryGetUnchecked", check: Syntax(tlm_try_unchecked) },
        Rule { id: "UVM-STR-005", name: "ComponentNullParentInCreate", check: Syntax(null_parent_create) },
        Rule { id: "SV-LSP-001", name: "LspConstraintContradiction", check: Syntax(lsp_constraint_mode) },
        Rule { id: "SV-OCP-001", name: "CastTypeInspectionLadder", check: Syntax(cast_ladder) },
        Rule { id: "SV-STR-001", name: "StreamingConcatenationSizeMismatch", check: Syntax(stream_slice) },
    ]
}

fn kids(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.children(n).collect()
}

/// A call's arguments as slang counts them: one per comma-separated slot,
/// `None` for an empty one; none at all for `()`.
fn args(c: &Cst, call: NodeId) -> Vec<Option<NodeId>> {
    let Some(list) = c.tree.child(call, ArgList) else { return Vec::new() };
    let mut out = vec![None];
    for k in c.kids(list) {
        match *k {
            Child::Token(t) if c.text_of(t) == "," => out.push(None),
            Child::Node(a) => *out.last_mut().unwrap() = Some(a),
            _ => {}
        }
    }
    if out.len() == 1 && out[0].is_none() {
        out.clear();
    }
    out
}

/// An ordered (not named) argument's expression.
fn ordered(c: &Cst, a: Option<NodeId>) -> Option<NodeId> {
    let a = a?;
    if c.kind(a) != Arg || c.tok(a, ".").is_some() {
        return None;
    }
    c.tree.children(a).next()
}

fn callee(c: &Cst, call: NodeId) -> Option<NodeId> {
    c.tree.children(call).next()
}

const VENDOR: &[&str] = &[
    "vcdpluson",
    "vcdplusoff",
    "vcdplusmemon",
    "vcdplusmemoff",
    "shm_open",
    "shm_close",
    "shm_probe",
    "fsdbDumpvars",
    "fsdbDumpfile",
    "fsdbDumpflush",
    "wlfdump",
    "wlfdumpvars",
];

fn vendor_calls(c: &Cst, s: &mut Sink) {
    for n in c.nodes(NameExpr) {
        let Some((a, b)) = c.tree.token_bounds(n) else { continue };
        let Some(name) = c.text_of(a).strip_prefix('$') else { continue };
        if a != b || !VENDOR.contains(&name) {
            continue;
        }
        s.add(
            "PORT-PRP-001",
            "ProprietaryVendorCalls",
            Severity::Warning,
            format!(
                "Proprietary simulator-specific system task '${name}' detected. \
                 Vendor-proprietary simulation tasks break VIP portability across simulators. \
                 Wrap proprietary calls in a simulator-specific `ifdef guard or use standard IEEE 1800 dump tasks ($dumpvars, $dumpfile)."
            ),
            c.loc(a),
        );
    }
}

fn config_db_wildcard(c: &Cst, s: &mut Sink) {
    for call in c.nodes(CallExpr) {
        let Some(f) = callee(c, call) else { continue };
        let text = c.text(f);
        if !text.contains("uvm_config_db") || !text.contains("::set") {
            continue;
        }
        let a = args(c, call);
        if a.len() < 2 {
            continue;
        }
        let arg = |x: Option<NodeId>| x.map(|n| c.trimmed(n)).unwrap_or_default();
        let (a0, a1) = (arg(a[0]), arg(a[1]));
        if a0 != "null" || (a1 != "\"*\"" && a1 != "\"\"") {
            continue;
        }
        let Some(t) = c.first_tok(call) else { continue };
        s.add(
            "UVM-CFG-003",
            "UvmConfigDbGlobalWildcard",
            Severity::Warning,
            format!(
                "Unbounded global config_db scope: uvm_config_db::set(null, {a1}, ...) sets configuration globally with no hierarchical context. \
                 This forces every component in the testbench hierarchy to perform regex string matching during build_phase, severely degrading simulation startup performance. Provide targeted scope (e.g. 'uvm_test_top.env.*')."
            ),
            c.loc(t),
        );
    }
}

/// The call an expression statement makes, directly or under a cast
/// (`void'(...)` is slang's own statement kind, not a cast).
fn statement_calls(c: &Cst) -> Vec<NodeId> {
    let mut out = Vec::new();
    for st in c.nodes(ExprStmt) {
        let Some(e) = c.tree.children(st).next() else { continue };
        let call = match c.kind(e) {
            CallExpr => Some(e),
            CastExpr => {
                let k = kids(c, e);
                let signed = k.first().is_some_and(|&t| matches!(c.tree.bare_text(t).as_str(), "signed" | "unsigned" | "void"));
                k.last().copied().filter(|&x| !signed && k.len() == 2 && c.kind(x) == CallExpr)
            }
            _ => None,
        };
        out.extend(call);
    }
    out
}

fn config_db_get_unchecked(c: &Cst, s: &mut Sink) {
    for call in statement_calls(c) {
        let Some(f) = callee(c, call) else { continue };
        if c.last_text(f) != "get" || !c.text(f).contains("uvm_config_db") {
            continue;
        }
        let Some(t) = c.first_tok(call) else { continue };
        s.add(
            "UVM-CFG-004",
            "ConfigDbGetUnchecked",
            Severity::Warning,
            "Calling 'uvm_config_db::get()' without checking its return boolean status risks silent uninitialized state or null-pointer dereferences if the field is not found. Check the return value in an 'if' statement (e.g. 'if (!uvm_config_db#(...)::get(...)) `uvm_fatal(...)').".to_string(),
            c.loc(t),
        );
    }
}

fn tlm_try_unchecked(c: &Cst, s: &mut Sink) {
    for call in statement_calls(c) {
        let Some(f) = callee(c, call) else { continue };
        let m = c.last_text(f);
        if !matches!(m, "try_get" | "try_peek" | "try_put") {
            continue;
        }
        let Some(t) = c.first_tok(call) else { continue };
        s.add(
            "UVM-TLM-002",
            "UvmTlmTryGetUnchecked",
            Severity::Warning,
            format!(
                "Calling non-blocking TLM method '.{m}()' without checking its return boolean status causes silent transaction drops when the channel is empty or full. Check the return status in an 'if' statement (e.g. 'if (!port.{m}(tr)) ...')."
            ),
            c.loc(t),
        );
    }
}

fn null_parent_create(c: &Cst, s: &mut Sink) {
    for call in c.nodes(CallExpr) {
        let Some(f) = callee(c, call) else { continue };
        let text = c.text(f);
        if !text.contains("::type_id::create") && !text.contains(".create") {
            continue;
        }
        let a = args(c, call);
        let Some(e) = a.get(1).and_then(|&x| ordered(c, x)) else { continue };
        if c.trimmed(e) != "null" {
            continue;
        }
        let whole = c.text(call);
        let call_text = whole.rsplit('\n').next().unwrap_or("").trim_matches([' ', '\t', '\r']);
        let Some(t) = c.first_tok(e) else { continue };
        s.add(
            "UVM-STR-005",
            "ComponentNullParentInCreate",
            Severity::Warning,
            format!(
                "Component factory creation '{call_text}' passes 'null' as the parent component handle. In UVM methodology (IEEE 1800.2 § 4.3 and \
                 IEEE 1800.2 § 4.3), sub-components must specify 'this' as parent to properly integrate into \
                 the component hierarchy. Passing 'null' creates an orphaned component excluded from phase \
                 traversal, topology reporting, and hierarchical uvm_config_db lookup."
            ),
            c.loc(t),
        );
    }
}

fn lsp_constraint_mode(c: &Cst, s: &mut Sink) {
    for cls in c.nodes(ClassDecl) {
        let Some(ext) = c.tree.child(cls, Header).and_then(|h| c.tree.child(h, Extends)) else { continue };
        let Some(base) = c.tree.children(ext).next() else { continue };
        let class_name = c.tree.child(cls, Name).map(|n| c.tree.bare_text(n)).unwrap_or_default();
        // The base name's last token, without its parameters.
        let base_name = c
            .tree
            .children(base)
            .find(|&k| c.kind(k) == ParamList)
            .and_then(|p| c.first_tok(p))
            .map_or_else(|| c.last_text(base).to_string(), |p| c.text_of(p - 1).to_string());
        for m in c.items(cls).into_iter().filter(|&m| !matches!(c.kind(m), Name | Header)) {
            let text = c.text(m);
            if !text.contains("constraint_mode(0)") && !text.contains("constraint_mode(1'b0)") {
                continue;
            }
            let Some(t) = c.first_tok(m) else { continue };
            s.add(
                "SV-LSP-001",
                "LspConstraintContradiction",
                Severity::Error,
                format!(
                    "Derived class '{class_name}' unconditionally disables an inherited constraint via '.constraint_mode(0)'. \
                     This violates the Liskov Substitution Principle (LSP); components receiving base class '{base_name}' references expect base random contracts to hold. \
                     Refactor base constraints using 'soft' constraints or constraint overriding instead of brute-force disabling."
                ),
                c.loc(t),
            );
        }
    }
}

fn cast_ladder(c: &Cst, s: &mut Sink) {
    for n in c.nodes(IfStmt) {
        if c.parent(n).is_some_and(|p| c.kind(p) == ElseClause) {
            continue;
        }
        let mut casts = 0;
        let mut cur = Some(n);
        while let Some(i) = cur {
            if kids(c, i).first().is_some_and(|&p| c.text(p).contains("$cast")) {
                casts += 1;
            }
            cur = c.tree.child(i, ElseClause).and_then(|e| c.tree.children(e).next()).filter(|&x| c.kind(x) == IfStmt);
        }
        if casts < 3 {
            continue;
        }
        let Some(t) = c.tok(n, "if") else { continue };
        s.add(
            "SV-OCP-001",
            "CastTypeInspectionLadder",
            Severity::Warning,
            format!(
                "Cascading dynamic cast ladder ($cast) detected with {casts} branches. \
                 Type inspection ladders violate the Open-Closed Principle (OCP) and bypass polymorphic dispatch. \
                 Introducing new subclasses requires modifying each ladder across the testbench. Refactor common branch operations into virtual methods on the base class."
            ),
            c.loc(t),
        );
    }
}

fn stream_slice(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ConcatExpr) {
        let Some(op) = c.tok(n, "<<").or_else(|| c.tok(n, ">>")) else { continue };
        // A slice size is a node between the operator and the inner `{`.
        let mut after_op = false;
        let mut sliced = false;
        for k in c.kids(n) {
            match *k {
                Child::Token(t) if t == op => after_op = true,
                Child::Token(t) if after_op && c.text_of(t) == "{" => break,
                Child::Node(_) if after_op => sliced = true,
                _ => {}
            }
        }
        if sliced {
            continue;
        }
        let o = c.text_of(op);
        s.add(
            "SV-STR-001",
            "StreamingConcatenationSizeMismatch",
            Severity::Warning,
            format!(
                "Streaming operator '{o}' omits explicit slice size ('{{{o} {{ ... }}}}'). \
                 Per IEEE 1800 § 11.4.14, omitting slice size creates an unbounded single-slice stream across operands, risking silent truncation or endianness misalignments when packing or unpacking composite types and queues. \
                 Specify an explicit slice width (e.g. '{{{o} 8 {{ ... }}}}' or '{{{o} byte {{ ... }}}}')."
            ),
            c.loc(op),
        );
    }
}
