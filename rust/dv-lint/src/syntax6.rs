//! Syntax rules, sixth batch (builder slice N6.3): assertions, coverage,
//! constraints and sequence starts. See [`crate::syntax`].

use sv_syntax::{Child, NodeId};

use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-COV-006", name: "CovergroupCrossBinExplosion", check: Syntax(cross_explosion) },
        Rule { id: "SV-COV-010", name: "CrossWithAllBinsIgnored", check: Syntax(cross_all_ignored) },
        Rule { id: "SV-COV-012", name: "WideCoverpointWithoutBins", check: Syntax(coverpoint_without_bins) },
        Rule { id: "UVM-PHS-004", name: "SequenceInFunctionPhase", check: Syntax(sequence_in_function_phase) },
        Rule { id: "SVA-VAC-001", name: "SvaVacuousAntecedent", check: Syntax(vacuous_antecedent) },
        Rule { id: "SVA-RST-001", name: "DisableIffReset", check: Syntax(disable_iff_reset) },
        Rule { id: "SV-PRF-012", name: "UnboundedDynamicArrayInConstraint", check: Syntax(unbounded_size) },
        Rule { id: "UVM-SEQ-012", name: "SequenceStartOnNullSequencer", check: Syntax(start_on_null) },
    ]
}

fn kids(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.children(n).collect()
}

/// slang's `toString()` reduced to its last line, trimmed.
fn last_line(text: &str) -> &str {
    text.rsplit('\n').next().unwrap_or("").trim_matches([' ', '\t', '\r'])
}

/// A coverpoint's or cross's label.
fn label(c: &Cst, n: NodeId) -> Option<String> {
    let l = c.tree.child(n, StmtLabel)?;
    c.toks(l).next().map(|t| c.text_of(t).to_string())
}

/// A cross: the crossed items, its `{ }` members and its keyword.
fn cross_parts(c: &Cst, n: NodeId) -> (usize, Vec<NodeId>, Option<u32>) {
    let mut items = 0;
    let mut members = Vec::new();
    let mut in_body = false;
    for k in c.kids(n) {
        match *k {
            Child::Token(t) if c.text_of(t) == "{" => in_body = true,
            Child::Node(x) if in_body => members.push(x),
            Child::Node(x) if !matches!(c.kind(x), StmtLabel | Attribute | EventExpr) => items += 1,
            _ => {}
        }
    }
    (items, members, c.tok(n, "cross"))
}

fn cross_explosion(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Cross) {
        let (items, members, kw) = cross_parts(c, n);
        if items < 3 {
            continue;
        }
        let filtered = members.iter().any(|&m| {
            let t = c.text(m);
            t.contains("ignore_bins") || t.contains("binsof") || t.contains("illegal_bins")
        });
        let Some(kw) = kw.filter(|_| !filtered) else { continue };
        let name = label(c, n).unwrap_or_else(|| "unnamed_cross".into());
        s.add(
            "SV-COV-006",
            "CovergroupCrossBinExplosion",
            Severity::Error,
            format!(
                "Covergroup cross '{name}' crosses {items} coverpoints without 'ignore_bins' or 'binsof' filters. \
                 Unfiltered 3-way or higher cross coverage creates a combinatorial explosion of auto-bins, \
                 exhausting simulator memory and bloating coverage databases. \
                 Apply 'ignore_bins' or 'binsof' expressions to exclude unreachable or irrelevant cross combinations."
            ),
            c.loc(kw),
        );
    }
}

fn cross_all_ignored(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Cross) {
        let (_, members, kw) = cross_parts(c, n);
        let bins: Vec<NodeId> = members.into_iter().filter(|&m| c.kind(m) == Bins).collect();
        let ignored = bins.iter().filter(|&&b| c.keyword(b).is_some_and(|k| c.text_of(k) == "ignore_bins")).count();
        if bins.is_empty() || ignored != bins.len() {
            continue;
        }
        let Some(kw) = kw else { continue };
        let name = label(c, n).unwrap_or_else(|| "anonymous_cross".into());
        s.add(
            "SV-COV-010",
            "CrossWithAllBinsIgnored",
            Severity::Warning,
            format!(
                "Covergroup cross '{name}' defines only 'ignore_bins' with zero positive coverage bins. Masking all cross combinations produces a degenerate 0-bin cross that cannot collect coverage closure data."
            ),
            c.loc(kw),
        );
    }
}

