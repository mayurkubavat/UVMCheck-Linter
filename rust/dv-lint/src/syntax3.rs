//! Syntax rules, third batch (builder slice N6.3): assignments in
//! expressions, event triggers, objects made in loops, inverted ranges,
//! zero repetitions, unbounded delays and sequence priorities.
//! See [`crate::syntax`].

use sv_syntax::{Child, NodeId};

use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-COR-020", name: "ProceduralAssignmentAsSubExpression", check: Syntax(assignment_sub_expr) },
        Rule { id: "SV-CON-013", name: "BlockingEventTriggerInClockedBlock", check: Syntax(blocking_trigger) },
        Rule { id: "SV-OOP-010", name: "ClassInstantiationInLoop", check: Syntax(new_in_loop) },
        Rule { id: "SV-RND-021", name: "ConstraintInsideInvertedRange", check: Syntax(inverted_range) },
        Rule { id: "SV-SVA-004", name: "NonConsecutiveRepetitionZeroBound", check: Syntax(zero_repetition) },
        Rule { id: "SVA-CYC-001", name: "SvaUnboundedDelay", check: Syntax(unbounded_delay) },
        Rule { id: "UVM-SEQ-007", name: "SequencePriorityRange", check: Syntax(sequence_priority) },
    ]
}

/// slang's `toString()` of the tokens `a..=b`.
fn span(c: &Cst, a: u32, b: u32) -> String {
    let mut s = String::new();
    for t in a..=b {
        let tok = &c.tree.tokens[t as usize];
        if !tok.from_macro {
            s.push_str(c.tree.trivia(t));
        }
        s.push_str(&tok.text);
    }
    s
}

fn child_nodes(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.children(n).collect()
}

fn assignment_sub_expr(c: &Cst, s: &mut Sink) {
    for n in c.nodes(BinaryExpr) {
        let Some(op) = c.tok(n, "=").or_else(|| c.tok(n, "+=")).or_else(|| c.tok(n, "-=")) else { continue };
        let mut up = c.parent(n);
        while let Some(p) = up.filter(|&p| c.kind(p) == ParenExpr) {
            up = c.parent(p);
        }
        let standalone = up.is_some_and(|p| match c.kind(p) {
            ExprStmt | CoverageOption => true,
            LoopStmt => c.keyword(p).is_some_and(|k| c.text_of(k) == "for"),
            _ => false,
        });
        if standalone {
            continue;
        }
        let text = c.text(n);
        let last = text.rsplit('\n').next().unwrap_or("");
        let expr = last.trim_matches([' ', '\t', '\r']);
        s.add(
            "SV-COR-020",
            "ProceduralAssignmentAsSubExpression",
            Severity::Warning,
            format!(
                "Assignment expression '{expr}' is embedded as a sub-expression. In SystemVerilog (IEEE 1800 § 10.4), \
                 assignments should stand alone as dedicated statements rather than being embedded in conditionals \
                 or arithmetic expressions, which creates subtle order-of-evaluation race bugs and severely degrades code maintainability."
            ),
            c.loc(op),
        );
    }
}

fn blocking_trigger(c: &Cst, s: &mut Sink) {
    for n in c.nodes(EventTrigger) {
        let Some(arrow) = c.tok(n, "->") else { continue };
        let mut clocked = false;
        let mut up = c.parent(n);
        while let Some(p) = up {
            let kw = c.keyword(p).map(|k| c.text_of(k));
            if c.kind(p) == ProceduralBlock && kw == Some("always_ff") {
                clocked = true;
                break;
            }
            if c.kind(p) == TimedStmt || (c.kind(p) == ProceduralBlock && kw == Some("always")) {
                let text = c.text(p);
                if text.contains("posedge") || text.contains("negedge") {
                    clocked = true;
                    break;
                }
            }
            up = c.parent(p);
        }
        if !clocked {
            continue;
        }
        let ev = c.tree.children(n).next().map(|e| c.text(e)).unwrap_or_else(|| "event".to_string());
        s.add(
            "SV-CON-013",
            "BlockingEventTriggerInClockedBlock",
            Severity::Warning,
            format!(
                "Blocking event trigger '-> {ev}' is invoked within a sequential clocked process. In SystemVerilog (IEEE 1800 § 15.5 and \
                 IEEE 1800 § 15.5), clocked processes should trigger events using \
                 non-blocking trigger ('->> {ev}') to schedule notification in the Nonblocking Events \
                 region, eliminating zero-time race conditions with concurrent listener threads."
            ),
            c.loc(arrow),
        );
    }
}

