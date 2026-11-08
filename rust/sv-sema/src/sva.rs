//! Assertions, checkers, covergroups and procedures (IEEE 1800 chapters
//! 9, 16, 17 and 19): rules found from the syntax and the declarations of
//! sequences and properties.
//!
//! - `restrict` has no action block, and no concurrent assertion is in
//!   another one's action block (16.14);
//! - one `default disable iff` per scope (16.15);
//! - a sequence or property in a clocking block has no clock of its own
//!   (16.16);
//! - a sequence does not instantiate itself (16.8);
//! - a sequence or property instance's arguments match its formals:
//!   count, names, empty actuals (16.8);
//! - no repetition on a property instance, no `[->n]` / `[=n]` on a
//!   sequence instance (16.9.2);
//! - match items are assignments, `++` / `--` or calls (16.10);
//! - a property with `disable iff` is only instantiated as an
//!   assertion's whole property (16.12);
//! - assertion ports: no `ref`, a direction only with `local`, and no
//!   `local output` / `inout` in a property (16.8.2);
//! - checkers: no `always`, only assertions and event controls in
//!   `initial`, only event controls as timing, ports with a direction
//!   have a type and are not `inout`, free (`rand`) variables are not
//!   assigned continuously or with `=` (17.2 - 17.7);
//! - `always_ff` has no blocking timing control and `always_comb` /
//!   `always_latch` no fork (9.2.2);
//! - covergroups: no `output` / `inout` formals, formals are not
//!   assigned, `option.` / `type_option.` name real options (19.7).
//!
//! Sequence and property names are matched by name, and only where the
//! name declares nothing else anywhere in the design.

use std::collections::{HashMap, HashSet};

use sv_syntax::{Child, NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

use crate::names::Diag;

const OPTIONS: &[&str] = &[
    "weight",
    "goal",
    "name",
    "comment",
    "at_least",
    "auto_bin_max",
    "cross_num_print_missing",
    "detect_overlap",
    "per_instance",
    "get_inst_coverage",
    "cross_retain_auto_bins",
];
const TYPE_OPTIONS: &[&str] =
    &["weight", "goal", "comment", "strobe", "merge_instances", "distribute_first", "real_interval"];

#[derive(Clone)]
struct Formal {
    name: String,
    default: bool,
}

#[derive(Clone)]
struct AssertDecl {
    kind: SyntaxKind,
    tree: usize,
    node: NodeId,
    formals: Vec<Formal>,
    disable_iff: bool,
}

fn own_toks(t: &Tree, n: NodeId) -> Vec<&str> {
    t.node(n)
        .children
        .iter()
        .filter_map(|c| match c {
            Child::Token(i) => Some(&*t.tokens[*i as usize].text),
            Child::Node(_) => None,
        })
        .collect()
}

fn first_tok(t: &Tree, n: NodeId) -> &str {
    t.token_bounds(n).map_or("", |(f, _)| &*t.tokens[f as usize].text)
}

fn name_of(t: &Tree, n: NodeId) -> String {
    t.child(n, Name).map(|c| t.bare_text(c)).unwrap_or_default()
}

/// The identifier a node is, when it is one token.
fn single_ident(t: &Tree, n: NodeId) -> Option<String> {
    let (a, b) = t.token_bounds(n)?;
    (a == b && matches!(t.tokens[a as usize].kind, sv_syntax::TokenKind::Identifier))
        .then(|| t.tokens[a as usize].text.to_string())
}

fn parents(t: &Tree) -> Vec<u32> {
    let mut parent = vec![u32::MAX; t.nodes.len()];
    for n in t.preorder() {
        for c in t.children(n) {
            parent[c.0 as usize] = n.0;
        }
    }
    parent
}

fn descendants(t: &Tree, n: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = t.children(n).collect();
    while let Some(c) = stack.pop() {
        out.push(c);
        stack.extend(t.children(c));
    }
    out
}

/// The comma-separated entries of a parenthesized list, as token texts.
fn entries(t: &Tree, n: NodeId) -> Vec<Vec<String>> {
    let toks: Vec<String> = t.token_range(n).iter().map(|&i| t.tokens[i as usize].text.to_string()).collect();
    let Some(open) = toks.iter().position(|x| x == "(") else { return Vec::new() };
    let mut out = vec![Vec::new()];
    let mut depth = 0;
    for x in &toks[open + 1..] {
        match x.as_str() {
            "(" | "[" | "{" | "'{" => depth += 1,
            ")" | "]" | "}" if depth == 0 => break,
            ")" | "]" | "}" => depth -= 1,
            "," if depth == 0 => {
                out.push(Vec::new());
                continue;
            }
            _ => {}
        }
        out.last_mut().unwrap().push(x.clone());
    }
    if out.len() == 1 && out[0].is_empty() {
        out.clear();
    }
    out
}

struct Ctx<'a> {
    trees: &'a [Tree],
    decls: HashMap<String, AssertDecl>,
    out: Vec<Diag>,
}