fn coverpoint_without_bins(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Coverpoint) {
        let mut expr = None;
        let mut bins = 0;
        let mut auto_max = false;
        let mut in_body = false;
        for k in c.kids(n) {
            match *k {
                Child::Token(t) if c.text_of(t) == "{" => in_body = true,
                Child::Node(x) if in_body => match c.kind(x) {
                    Bins => bins += 1,
                    CoverageOption => auto_max |= c.text(x).contains("auto_bin_max"),
                    _ => {}
                },
                Child::Node(x) if expr.is_none() && !matches!(c.kind(x), StmtLabel | Attribute | DataType) => {
                    expr = Some(x)
                }
                _ => {}
            }
        }
        if bins > 0 || auto_max {
            continue;
        }
        let name = label(c, n).unwrap_or_else(|| "unnamed_cp".into());
        let e = expr.map(|x| c.trimmed(x)).unwrap_or_default();
        // slang anchors an implicit-typed coverpoint at its body `{` or closing `;`.
        let explicit = c.tree.child(n, DataType).is_some();
        let body = c.kids(n).iter().find_map(|k| match *k {
            Child::Token(t) if matches!(c.text_of(t), "{" | ";") => Some(t),
            _ => None,
        });
        let Some(t) = (if explicit { c.first_tok(n) } else { body.or_else(|| c.first_tok(n)) }) else { continue };
        s.add(
            "SV-COV-012",
            "WideCoverpointWithoutBins",
            Severity::Warning,
            format!(
                "Coverpoint '{name}' on expression '{e}' declares zero explicit bins and lacks 'option.auto_bin_max'. \
                 In SystemVerilog (IEEE 1800 § 19.5), coverpoints without explicit bins partition the state space into a default of 64 auto-bins. \
                 For wide vectors or integer types, this creates misleading coverage metrics where 64 arbitrary hits yield 100% closure across millions of untested values. \
                 Define explicit bins or configure 'option.auto_bin_max'."
            ),
            c.loc(t),
        );
    }
}

const FUNCTION_PHASES: &[&str] = &[
    "build_phase",
    "connect_phase",
    "end_of_elaboration_phase",
    "start_of_simulation_phase",
    "extract_phase",
    "check_phase",
    "report_phase",
    "final_phase",
];

fn sequence_in_function_phase(c: &Cst, s: &mut Sink) {
    for n in c.nodes(FunctionDecl) {
        let Some(name) = c.tree.child(n, Name).and_then(|nm| c.first_tok(nm)) else { continue };
        let f = c.text_of(name);
        if !FUNCTION_PHASES.contains(&f) {
            continue;
        }
        let body = c.text(n);
        if !body.contains(".start(") && !body.contains(".start (") {
            continue;
        }
        s.add(
            "UVM-PHS-004",
            "SequenceInFunctionPhase",
            Severity::Error,
            format!(
                "Calling sequence method '.start()' inside non-time-consuming function phase '{f}' is illegal in UVM (IEEE 1800.2 § 9.3). Function phases execute in zero simulation time, \
                 whereas sequence execution requires time-consuming clocking events and driver handshakes. \
                 Sequences must only be started in task phases such as 'run_phase' or 'main_phase'."
            ),
            c.loc(name),
        );
    }
}

fn vacuous_antecedent(c: &Cst, s: &mut Sink) {
    for n in c.nodes(SequenceExpr) {
        if c.tok(n, "|->").is_none() && c.tok(n, "|=>").is_none() {
            continue;
        }
        let Some(&left) = kids(c, n).first() else { continue };
        let ant = c.trimmed(left);
        let vacuous = matches!(ant.as_str(), "0" | "1'b0" | "'0" | "false" | "1'sb0")
            || ant.contains("&& !")
            || ant.contains("&& ~");
        let Some(t) = c.first_tok(left).filter(|_| vacuous) else { continue };
        s.add(
            "SVA-VAC-001",
            "SvaVacuousAntecedent",
            Severity::Error,
            format!(
                "Vacuous assertion implication detected: antecedent '{ant}' evaluates to constant false. \
                 In SVA (IEEE 1800 § 16.12.22), a false antecedent makes the implication evaluate to true on every cycle, \
                 causing the assertion to pass vacuously 100% of the time and masking unverified DUT logic."
            ),
            c.loc(t),
        );
    }
}

fn has_disable(c: &Cst, n: NodeId) -> bool {
    c.tree.child(n, PropertyExpr).is_some_and(|p| c.tree.child(p, DisableIff).is_some())
}