/// A `new` that slang reads as a class instantiation: not an array or a
/// copy, and not a call argument.
fn is_class_new(c: &Cst, n: NodeId) -> bool {
    c.tok(n, "[").is_none()
        && c.tree.children(n).all(|k| c.kind(k) == ArgList)
        && !c.parent(n).is_some_and(|p| c.kind(p) == Arg)
}

fn new_in_loop(c: &Cst, s: &mut Sink) {
    let mut loops = c.nodes(LoopStmt);
    loops.extend(c.nodes(DoWhileStmt));
    loops.sort_by_key(|n| c.first_tok(*n));
    for l in loops {
        let Some(kw) = c.keyword(l) else { continue };
        let kids = child_nodes(c, l);
        let body = match c.text_of(kw) {
            "for" | "foreach" | "while" | "repeat" => kids.last(),
            "do" => kids.first(),
            _ => None,
        };
        let Some(&body) = body else { continue };
        let mut stack = vec![body];
        let mut found = Vec::new();
        while let Some(x) = stack.pop() {
            if c.kind(x) == NewExpr && is_class_new(c, x) {
                found.push(x);
            }
            stack.extend(c.tree.children(x));
        }
        found.sort_by_key(|n| c.first_tok(*n));
        for n in found {
            let Some(t) = c.first_tok(n) else { continue };
            s.add(
                "SV-OOP-010",
                "ClassInstantiationInLoop",
                Severity::Warning,
                "Direct object instantiation with 'new()' inside a simulation loop. Repeatedly creating dynamic heap objects in simulation loops triggers frequent garbage collection and memory manager lock contention. Preallocate objects outside the loop or reuse transaction instances from an object pool.".to_string(),
                c.loc(t),
            );
        }
    }
}

/// An unsized integer literal, read as `std::stoll(text, nullptr, 0)`
/// does: a leading 0 means octal, and parsing stops at the first `_`.
fn int_literal(text: &str) -> Option<i64> {
    if text.is_empty() || !text.chars().all(|ch| ch.is_ascii_digit() || ch == '_') {
        return None;
    }
    let (digits, radix) = match text.strip_prefix('0') {
        Some(rest) if !rest.is_empty() => (rest, 8),
        _ => (text, 10),
    };
    let end = digits.find(|ch: char| !ch.is_digit(radix)).unwrap_or(digits.len());
    if end == 0 {
        return (radix == 8).then_some(0);
    }
    i64::from_str_radix(&digits[..end], radix).ok()
}

fn inverted_range(c: &Cst, s: &mut Sink) {
    for n in c.nodes(OpenRange) {
        let kids = child_nodes(c, n);
        let [l, r] = kids[..] else { continue };
        if c.kind(l) != Literal || c.kind(r) != Literal {
            continue;
        }
        let (Some(lo), Some(hi)) = (int_literal(&c.trimmed(l)), int_literal(&c.trimmed(r))) else { continue };
        let Some(open) = c.tok(n, "[") else { continue };
        if lo > hi {
            s.add(
                "SV-RND-021",
                "ConstraintInsideInvertedRange",
                Severity::Error,
                format!(
                    "Range expression '[{lo} : {hi}]' inside set membership specifies an inverted range where lower bound exceeds upper bound ({lo} > {hi}). \
                     In SystemVerilog (IEEE 1800 § 18.5.7 and IEEE 1800 § 18.5.4), an inverted range defines an empty set, causing \
                     constraint solver contradictions and runtime randomization failure."
                ),
                c.loc(open),
            );
        }
    }
}