/// Runs this module's checks over every tree of a compile and returns the
/// errors found (assertion, checker and covergroup errors), in tree and source order.
pub fn check(trees: &[Tree]) -> Vec<Diag> {
    // Every declared name, to keep only sequence / property names that
    // name nothing else.
    let mut count: HashMap<String, usize> = HashMap::new();
    let mut decls: HashMap<String, AssertDecl> = HashMap::new();
    for (ix, t) in trees.iter().enumerate() {
        for n in t.preorder() {
            if t.kind(n) == Name {
                *count.entry(t.bare_text(n)).or_default() += 1;
            }
            let k = t.kind(n);
            if !matches!(k, SequenceDecl | PropertyDecl) {
                continue;
            }
            let formals = t
                .child(n, PortList)
                .map(|pl| {
                    t.children(pl)
                        .filter(|&p| t.kind(p) == Port)
                        .filter_map(|p| t.child(p, Declarator))
                        .map(|d| Formal { name: name_of(t, d), default: own_toks(t, d).contains(&"=") })
                        .collect()
                })
                .unwrap_or_default();
            let disable_iff = t.child(n, PropertyExpr).is_some_and(|pe| t.child(pe, DisableIff).is_some());
            decls.insert(name_of(t, n), AssertDecl { kind: k, tree: ix, node: n, formals, disable_iff });
        }
    }
    decls.retain(|name, _| count.get(name) == Some(&1));
    let mut c = Ctx { trees, decls, out: Vec::new() };
    for ix in 0..trees.len() {
        c.tree(ix);
    }
    c.recursion();
    c.out
}

