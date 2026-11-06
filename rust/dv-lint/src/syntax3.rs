//! Syntax rules, third batch (builder slice N6.3): assignments in
//! expressions, event triggers, objects made in loops, inverted ranges,
//! zero repetitions, unbounded delays and sequence priorities.
//! See [`crate::syntax`].

use std::collections::HashMap;

use sv_syntax::{Child, NodeId, Origin};

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
        Rule { id: "SV-ARR-001", name: "OffByOneQueueIndex", check: Syntax(off_by_one_index) },
        Rule { id: "SV-RND-012", name: "ZeroWeightDistribution", check: Syntax(zero_weight_dist) },
        Rule { id: "SV-STY-003", name: "TodoCommentTracker", check: Syntax(todo_comments) },
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

fn off_by_one_index(c: &Cst, s: &mut Sink) {
    for n in c.nodes(SelectExpr) {
        let toks: Vec<u32> = c.toks(n).collect();
        let mut open = None;
        for &t in &toks {
            match c.text_of(t) {
                "[" => open = Some(t),
                "]" => {
                    let Some(o) = open.take() else { continue };
                    let text = span(c, o, t);
                    let text = text.trim_matches([' ', '\t', '\r', '\n']);
                    if text.contains(".size") && !text.contains('-') && !text.contains('+') {
                        s.add(
                            "SV-ARR-001",
                            "OffByOneQueueIndex",
                            Severity::Error,
                            format!(
                                "Out-of-bounds array access '{text}'. \
                                 In SystemVerilog (IEEE 1800 § 7.10.1), queue and dynamic array indices span from 0 to size()-1. \
                                 Accessing index size() is an off-by-one error that silently returns an uninitialized value ('x or 0) \
                                 without triggering a simulator runtime error. Did you mean index [size() - 1]?"
                            ),
                            c.loc(o),
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

fn zero_weight_dist(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ExprOrDist) {
        let Some(dist) = c.tok(n, "dist") else { continue };
        let Some(list) = c.tree.child(n, RangeList) else { continue };
        let items = child_nodes(c, list);
        let all_zero = items.iter().all(|&item| {
            if c.kind(item) != DistItem || (c.tok(item, ":=").is_none() && c.tok(item, ":/").is_none()) {
                return false;
            }
            let Some(&w) = child_nodes(c, item).last() else { return false };
            matches!(c.trimmed(w).as_str(), "0" | "'0" | "32'd0" | "32'b0")
        });
        if !items.is_empty() && all_zero {
            s.add(
                "SV-RND-012",
                "ZeroWeightDistribution",
                Severity::Error,
                "All weights in constraint distribution ('dist { ... }') evaluate to zero. \
                 Per IEEE 1800 § 18.5.4, at least one weight in a distribution constraint must evaluate to a non-zero value; a distribution sum of zero creates an unsatisfiable constraint that triggers solver runtime failure."
                    .to_string(),
                c.loc(dist),
            );
        }
    }
}

/// The comments in `text` (which starts at byte `base` of its file), as
/// byte ranges, with whether each is inside a `define. Strings are skipped.
fn comments(text: &str, base: u32) -> Vec<(u32, u32, bool)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut define_end = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'`' if text[i..].starts_with("`define") => {
                // To the first line end not escaped by a backslash.
                let mut e = i;
                while e < b.len() {
                    if b[e] == b'\n' {
                        let line = text[..e].trim_end_matches('\r');
                        if !line.ends_with('\\') {
                            break;
                        }
                    }
                    e += 1;
                }
                define_end = e;
                i += 1;
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' && b[i] != b'\n' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = text[i..].find('\n').map_or(b.len(), |e| i + e);
                out.push((base + i as u32, base + end as u32, i < define_end));
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let end = text[i + 2..].find("*/").map_or(b.len(), |e| i + 2 + e + 2);
                out.push((base + i as u32, base + end as u32, i < define_end));
                i = end;
            }
            _ => i += 1,
        }
    }
    out
}

/// Each token's leading comments, by token offset, from lexing `src`.
fn comments_before(src: &str) -> HashMap<u32, Vec<(u32, u32)>> {
    let mut map = HashMap::new();
    let mut pending = Vec::new();
    for tk in sv_lexer::lex(src) {
        match tk.kind {
            sv_lexer::Kind::LineComment | sv_lexer::Kind::BlockComment => pending.push((tk.start, tk.start + tk.len)),
            k if k.is_trivia() => {}
            // Comments before a macro use are dropped.
            sv_lexer::Kind::Directive => pending.clear(),
            _ => {
                if !pending.is_empty() {
                    map.insert(tk.start, std::mem::take(&mut pending));
                }
            }
        }
    }
    map
}

const TAGS: [&str; 5] = ["TODO", "FIXME", "ICEBOX", "HACK", "XXX"];

/// SV-STY-003 reads the comments slang keeps as token trivia: not those in
/// a branch not taken, in a `define line, or right before a macro use
/// (the expansion's first token takes the use's place); but those inside
/// a macro body, after its first token, at each expansion.
fn todo_comments(c: &Cst, s: &mut Sink) {
    let t = c.tree;
    let mut uses: HashMap<u32, Vec<u32>> = HashMap::new();
    for u in &t.macro_uses {
        uses.entry(u.origin.file).or_default().push(u.origin.offset);
    }
    let skipped = |f: u32, p: u32| t.skipped.iter().any(|&(sf, a, e)| sf == f && a <= p && p < e);
    let report = |s: &mut Sink, f: u32, a: u32, e: u32, loc: dv_model::Loc| {
        let text = &t.sources[f as usize][a as usize..e as usize];
        let Some(tag) = TAGS.iter().find(|tag| text.contains(*tag)) else { return };
        let mut snippet = text.trim_end_matches(['\r', '\n']).to_string();
        if snippet.len() > 80 {
            let mut cut = 77;
            while !snippet.is_char_boundary(cut) {
                cut -= 1;
            }
            snippet = format!("{}...", &snippet[..cut]);
        }
        s.add(
            "SV-STY-003",
            "TodoCommentTracker",
            Severity::Info,
            format!(
                "Unresolved development tag '{tag}' found in comment: {snippet}. Track and resolve all temporary FIXME, TODO, and ICEBOX items prior to verification closure."
            ),
            loc,
        );
    };
    let mut lexed: HashMap<u32, HashMap<u32, Vec<(u32, u32)>>> = HashMap::new();
    // The last token written in each file, and its end.
    let mut last: HashMap<u32, (usize, u32)> = HashMap::new();
    for (i, tok) in t.tokens.iter().enumerate() {
        if tok.from_macro {
            // Comments before a token of a macro body (or argument), but
            // not the expansion's first token.
            let Some(prev) = i.checked_sub(1).map(|p| &t.tokens[p]) else { continue };
            if !prev.from_macro || prev.origin != tok.origin {
                continue;
            }
            let w = tok.written;
            let map = lexed.entry(w.file).or_insert_with(|| comments_before(&t.sources[w.file as usize]));
            for &(a, e) in map.get(&w.offset).map_or(&[][..], Vec::as_slice) {
                report(s, w.file, a, e, c.loc(i as u32));
            }
            continue;
        }
        let f = tok.origin.file;
        last.insert(f, (i, tok.origin.offset + tok.text.len() as u32));
        if tok.trivia_start == tok.origin.offset {
            continue;
        }
        let triv = &t.sources[f as usize][tok.trivia_start as usize..tok.origin.offset as usize];
        let file_uses = uses.get(&f).map_or(&[][..], Vec::as_slice);
        for (a, e, in_define) in comments(triv, tok.trivia_start) {
            if in_define || skipped(f, a) || file_uses.iter().any(|&m| m > a && m < tok.origin.offset) {
                continue;
            }
            report(s, f, a, e, c.loc(i as u32));
        }
    }
    // Trivia after a file's last token goes with the next token, or with
    // the end of the main file.
    let mut ends: Vec<(u32, Option<(usize, u32)>)> = last.into_iter().map(|(f, e)| (f, Some(e))).collect();
    if !t.sources.is_empty() && !ends.iter().any(|&(f, _)| f == 0) {
        // A main file with no code at all.
        ends.push((0, None));
    }
    ends.sort_unstable();
    for (f, e) in ends {
        let src = &t.sources[f as usize];
        let end = e.map_or(0, |(_, end)| end);
        let at = match e.map(|(i, _)| i + 1).filter(|&n| n < t.tokens.len()) {
            Some(next) => c.loc(next as u32),
            None => c.loc_at(Origin { file: f, offset: src.len() as u32 }),
        };
        let file_uses = uses.get(&f).map_or(&[][..], Vec::as_slice);
        for (a, e, in_define) in comments(&src[end as usize..], end) {
            if in_define || skipped(f, a) || file_uses.iter().any(|&m| m > a) {
                continue;
            }
            report(s, f, a, e, at);
        }
    }
}