fn zero_repetition(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Repetition) {
        let toks: Vec<u32> = c.toks(n).collect();
        let Some(&op) = toks.get(1) else { continue };
        let op_text = c.text_of(op);
        if op_text != "->" && op_text != "=" {
            continue;
        }
        let Some((_, last)) = c.tree.token_bounds(n) else { continue };
        if last <= op + 1 {
            continue;
        }
        let sel: String = span(c, op + 1, last - 1).chars().filter(|ch| !ch.is_ascii_whitespace()).collect();
        let zero = sel == "0" || sel == "1'b0" || sel == "32'd0" || sel.starts_with("0:") || sel.starts_with("1'b0:");
        if zero {
            s.add(
                "SV-SVA-004",
                "NonConsecutiveRepetitionZeroBound",
                Severity::Error,
                format!(
                    "Non-consecutive repetition operator '[{op_text} {sel}]' specifies a zero-repetition bound. Under IEEE 1800 § 16.9.2 assertion semantics, \
                     non-consecutive repetitions ('[= ...]' and '[-> ...]') must specify a positive minimum repetition bound (>= 1). \
                     Repetition bounds of 0 are vacuous, illegal under standard LRM rules, and cause assertion solver aborts."
                ),
                c.loc(op),
            );
        }
    }
}

fn unbounded_delay(c: &Cst, s: &mut Sink) {
    for n in c.nodes(TimingControl) {
        let Some(first) = c.keyword(n) else { continue };
        let hash = c.text_of(first);
        if hash != "#" && hash != "##" {
            continue;
        }
        let Some((_, last)) = c.tree.token_bounds(n) else { continue };
        let shown = if let Some(open) = c.tok(n, "[") {
            // A sequence's `##[a:b]` (slang's DelayedSequenceElement).
            let Some(close) = c.tok(n, "]") else { continue };
            if close <= open + 1 || !c.parent(n).is_some_and(|p| c.kind(p) == SequenceExpr) {
                continue;
            }
            format!("[{}]", span(c, open + 1, close - 1))
        } else {
            // A statement's delay (slang's DelaySyntax).
            if !c.parent(n).is_some_and(|p| matches!(c.kind(p), TimedStmt | TimedExpr)) || last <= first {
                continue;
            }
            span(c, first + 1, last)
        };
        if !shown.contains('$') {
            continue;
        }
        s.add(
            "SVA-CYC-001",
            "SvaUnboundedDelay",
            Severity::Warning,
            format!(
                "Unbounded delay cycle range '##{shown}' in assertion sequence. \
                 In simulation, unbounded delays keep evaluation threads alive indefinitely in memory waiting for unreachable events. \
                 In formal property verification (FPV), unbounded delays cause infinite induction unrolling and proof timeouts. \
                 Specify a bounded maximum delay (e.g. '##[1:MAX_CYCLES]') or use the 's_eventually' liveness operator."
            ),
            c.loc(first),
        );
    }
}

fn sequence_priority(c: &Cst, s: &mut Sink) {
    for call in c.nodes(CallExpr) {
        let Some(callee) = c.tree.children(call).next() else { continue };
        if c.last_text(callee) != "start" {
            continue;
        }
        let Some(args) = c.tree.child(call, ArgList) else { continue };
        // The third argument: the node between the second and third commas.
        let mut commas = 0;
        let mut third = None;
        for k in c.kids(args) {
            match *k {
                Child::Token(t) if c.text_of(t) == "," => commas += 1,
                Child::Node(a) if commas == 2 => third = Some(a),
                _ => {}
            }
        }
        let Some(arg) = third else { continue };
        if c.kind(arg) != Arg || c.tok(arg, ".").is_some() {
            continue;
        }
        let Some(expr) = c.tree.children(arg).next() else { continue };
        let prio = c.trimmed(expr);
        if !prio.starts_with('-') || prio == "-1" {
            continue;
        }
        let Some(t) = c.first_tok(expr) else { continue };
        s.add(
            "UVM-SEQ-007",
            "SequencePriorityRange",
            Severity::Warning,
            format!(
                "Invalid sequence priority value '{prio}' passed to 'seq.start()'. In UVM, sequence priorities must be non-negative integers >= 0 (or -1 to inherit the sequencer default priority 100). Arbitrary negative values corrupt arbitration hierarchies."
            ),
            c.loc(t),
        );
    }
}