fn disable_iff_reset(c: &Cst, s: &mut Sink) {
    for n in c.nodes(PropertyDecl) {
        let Some(name) = c.tree.child(n, Name).and_then(|nm| c.first_tok(nm)) else { continue };
        if c.tree.child(n, PropertyExpr).is_none() || has_disable(c, n) {
            continue;
        }
        s.add(
            "SVA-RST-001",
            "DisableIffReset",
            Severity::Warning,
            format!(
                "Property declaration '{}' lacks a 'disable iff (!rst_n)' reset gating condition. \
                 Concurrent assertions without reset gating evaluate during power-on and warm reset cycles, \
                 causing spurious false-failure storms in simulation regressions.",
                c.text_of(name)
            ),
            c.loc(name),
        );
    }
    for n in c.nodes(ConcurrentAssert) {
        let Some(kw) = c.keyword(n) else { continue };
        let k = c.text_of(kw);
        if !matches!(k, "assert" | "assume") || c.tree.child(n, PropertyExpr).is_none() || has_disable(c, n) {
            continue;
        }
        s.add(
            "SVA-RST-001",
            "DisableIffReset",
            Severity::Warning,
            format!(
                "Inline concurrent assertion '{k} property' lacks a 'disable iff (!rst_n)' reset gating condition. \
                 Add a disable iff condition to prevent spurious assertion violations while the DUT is under reset."
            ),
            c.loc(kw),
        );
    }
}

fn unbounded_size(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ConstraintItem) {
        // slang's ExpressionConstraint: `[soft] expr;`.
        if !c.toks(n).all(|t| matches!(c.text_of(t), "soft" | ";")) || c.tok(n, ";").is_none() {
            continue;
        }
        let [e] = kids(c, n)[..] else { continue };
        let text = c.text(e);
        let expr = last_line(&text);
        if !expr.contains(".size()") {
            continue;
        }
        let upper = expr.contains('<');
        let lower = expr.contains('>') || expr.contains("inside") || expr.contains("==");
        let Some(t) = c.first_tok(e).filter(|_| upper && !lower) else { continue };
        s.add(
            "SV-PRF-012",
            "UnboundedDynamicArrayInConstraint",
            Severity::Warning,
            format!(
                "Dynamic array size constraint '{expr}' specifies an upper bound without an explicit non-negative lower bound (e.g. 'size() >= 0' or 'size() > 0'). \
                 In SystemVerilog constraint optimization (IEEE 1800 § 18.5.1), \
                 solvers must search full 32-bit signed integer negative spaces when lower bounds are omitted, resulting in \
                 exponential solver search space explosion. Constrain both bounds explicitly (e.g. 'size() inside {{[1:10]}}')."
            ),
            c.loc(t),
        );
    }
}

fn start_on_null(c: &Cst, s: &mut Sink) {
    for call in c.nodes(CallExpr) {
        let Some(f) = c.tree.children(call).next() else { continue };
        if c.last_text(f) != "start" {
            continue;
        }
        let Some(list) = c.tree.child(call, ArgList) else { continue };
        // The first argument: the node before any comma.
        let first =
            c.kids(list).iter().take_while(|k| !matches!(**k, Child::Token(t) if c.text_of(t) == ",")).find_map(|k| {
                match *k {
                    Child::Node(a) => Some(a),
                    Child::Token(_) => None,
                }
            });
        let Some(arg) = first.filter(|&a| c.kind(a) == Arg && c.tok(a, ".").is_none()) else { continue };
        let Some(e) = c.tree.children(arg).next() else { continue };
        if c.trimmed(e) != "null" {
            continue;
        }
        let callee = c.text(f);
        let seq = match callee.rfind(".start") {
            Some(p) => last_line(&callee[..p]).to_string(),
            None => "seq".to_string(),
        };
        let Some(t) = c.first_tok(e) else { continue };
        s.add(
            "UVM-SEQ-012",
            "SequenceStartOnNullSequencer",
            Severity::Warning,
            format!(
                "Sequence invocation '{seq}.start(null)' starts sequence with null sequencer handle. \
                 In UVM sequence architecture (IEEE 1800.2 § 5.8), \
                 starting a sequence on 'null' leaves 'm_sequencer' and 'p_sequencer' unattached, causing immediate \
                 fatal null-pointer crashes when the sequence attempts to send transactions (start_item/finish_item) \
                 or query sequencer configuration. Pass a valid sequencer reference (e.g. 'm_sequencer' or 'p_sequencer')."
            ),
            c.loc(t),
        );
    }
}
