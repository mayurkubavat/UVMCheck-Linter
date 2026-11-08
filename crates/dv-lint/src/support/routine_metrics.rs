//! Routine metrics (`metrics::computeRoutineMetrics`) for the SV-MET and
//! SV-SML-004 rules.

use sv_syntax::{NodeId, SyntaxKind};

use crate::support::statements::*;

use crate::support::class_index::*;

use crate::syntax::Cst;

use SyntaxKind::*;

/// A routine with a body that rules lint.
pub(crate) struct Routine {
    pub name: String,
    pub tok: u32,
    pub body: Option<NodeId>,
    pub is_task: bool,
    /// Owner name for `owner::name` messages ("" for $unit).
    pub owner: String,
    /// Class index of a class method.
    pub class: Option<usize>,
    pub ports: usize,
    /// How many times slang lists it (module instances).
    pub times: usize,
}

/// `swe::allUserRoutines`: methods of user classes, then free routines in
/// packages, $unit and module instances (each instance body on its own).
pub(crate) fn user_routines(c: &Cst, classes: &Classes) -> Vec<Routine> {
    let mut out = Vec::new();
    for i in classes.user() {
        let cls = &classes.list[i];
        for m in &cls.methods {
            out.push(Routine {
                name: m.name.clone(),
                tok: m.tok,
                body: m.body,
                is_task: m.is_task,
                owner: cls.name.clone(),
                class: Some(i),
                ports: m.ports.len(),
                times: 1,
            });
        }
    }
    let units = Units::new(c);
    for d in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        if has_scope(c, d) || c.toks(d).any(|t| c.text_of(t) == "import") {
            continue;
        }
        let Some(parent) = c.parent(d) else { continue };
        let (owner, times) = match c.kind(parent) {
            PackageDecl => {
                let p = c.tree.child(parent, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
                if p == "uvm_pkg" {
                    continue;
                }
                (p, 1)
            }
            SourceFile => (String::new(), 1),
            ModuleDecl | InterfaceDecl | ProgramDecl => {
                let p = c.tree.child(parent, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
                (p, units.direct_times(c, parent))
            }
            _ => continue,
        };
        if times == 0 {
            continue;
        }
        let Some(nm) = c.tree.child(d, Name) else { continue };
        let Some(tok) = c.tree.token_bounds(nm).map(|(_, b)| b) else {
            continue;
        };
        let f = file_of(c, tok);
        if f.contains("uvm_mock.svh") || f.contains("uvm_pkg.sv") {
            continue;
        }
        out.push(Routine {
            name: c.text_of(tok).to_string(),
            tok,
            body: c.tree.child(d, Body),
            is_task: c.kind(d) == TaskDecl,
            owner,
            class: None,
            ports: ports(c, d).len(),
            times,
        });
    }
    out
}

pub(crate) fn has_scope(c: &Cst, d: NodeId) -> bool {
    c.tree.child(d, Name).is_some_and(|n| c.toks(n).any(|t| c.text_of(t) == "::"))
}

#[derive(Default)]
pub(crate) struct Metrics {
    pub cognitive: i64,
    pub cyclomatic: i64,
    pub max_nesting: i64,
    pub deepest: Option<NodeId>,
    pub max_bool_ops: i64,
    pub most_complex: Option<NodeId>,
}

pub(crate) fn logical_ops(c: &Cst, e: NodeId) -> i64 {
    subtree(c, e).into_iter().filter(|&x| c.kind(x) == BinaryExpr && matches!(op(c, x), "&&" | "||")).count() as i64
}

pub(crate) fn logical_sequences(c: &Cst, e: NodeId, parent: Option<&str>) -> i64 {
    let e = strip_parens(c, e);
    match c.kind(e) {
        BinaryExpr => {
            let o = op(c, e);
            let mut it = c.tree.children(e);
            let (l, r) = (it.next(), it.next());
            let (Some(l), Some(r)) = (l, r) else { return 0 };
            if o == "&&" || o == "||" {
                let me = i64::from(parent != Some(o));
                me + logical_sequences(c, l, Some(o)) + logical_sequences(c, r, Some(o))
            } else {
                logical_sequences(c, l, None) + logical_sequences(c, r, None)
            }
        }
        PrefixExpr => c.tree.children(e).next().map(|x| logical_sequences(c, x, None)).unwrap_or(0),
        _ => 0,
    }
}

pub(crate) struct Walker<'a, 'c> {
    pub(crate) c: &'a Cst<'c>,
    pub(crate) m: Metrics,
}

impl Walker<'_, '_> {
    pub(crate) fn condition(&mut self, e: NodeId) {
        let ops = logical_ops(self.c, e);
        if ops > self.m.max_bool_ops {
            self.m.max_bool_ops = ops;
            self.m.most_complex = Some(strip_parens(self.c, e));
        }
        self.m.cognitive += logical_sequences(self.c, e, None);
        self.m.cyclomatic += ops;
    }

    pub(crate) fn structure(&mut self, s: NodeId, nesting: i64) {
        self.m.cognitive += 1 + nesting;
        if nesting + 1 > self.m.max_nesting {
            self.m.max_nesting = nesting + 1;
            self.m.deepest = Some(s);
        }
    }

    /// Ternaries inside a plain statement, outermost first.
    pub(crate) fn expressions(&mut self, s: NodeId, nesting: i64) {
        let c = self.c;
        for x in visited(c, s) {
            if c.kind(x) != CondExpr {
                continue;
            }
            self.m.cognitive += 1 + nesting;
            self.m.cyclomatic += 1;
            if let Some(cond) = c.tree.children(x).next() {
                self.condition(cond);
            }
        }
    }

    pub(crate) fn walk(&mut self, s: NodeId, nesting: i64) {
        let c = self.c;
        let Some(k) = stmt(c, s) else { return };
        match k {
            St::Seq => {
                for x in list(c, s) {
                    self.walk(x, nesting);
                }
            }
            St::Par(_) => {
                self.structure(s, nesting);
                for x in list(c, s) {
                    self.walk(x, nesting + 1);
                }
            }
            St::If => {
                let (cond, then, mut els) = if_parts(c, s);
                if is_report_guard(c, s) {
                    if let Some(t) = then {
                        self.walk(t, nesting);
                    }
                    return;
                }
                self.structure(s, nesting);
                self.m.cyclomatic += 1;
                if let Some(e) = cond {
                    self.condition(e);
                }
                if let Some(t) = then {
                    self.walk(t, nesting + 1);
                }
                while let Some(e) = els {
                    self.m.cognitive += 1;
                    if stmt(c, e) == Some(St::If) && !is_report_guard(c, e) {
                        let (cond, then, next) = if_parts(c, e);
                        self.m.cyclomatic += 1;
                        if let Some(x) = cond {
                            self.condition(x);
                        }
                        if let Some(t) = then {
                            self.walk(t, nesting + 1);
                        }
                        els = next;
                    } else {
                        self.walk(e, nesting + 1);
                        els = None;
                    }
                }
            }
            St::Case | St::RandCase => {
                let p = case_parts(c, s);
                self.structure(s, nesting);
                self.m.cyclomatic += p.items.len() as i64;
                for (_, x) in &p.items {
                    self.walk(*x, nesting + 1);
                }
                if k == St::Case {
                    if let Some(d) = p.default {
                        self.walk(d, nesting + 1);
                    }
                }
            }
            St::For => {
                let p = for_parts(c, s);
                self.structure(s, nesting);
                self.m.cyclomatic += 1;
                if let Some(e) = p.stop {
                    self.condition(e);
                }
                if let Some(b) = p.body {
                    self.walk(b, nesting + 1);
                }
            }
            St::While | St::DoWhile => {
                self.structure(s, nesting);
                self.m.cyclomatic += 1;
                if let Some(e) = exprs(c, s).into_iter().next() {
                    self.condition(e);
                }
                if let Some(b) = body_of(c, s) {
                    self.walk(b, nesting + 1);
                }
            }
            St::Foreach | St::Repeat | St::Forever => {
                self.structure(s, nesting);
                self.m.cyclomatic += 1;
                if let Some(b) = body_of(c, s) {
                    self.walk(b, nesting + 1);
                }
            }
            St::Timed => {
                if let Some(b) = body_of(c, s) {
                    self.walk(b, nesting);
                }
            }
            St::Wait if c.tok(s, "(").is_some() => {
                if let Some(e) = exprs(c, s).into_iter().next() {
                    self.condition(e);
                }
                if let Some(b) = body_of(c, s) {
                    self.walk(b, nesting);
                }
            }
            St::Assert => {
                if let Some(e) = exprs(c, s).into_iter().find(|&x| c.kind(x) != ElseClause) {
                    self.condition(e);
                }
                for x in nested_stmts(c, s) {
                    self.walk(x, nesting);
                }
            }
            _ => self.expressions(s, nesting),
        }
    }
}

pub(crate) fn routine_metrics(c: &Cst, body: Option<NodeId>) -> Metrics {
    let mut w = Walker { c, m: Metrics { cyclomatic: 1, ..Default::default() } };
    if let Some(b) = body {
        for x in list(c, b) {
            w.walk(x, 0);
        }
    }
    w.m
}

/// Nodes slang's visitor reaches in a body: local declarations'
/// initializers are not visited.
pub(crate) fn visited(c: &Cst, body: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![body];
    while let Some(x) = stack.pop() {
        if c.kind(x) == DataDecl {
            continue;
        }
        out.push(x);
        let kids: Vec<NodeId> = c.tree.children(x).collect();
        stack.extend(kids.into_iter().rev());
    }
    out
}

pub(crate) fn each_routine(c: &Cst, mut f: impl FnMut(&Classes, &Routine)) {
    let classes = Classes::new(c);
    for r in user_routines(c, &classes) {
        f(&classes, &r);
    }
}