impl Ctx<'_> {
    fn err(&mut self, tree: usize, node: NodeId, message: String) {
        self.out.push(Diag { tree, node, message });
    }

    fn tree(&mut self, ix: usize) {
        let t = &self.trees[ix];
        let parent = parents(t);
        let up = |n: NodeId| (parent[n.0 as usize] != u32::MAX).then(|| NodeId(parent[n.0 as usize]));
        for n in t.preorder() {
            match t.kind(n) {
                ConcurrentAssert => self.assertion(ix, n),
                ModuleDecl | InterfaceDecl | ProgramDecl | CheckerDecl | GenerateBlock => {
                    let dd: Vec<NodeId> = t.children(n).filter(|&c| t.kind(c) == DefaultDisable).collect();
                    for &d in dd.iter().skip(1) {
                        self.err(ix, d, "scope already has a default disable declaration".into());
                    }
                    if t.kind(n) == CheckerDecl {
                        self.checker(ix, n);
                    }
                }
                // A sequence or property in a clocking block is the item itself.
                ClockingItem if matches!(first_tok(t, n), "sequence" | "property") => {
                    let clocked = t
                        .child(n, PropertyExpr)
                        .and_then(|pe| t.children(pe).next())
                        .is_some_and(|c| t.kind(c) == TimingControl && first_tok(t, c) == "@");
                    if clocked {
                        self.err(
                            ix,
                            n,
                            "sequences and properties declared within clocking blocks cannot specify an explicit clock event"
                                .into(),
                        );
                    }
                }
                SequenceDecl | PropertyDecl => self.assertion_ports(ix, n),
                CallExpr | NameExpr if self.in_property(t, n, &up) => self.instance(ix, n, &up),
                Repetition => self.repetition(ix, n, &up),
                SequenceExpr if first_tok(t, n) == "(" && own_toks(t, n).contains(&",") => {
                    // An event list in an argument: `s((e1, e2 iff c))`.
                    let mut a = up(n);
                    while let Some(p) = a.filter(|&p| matches!(t.kind(p), SequenceExpr | PropertyExpr)) {
                        a = up(p);
                    }
                    if a.is_some_and(|p| t.kind(p) == Arg) {
                        continue;
                    }
                    for item in t.children(n).skip(1) {
                        let ok = match t.kind(item) {
                            BinaryExpr => own_toks(t, item).iter().any(|x| {
                                x.ends_with('=')
                                    && !matches!(*x, "==" | "!=" | "<=" | ">=" | "===" | "!==" | "==?" | "!=?")
                            }),
                            PostfixExpr | PrefixExpr => own_toks(t, item).iter().any(|x| matches!(*x, "++" | "--")),
                            CallExpr => true,
                            _ => false,
                        };
                        if !ok {
                            self.err(ix, item, "sequence match item must be a local variable assignment, increment/decrement operator, or subroutine call".into());
                        }
                    }
                }
                ProceduralBlock => self.procedure(ix, n),
                CovergroupDecl => self.covergroup(ix, n),
                _ => {}
            }
        }
    }

    /// Inside a sequence, property or assertion's property expression.
    fn in_property(&self, t: &Tree, n: NodeId, up: &impl Fn(NodeId) -> Option<NodeId>) -> bool {
        let mut cur = up(n);
        while let Some(p) = cur {
            match t.kind(p) {
                PropertyExpr => return true,
                CallExpr | ArgList => {}
                _ => {}
            }
            cur = up(p);
        }
        false
    }

    fn assertion(&mut self, ix: usize, n: NodeId) {
        let t = &self.trees[ix];
        let kw = first_tok(t, n).to_string();
        let actions: Vec<NodeId> = t.children(n).filter(|&c| t.kind(c) != PropertyExpr).collect();
        if kw == "restrict" && actions.iter().any(|&a| t.kind(a) != EmptyStmt) {
            self.err(ix, n, "restrict statement cannot have an action block".into());
        }
        for a in actions {
            let mut nested: Vec<NodeId> = descendants(t, a);
            nested.push(a);
            for d in nested {
                if t.kind(d) == ConcurrentAssert {
                    let k = first_tok(t, d).to_string();
                    self.err(
                        ix,
                        d,
                        format!("cannot have '{k}' statement in the action block of another concurrent assertion"),
                    );
                }
            }
        }
    }

    /// The declaration a sequence / property instance names.
    fn target(&self, t: &Tree, n: NodeId) -> Option<(String, AssertDecl)> {
        let callee = if t.kind(n) == CallExpr { t.children(n).next()? } else { n };
        if t.kind(callee) != NameExpr {
            return None;
        }
        let name = single_ident(t, callee)?;
        let d = self.decls.get(&name)?.clone();
        Some((name, d))
    }

    fn instance(&mut self, ix: usize, n: NodeId, up: &impl Fn(NodeId) -> Option<NodeId>) {
        let t = &self.trees[ix];
        if t.kind(n) == NameExpr && up(n).is_some_and(|p| t.kind(p) == CallExpr && t.children(p).next() == Some(n)) {
            return;
        }
        let Some((name, d)) = self.target(t, n) else { return };
        // Arguments.
        let args = t.child(n, ArgList).map(|al| entries(t, al)).unwrap_or_default();
        let named = args.first().is_some_and(|a| a.first().is_some_and(|x| x == "."));
        if named {
            let mut seen = HashSet::new();
            for a in &args {
                let Some(an) = a.get(1) else { continue };
                if !d.formals.iter().any(|f| &f.name == an) {
                    self.err(ix, n, format!("argument '{an}' does not exist in '{name}'"));
                } else if !seen.insert(an.clone()) {
                    self.err(ix, n, format!("duplicate assignment for argument '{an}'"));
                } else if a.len() <= 4 && !d.formals.iter().any(|f| &f.name == an && f.default) {
                    // `.a()`
                    self.err(
                        ix,
                        n,
                        format!("argument '{an}' cannot be empty because it does not have a default value"),
                    );
                }
            }
            for f in &d.formals {
                if !f.default && !seen.contains(&f.name) {
                    self.err(ix, n, format!("argument '{}' is missing a value", f.name));
                }
            }
        } else if args.len() > d.formals.len() {
            self.err(
                ix,
                n,
                format!(
                    "too many arguments for '{name}'; expected {} but {} were provided",
                    d.formals.len(),
                    args.len()
                ),
            );
        } else {
            for (i, f) in d.formals.iter().enumerate() {
                match args.get(i) {
                    Some(a) if a.is_empty() && !f.default => {
                        self.err(
                            ix,
                            n,
                            format!("argument '{}' cannot be empty because it does not have a default value", f.name),
                        );
                    }
                    None if !f.default => {
                        self.err(
                            ix,
                            n,
                            format!(
                                "too few arguments for '{name}'; expected {} but {} were provided",
                                d.formals.len(),
                                args.len()
                            ),
                        );
                        break;
                    }
                    _ => {}
                }
            }
        }
        if d.kind == PropertyDecl && d.disable_iff && !self.whole_property(t, n, up) {
            self.err(ix, n, format!("cannot instantiate '{name}' here because it has a disable iff condition"));
        }
    }

    /// `n` is the whole property of an assertion: nothing but nesting
    /// between it and the assertion.
    fn whole_property(&self, t: &Tree, n: NodeId, up: &impl Fn(NodeId) -> Option<NodeId>) -> bool {
        let mut cur = n;
        while let Some(p) = up(cur) {
            match t.kind(p) {
                SequenceExpr | PropertyExpr
                    if t.children(p).count() == 1 && own_toks(t, p).iter().all(|x| matches!(*x, "(" | ")")) =>
                {
                    cur = p
                }
                ConcurrentAssert => return true,
                _ => return false,
            }
        }
        false
    }

    fn repetition(&mut self, ix: usize, n: NodeId, up: &impl Fn(NodeId) -> Option<NodeId>) {
        let t = &self.trees[ix];
        let Some(p) = up(n) else { return };
        let kids: Vec<NodeId> = t.children(p).collect();
        let Some(i) = kids.iter().position(|&k| k == n) else { return };
        let Some(&operand) = i.checked_sub(1).and_then(|j| kids.get(j)) else { return };
        let inner: Vec<NodeId> = t.children(operand).collect();
        let [inst] = inner[..] else { return };
        if !own_toks(t, operand).is_empty() {
            return;
        }
        let Some((_, d)) = self.target(t, inst) else { return };
        let op = own_toks(t, n).get(1).copied().unwrap_or("");
        if d.kind == PropertyDecl {
            self.err(ix, n, "property instances cannot specify a repetition".into());
        } else if matches!(op, "->" | "=") {
            self.err(ix, n, "sequences cannot specify a non-consecutive or go-to repetition".into());
        }
    }

    /// Sequences that instantiate themselves, directly or not.
    fn recursion(&mut self) {
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        for (name, d) in &self.decls {
            if d.kind != SequenceDecl {
                continue;
            }
            let t = &self.trees[d.tree];
            let Some(pe) = t.child(d.node, PropertyExpr) else { continue };
            let mut to = Vec::new();
            for c in descendants(t, pe) {
                if t.kind(c) == NameExpr {
                    if let Some(x) = single_ident(t, c) {
                        if self.decls.get(&x).is_some_and(|e| e.kind == SequenceDecl) {
                            to.push(x);
                        }
                    }
                }
            }
            edges.insert(name.clone(), to);
        }
        let mut names: Vec<&String> = edges.keys().collect();
        names.sort();
        let mut bad = Vec::new();
        for start in names {
            let mut seen = HashSet::new();
            let mut stack = edges[start].clone();
            while let Some(x) = stack.pop() {
                if &x == start {
                    bad.push(start.clone());
                    break;
                }
                if seen.insert(x.clone()) {
                    stack.extend(edges.get(&x).cloned().unwrap_or_default());
                }
            }
        }
        for name in bad {
            let d = &self.decls[&name];
            let (tree, node) = (d.tree, d.node);
            self.err(tree, node, format!("sequence '{name}' is recursive"));
        }
    }

    fn assertion_ports(&mut self, ix: usize, n: NodeId) {
        let t = &self.trees[ix];
        let Some(pl) = t.child(n, PortList) else { return };
        let property = t.kind(n) == PropertyDecl;
        for p in t.children(pl).filter(|&c| t.kind(c) == Port) {
            let toks: Vec<String> = t.token_range(p).iter().map(|&i| t.tokens[i as usize].text.to_string()).collect();
            let dir = toks.iter().find(|x| matches!(x.as_str(), "input" | "output" | "inout" | "ref")).cloned();
            let local = toks.first().is_some_and(|x| x == "local");
            match dir.as_deref() {
                Some("ref") => self.err(ix, p, "assertion item port direction cannot be 'ref'".into()),
                Some(d) if !local => self.err(
                    ix,
                    p,
                    format!("assertion port cannot specify direction '{d}' without also using the 'local' keyword"),
                ),
                Some(d @ ("output" | "inout")) if property => self.err(
                    ix,
                    p,
                    format!("local variable formal arguments in properties cannot have direction '{d}'"),
                ),
                _ => {}
            }
        }
    }

    fn checker(&mut self, ix: usize, n: NodeId) {
        let t = &self.trees[ix];
        if let Some(pl) = t.child(n, Header).and_then(|h| t.child(h, PortList)).or_else(|| t.child(n, PortList)) {
            for p in t.children(pl).filter(|&c| t.kind(c) == Port) {
                let own = own_toks(t, p);
                if own.contains(&"inout") || own.contains(&"ref") {
                    let d = if own.contains(&"inout") { "inout" } else { "ref" };
                    self.err(ix, p, format!("checker port direction cannot be '{d}'"));
                } else if own.iter().any(|x| matches!(*x, "input" | "output")) && t.child(p, DataType).is_none() {
                    self.err(
                        ix,
                        p,
                        "checker ports with an explicit direction declared must also declare a type".into(),
                    );
                }
            }
        }
        // Free checker variables.
        let mut free = HashSet::new();
        for d in descendants(t, n).into_iter().filter(|&c| t.kind(c) == DataDecl) {
            if own_toks(t, d).contains(&"rand") {
                for decl in t.children(d).filter(|&c| t.kind(c) == Declarator) {
                    free.insert(name_of(t, decl));
                }
            }
        }
        for c in descendants(t, n) {
            match t.kind(c) {
                ProceduralBlock => {
                    let kw = first_tok(t, c);
                    if kw == "always" {
                        self.err(ix, c, "always procedures are not allowed in checkers (use always_comb, always_latch, or always_ff)".into());
                    }
                    let body: Vec<NodeId> = t.children(c).collect();
                    if kw == "initial" {
                        for s in body.iter().copied() {
                            self.checker_initial(ix, s);
                        }
                    }
                    for s in body.iter().flat_map(|&s| descendants(t, s).into_iter().chain([s])) {
                        if t.kind(s) == TimedStmt
                            && t.children(s).next().is_some_and(|tc| matches!(first_tok(t, tc), "#" | "##"))
                        {
                            self.err(
                                ix,
                                s,
                                "procedural timing control statements in checkers can only use event controls".into(),
                            );
                        }
                        if t.kind(s) == BinaryExpr && own_toks(t, s) == ["="] {
                            if let Some(l) = t.children(s).next().and_then(|l| single_ident(t, l)) {
                                if free.contains(&l) {
                                    self.err(ix, s, "continuous assignments and blocking procedural assignments to free checker variables are not allowed".into());
                                }
                            }
                        }
                    }
                }
                ContinuousAssign => {
                    for e in t.children(c).filter(|&e| t.kind(e) == BinaryExpr) {
                        if let Some(l) = t.children(e).next().and_then(|l| single_ident(t, l)) {
                            if free.contains(&l) {
                                self.err(ix, e, "continuous assignments and blocking procedural assignments to free checker variables are not allowed".into());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// An `initial` procedure in a checker holds only assertions and
    /// event controls (17.7).
    fn checker_initial(&mut self, ix: usize, s: NodeId) {
        let t = &self.trees[ix];
        match t.kind(s) {
            SeqBlock => {
                for c in t.children(s).collect::<Vec<_>>() {
                    self.checker_initial(ix, c);
                }
            }
            TimedStmt => {
                let kids: Vec<NodeId> = t.children(s).collect();
                for c in kids.into_iter().skip(1) {
                    self.checker_initial(ix, c);
                }
            }
            ConcurrentAssert | AssertStmt | EmptyStmt | LetDecl | EndLabel | StmtLabel | DataDecl => {}
            _ => {
                self.err(ix, s, "statement is not allowed in 'initial' procedure inside a checker".into());
            }
        }
    }

    fn procedure(&mut self, ix: usize, n: NodeId) {
        let t = &self.trees[ix];
        let kw = first_tok(t, n).to_string();
        let Some(body) = t.children(n).next() else { return };
        match kw.as_str() {
            "always_ff" => {
                // After the procedure's own event control.
                let inner: Vec<NodeId> = if t.kind(body) == TimedStmt {
                    t.children(body).skip(1).flat_map(|c| descendants(t, c).into_iter().chain([c])).collect()
                } else {
                    descendants(t, body)
                };
                for c in inner {
                    if matches!(t.kind(c), TimedStmt | WaitStmt) {
                        self.err(ix, c, "always_ff procedures cannot have blocking timing controls".into());
                        break;
                    }
                    if matches!(t.kind(c), FunctionDecl | TaskDecl | ClassDecl) {
                        break;
                    }
                }
            }
            "always_comb" | "always_latch" => {
                for c in descendants(t, body).into_iter().chain([body]) {
                    if t.kind(c) == ParBlock {
                        self.err(ix, c, format!("fork-join is not allowed in {kw} procedure"));
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    fn covergroup(&mut self, ix: usize, n: NodeId) {
        let t = &self.trees[ix];
        let mut formals = HashSet::new();
        if let Some(pl) = t.child(n, PortList) {
            for p in t.children(pl).filter(|&c| t.kind(c) == Port) {
                let own = own_toks(t, p);
                if own.iter().any(|x| matches!(*x, "output" | "inout")) {
                    self.err(ix, p, "covergroups cannot have 'output' or 'inout' arguments".into());
                }
                if let Some(d) = t.child(p, Declarator) {
                    formals.insert(name_of(t, d));
                }
            }
        }
        for c in descendants(t, n) {
            match t.kind(c) {
                CoverageOption => {
                    for e in t.children(c).filter(|&e| t.kind(e) == BinaryExpr) {
                        let Some(m) = t.children(e).next().filter(|&m| t.kind(m) == MemberExpr) else { continue };
                        let Some(base) = t.children(m).next().and_then(|b| single_ident(t, b)) else { continue };
                        let Some(member) = own_toks(t, m).last().map(|s| s.to_string()) else { continue };
                        let list = match base.as_str() {
                            "option" => OPTIONS,
                            "type_option" => TYPE_OPTIONS,
                            _ => continue,
                        };
                        if !list.contains(&member.as_str()) {
                            self.err(ix, m, format!("no member named '{member}' in '{base}'"));
                        }
                    }
                }
                BinaryExpr if own_toks(t, c).iter().any(|x| crate::checks::is_assign_op(x)) => {
                    if let Some(l) = t.children(c).next().and_then(|l| single_ident(t, l)) {
                        if formals.contains(&l) {
                            self.err(ix, c, format!("cannot assign to read-only variable '{l}'"));
                        }
                    }
                }
                PostfixExpr | PrefixExpr if own_toks(t, c).iter().any(|x| matches!(*x, "++" | "--")) => {
                    if let Some(l) = t.children(c).next().and_then(|l| single_ident(t, l)) {
                        if formals.contains(&l) {
                            self.err(ix, c, format!("cannot assign to read-only variable '{l}'"));
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
