//! Rules that read only the syntax tree, ported from the C++ rules that walk
//! slang's syntax with a `SyntaxVisitor`. They run over `sv-syntax` trees.
//!
//! sv-syntax has fewer node kinds than slang (a case statement's items are
//! flat children, an assignment is a `BinaryExpr` with an `=` token), so each
//! port names the slang shape it stands for. Messages are verbatim.

use std::cell::RefCell;

use dv_model::Loc;
use sv_sema::Locator;
use sv_syntax::{Child, NodeId, SyntaxKind, Tree};

use crate::{Rule, Severity, Sink};
use SyntaxKind::*;

/// One tree, with a pre-order of its nodes and their parents, and the
/// locator that numbers files as the design does.
pub struct Cst<'t> {
    pub tree: &'t Tree,
    ix: usize,
    pre: Vec<NodeId>,
    parent: Vec<u32>,
    locator: RefCell<Locator>,
    /// Files earlier trees of the compile read: slang does not read an
    /// include-guarded header again.
    pub prior_files: std::rc::Rc<std::collections::HashSet<std::path::PathBuf>>,
    /// Every tree of the compile (this one at `ix`), for checks that see
    /// names other files declare.
    pub all: &'t [Tree],
}

const NO_PARENT: u32 = u32::MAX;

impl<'t> Cst<'t> {
    pub fn new(tree: &'t Tree, ix: usize, locator: Locator) -> Self {
        let pre = tree.preorder();
        let mut parent = vec![NO_PARENT; tree.nodes.len()];
        for &n in &pre {
            for c in tree.children(n) {
                parent[c.0 as usize] = n.0;
            }
        }
        Cst { tree, ix, pre, parent, locator: RefCell::new(locator), prior_files: Default::default(), all: std::slice::from_ref(tree) }
    }

    /// This tree's index in [`Self::all`].
    pub fn index(&self) -> usize {
        if self.all.len() == 1 { 0 } else { self.ix }
    }

    /// Gives back the locator, with any files this tree added.
    pub fn into_locator(self) -> Locator {
        self.locator.into_inner()
    }

    /// Nodes of a kind, in source order.
    pub fn nodes(&self, kind: SyntaxKind) -> Vec<NodeId> {
        self.pre.iter().copied().filter(|&n| self.tree.kind(n) == kind).collect()
    }

    pub fn kind(&self, n: NodeId) -> SyntaxKind {
        self.tree.kind(n)
    }

    pub fn parent(&self, n: NodeId) -> Option<NodeId> {
        let p = self.parent[n.0 as usize];
        (p != NO_PARENT).then_some(NodeId(p))
    }

    pub fn kids(&self, n: NodeId) -> &'t [Child] {
        &self.tree.node(n).children
    }

    /// Child nodes, skipping labels and attributes (slang keeps those
    /// outside a block's items and a statement's kind).
    pub fn items(&self, n: NodeId) -> Vec<NodeId> {
        self.tree
            .children(n)
            .filter(|&c| !matches!(self.kind(c), EndLabel | StmtLabel | Attribute))
            .collect()
    }

    /// Tokens that are direct children of `n`.
    pub fn toks(&self, n: NodeId) -> impl Iterator<Item = u32> + 't {
        self.tree.node(n).children.iter().filter_map(|c| match *c {
            Child::Token(t) => Some(t),
            Child::Node(_) => None,
        })
    }

    pub fn text_of(&self, tok: u32) -> &'t str {
        &self.tree.tokens[tok as usize].text
    }

    /// First direct token with this text.
    pub fn tok(&self, n: NodeId, text: &str) -> Option<u32> {
        self.toks(n).find(|&t| self.text_of(t) == text)
    }

    /// The first direct token (a statement's or block's keyword).
    pub fn keyword(&self, n: NodeId) -> Option<u32> {
        self.toks(n).next()
    }

    pub fn first_tok(&self, n: NodeId) -> Option<u32> {
        self.tree.token_bounds(n).map(|(a, _)| a)
    }

    pub fn last_text(&self, n: NodeId) -> &'t str {
        self.tree.token_bounds(n).map(|(_, b)| self.text_of(b)).unwrap_or("")
    }

    /// slang's `toString()`: the text with each token's leading trivia
    /// (none for macro-made tokens here).
    pub fn text(&self, n: NodeId) -> String {
        let mut s = String::new();
        for (i, t) in self.tree.token_range(n).into_iter().enumerate() {
            let tok = &self.tree.tokens[t as usize];
            if !tok.from_macro {
                s.push_str(self.tree.trivia(t));
            } else if i > 0 && tok.space_before {
                // slang keeps a macro body's trivia on the expanded tokens.
                s.push(' ');
            }
            s.push_str(&tok.text);
        }
        s
    }

    /// [`Self::text`] with surrounding whitespace trimmed.
    pub fn trimmed(&self, n: NodeId) -> String {
        self.text(n).trim_matches([' ', '\t', '\n', '\r']).to_string()
    }

    /// Location of a preprocessor position (not in a macro).
    pub fn loc_at(&self, o: sv_syntax::Origin) -> Loc {
        let (file, line, column) = self.locator.borrow_mut().position(self.ix, self.tree, o);
        Loc { file, line, column, expansion: None }
    }

    pub fn loc(&self, tok: u32) -> Loc {
        self.locator.borrow_mut().loc(self.ix, self.tree, tok)
    }

    /// The node after token `tok` among `n`'s children.
    pub fn node_after(&self, n: NodeId, tok: u32) -> Option<NodeId> {
        let kids = self.kids(n);
        let i = kids.iter().position(|c| *c == Child::Token(tok))?;
        kids[i + 1..].iter().find_map(|c| match *c {
            Child::Node(x) => Some(x),
            Child::Token(_) => None,
        })
    }
}

pub type SyntaxFn = fn(&Cst, &mut Sink);

pub(crate) fn rules() -> Vec<Rule> {
    use crate::Check::Syntax;
    vec![
        Rule { id: "SV-COR-009", name: "CasexProhibited", check: Syntax(casex_prohibited) },
        Rule { id: "SV-COR-006", name: "CaseWithoutDefault", check: Syntax(case_without_default) },
        Rule { id: "SV-COR-008", name: "SelfComparison", check: Syntax(self_comparison) },
        Rule { id: "SV-DEP-001", name: "ProgramBlockDeprecation", check: Syntax(program_block) },
        Rule { id: "SV-DEP-002", name: "DefparamDeprecation", check: Syntax(defparam) },
        Rule { id: "SV-DEP-003", name: "LegacyAlwaysBlock", check: Syntax(legacy_always) },
        Rule { id: "SV-DEP-005", name: "ProceduralContinuousAssign", check: Syntax(procedural_assign) },
        Rule { id: "SV-DED-009", name: "EmptyAlwaysOrInitial", check: Syntax(empty_procedural_block) },
        Rule { id: "SV-DED-010", name: "EmptyForkJoin", check: Syntax(empty_fork_join) },
        Rule { id: "SV-DLY-002", name: "IntraAssignmentDelay", check: Syntax(intra_assignment_delay) },
        Rule { id: "SV-RCE-001", name: "ZeroDelayEventTrigger", check: Syntax(zero_delay_event_trigger) },
        Rule { id: "SV-CON-005", name: "SemaphoreZeroKey", check: Syntax(semaphore_zero_key) },
        Rule { id: "SV-CON-007", name: "UnboundedMailboxInstantiation", check: Syntax(unbounded_mailbox) },
    ]
}

/// slang's `CaseStatementSyntax`: case, casex, casez (randcase is another kind).
fn case_statements(c: &Cst) -> Vec<(NodeId, u32)> {
    c.nodes(CaseStmt).into_iter().filter_map(|n| {
        let kw = c.toks(n).find(|&t| matches!(c.text_of(t), "case" | "casex" | "casez"))?;
        Some((n, kw))
    }).collect()
}

fn casex_prohibited(c: &Cst, s: &mut Sink) {
    for (_, kw) in case_statements(c) {
        if c.text_of(kw) == "casex" {
            s.add(
                "SV-COR-009",
                "CasexProhibited",
                Severity::Error,
                "Use of 'casex' statement is prohibited in verification environments. \
                 In SystemVerilog (IEEE 1800 § 12.5.3), 'casex' treats both literal 'x'/'z' and unknown simulation 'x' bits as don't-care wildcards. \
                 If an uninitialized signal, timing race, or bug produces 'x', 'casex' silently matches the first matching branch, \
                 masking critical testbench and DUT failures. Replace 'casex' with 'casez' or 'case inside'."
                    .into(),
                c.loc(kw),
            );
        }
    }
}

fn case_without_default(c: &Cst, s: &mut Sink) {
    for (n, kw) in case_statements(c) {
        if c.tok(n, "default").is_none() {
            s.add(
                "SV-COR-006",
                "CaseWithoutDefault",
                Severity::Warning,
                format!(
                    "Statement '{}' lacks an explicit 'default:' fallback branch. In verification state decoders, bus monitors, and packet parsers, omitting a default branch causes unhandled transaction edge cases and inadvertent simulation latch behavior.",
                    c.text_of(kw)
                ),
                c.loc(kw),
            );
        }
    }
}

/// slang's binary operator kinds the rule checks; `<=` directly under an
/// expression statement is a nonblocking assignment there instead.
fn self_comparison(c: &Cst, s: &mut Sink) {
    for n in c.nodes(BinaryExpr) {
        let Some(op) = c.toks(n).next() else { continue };
        let ops = ["==", "!=", "===", "!==", "==?", "!=?", "<", "<=", ">", ">="];
        if !ops.contains(&c.text_of(op)) {
            continue;
        }
        if c.text_of(op) == "<=" && c.parent(n).is_some_and(|p| c.kind(p) == ExprStmt) {
            continue;
        }
        let mut sides = c.tree.children(n);
        let (Some(l), Some(r)) = (sides.next(), sides.next()) else { continue };
        let (ls, rs) = (c.trimmed(l), c.trimmed(r));
        if !ls.is_empty() && ls == rs {
            s.add(
                "SV-COR-008",
                "SelfComparison",
                Severity::Error,
                format!(
                    "Self-comparison detected: expression '{ls}' is compared to itself using operator '{}'. Self-comparisons tautologically evaluate to constant true or false, almost invariably indicating a copy-paste error where another variable or expected value was intended.",
                    c.text_of(op)
                ),
                c.loc(op),
            );
        }
    }
}

fn program_block(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ProgramDecl) {
        let Some(kw) = c.tok(n, "program") else { continue };
        // A program with no name is a syntax error that uvmcheck reports
        // nothing else on.
        let name = c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        s.add(
            "SV-DEP-001",
            "ProgramBlockDeprecation",
            Severity::Error,
            format!(
                "The 'program' block '{name}' is obsolete and deprecated in modern SystemVerilog/UVM (IEEE 1800 Annex C / IEEE 1800-2017 § 24). \
                 Program blocks cause premature simulation termination ($finish when initial blocks end) and reactive scheduling hazards. \
                 Replace with 'module' and clocking blocks or standard packages."
            ),
            c.loc(kw),
        );
    }
}

fn defparam(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ParamDecl) {
        let Some(kw) = c.tok(n, "defparam") else { continue };
        s.add(
            "SV-DEP-002",
            "DefparamDeprecation",
            Severity::Error,
            "Hierarchical parameter override 'defparam' is formally deprecated in IEEE 1800 § 23.10.1. \
             defparam violates module encapsulation, introduces cross-hierarchy elaboration dependencies, \
             and prevents simulator caching. Use named parameter overrides at the instantiation site (#(.PARAM(val)))."
                .into(),
            c.loc(kw),
        );
    }
}

const PROCEDURAL_KEYWORDS: [&str; 6] = ["initial", "final", "always", "always_comb", "always_ff", "always_latch"];

/// slang's `ProceduralBlockSyntax` and its keyword.
fn procedural_blocks(c: &Cst) -> Vec<(NodeId, u32)> {
    c.nodes(ProceduralBlock).into_iter().filter_map(|n| {
        let kw = c.toks(n).find(|&t| PROCEDURAL_KEYWORDS.contains(&c.text_of(t)))?;
        Some((n, kw))
    }).collect()
}

fn legacy_always(c: &Cst, s: &mut Sink) {
    for (_, kw) in procedural_blocks(c) {
        if c.text_of(kw) == "always" {
            s.add(
                "SV-DEP-003",
                "LegacyAlwaysBlock",
                Severity::Warning,
                "Legacy untyped 'always' block is superseded in IEEE 1800 § 9.2.2. \
                 Untyped always blocks permit unintended latch inference and incomplete sensitivity lists. \
                 Use intent-specific procedural blocks: 'always_comb' for combinational logic, \
                 'always_ff @(posedge clk ...)' for sequential logic, or 'always_latch' for transparent latches."
                    .into(),
                c.loc(kw),
            );
        }
    }
}

fn procedural_assign(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ProcAssign) {
        // `deassign` and `release` are slang's ProceduralDeassignStatement.
        let Some(kw) = c.toks(n).find(|&t| matches!(c.text_of(t), "assign" | "force")) else {
            continue;
        };
        s.add(
            "SV-DEP-005",
            "ProceduralContinuousAssign",
            Severity::Error,
            format!(
                "Procedural continuous assignment ('{}') is non-synthesizable and strongly discouraged in IEEE 1800 § 10.6. \
                 Procedural assign/deassign statements create simulator-synthesis mismatches, break formal verification, \
                 and induce unpredictable dynamic driver overriding races. Use standard blocking/non-blocking procedural assignments.",
                c.text_of(kw)
            ),
            c.loc(kw),
        );
    }
}

fn effectively_empty(c: &Cst, stmt: Option<NodeId>) -> bool {
    let Some(n) = stmt else { return true };
    match c.kind(n) {
        EmptyStmt => true,
        SeqBlock | ParBlock => c.items(n).into_iter().next().is_none(),
        TimedStmt => effectively_empty(c, c.items(n).into_iter().find(|&x| c.kind(x) != TimingControl)),
        _ => false,
    }
}

fn empty_procedural_block(c: &Cst, s: &mut Sink) {
    for (n, kw) in procedural_blocks(c) {
        if effectively_empty(c, c.items(n).into_iter().next()) {
            s.add(
                "SV-DED-009",
                "EmptyAlwaysOrInitial",
                Severity::Warning,
                format!(
                    "Procedural '{}' block contains an empty body. Empty procedural blocks consume simulator scheduling cycles with zero functional impact and frequently indicate unfinished or abandoned logic.",
                    c.text_of(kw)
                ),
                c.loc(kw),
            );
        }
    }
}

fn empty_fork_join(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ParBlock) {
        if c.items(n).into_iter().next().is_some() {
            continue;
        }
        let (Some(fork), Some(join)) = (
            c.tok(n, "fork"),
            c.toks(n).find(|&t| matches!(c.text_of(t), "join" | "join_any" | "join_none")),
        ) else {
            continue;
        };
        s.add(
            "SV-DED-010",
            "EmptyForkJoin",
            Severity::Warning,
            format!(
                "Parallel 'fork ... {}' block contains zero statements. Spawning empty concurrent fork blocks adds unnecessary delta cycles to the simulator event queue with zero functional behavior. Remove the empty block.",
                c.text_of(join)
            ),
            c.loc(fork),
        );
    }
}

/// slang's `AssignmentExpression` (plain `=`): its operator, left and right.
fn assignments(c: &Cst) -> Vec<(u32, NodeId, NodeId)> {
    c.nodes(BinaryExpr).into_iter().filter_map(|n| {
        let op = c.toks(n).next()?;
        if c.text_of(op) != "=" {
            return None;
        }
        let mut sides = c.tree.children(n);
        Some((op, sides.next()?, sides.next()?))
    }).collect()
}

fn intra_assignment_delay(c: &Cst, s: &mut Sink) {
    // slang gives `<=` its own kind, so only the blocking form is reached.
    for (op, _, rhs) in assignments(c) {
        if c.kind(rhs) == TimedExpr {
            s.add(
                "SV-DLY-002",
                "IntraAssignmentDelay",
                Severity::Error,
                "Intra-assignment delay used on blocking procedural assignment ('='). \
                 Blocking intra-assignment delays (e.g. 'a = #5 b;') sample the RHS, suspend thread execution for the delay, and update the LHS later. \
                 This creates severe simulation race conditions if RHS signals mutate in the interim. \
                 Use separate procedural delay statements ('#5; a = b;') or non-blocking clocking drives."
                    .into(),
                c.loc(op),
            );
        }
    }
}

fn zero_delay_event_trigger(c: &Cst, s: &mut Sink) {
    for n in c.nodes(EventTrigger) {
        let Some(trigger) = c.tok(n, "->") else { continue };
        let Some(name) = c.items(n).into_iter().next() else { continue };
        let ev = c.last_text(name);
        s.add(
            "SV-RCE-001",
            "ZeroDelayEventTrigger",
            Severity::Warning,
            format!(
                "Blocking event trigger '-> {ev}' executed in Active region. \
                 If waiting threads have not yet evaluated '@({ev})' within the current time step, the trigger is permanently missed, creating a classic zero-delay race condition. \
                 Use non-blocking event trigger '->> {ev}' to schedule the event in the NBA region, or use persistent event checking 'wait({ev}.triggered)' per IEEE 1800 § 15.5."
            ),
            c.loc(trigger),
        );
    }
}

/// The first argument of an argument list when it is ordered (not named or
/// left empty): its expression.
fn first_ordered_arg(c: &Cst, list: NodeId) -> Option<NodeId> {
    let kids = c.kids(list);
    let open = kids.iter().position(|k| matches!(*k, Child::Token(t) if c.text_of(t) == "("))?;
    let Child::Node(arg) = *kids.get(open + 1)? else { return None };
    if c.kind(arg) != Arg || c.keyword(arg).is_some_and(|t| c.text_of(t) == ".") {
        return None;
    }
    c.items(arg).into_iter().next()
}

fn semaphore_zero_key(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CallExpr) {
        let Some(left) = c.tree.children(n).next() else { continue };
        let method = c.last_text(left);
        if !matches!(method, "get" | "put" | "try_get") || !c.text(left).contains("sem") {
            continue;
        }
        let Some(args) = c.tree.child(n, ArgList) else { continue };
        let Some(expr) = first_ordered_arg(c, args) else { continue };
        let arg = c.trimmed(expr);
        if matches!(arg.as_str(), "0" | "32'd0" | "1'b0") || arg.starts_with('-') {
            let Some(first) = c.first_tok(expr) else { continue };
            s.add(
                "SV-CON-005",
                "SemaphoreZeroKey",
                Severity::Error,
                format!(
                    "Semaphore operation '.{method}({arg})' specifies a non-positive key count (<= 0). Per IEEE 1800 § 15.3, key counts must be positive integers >= 1. Requesting 0 keys causes immediate simulation deadlock or undefined simulator scheduling behavior."
                ),
                c.loc(first),
            );
        }
    }
}

/// slang's `NewClassExpression`: `new`, `new(...)` or `pkg::cls::new(...)`,
/// with its argument list.
fn new_class(c: &Cst, e: NodeId) -> Option<(u32, Option<NodeId>)> {
    match c.kind(e) {
        NewExpr => {
            let kw = c.tok(e, "new")?;
            if c.tok(e, "[").is_some() || c.items(e).into_iter().any(|x| c.kind(x) != ArgList) {
                return None;
            }
            Some((kw, c.tree.child(e, ArgList)))
        }
        CallExpr => {
            let callee = c.tree.children(e).next()?;
            (c.kind(callee) == ScopedExpr && c.last_text(callee) == "new")
                .then(|| Some((c.first_tok(callee)?, c.tree.child(e, ArgList))))?
        }
        _ => None,
    }
}

fn check_mailbox_new(c: &Cst, s: &mut Sink, e: NodeId, var: &str) {
    let Some((at, args)) = new_class(c, e) else { return };
    let unbounded = match args.map(|a| (a, c.items(a).into_iter().find(|&x| c.kind(x) == Arg))) {
        None | Some((_, None)) => true,
        Some((a, Some(_))) => {
            // slang's first parameter, which may be an empty argument.
            let kids = c.kids(a);
            let open = kids.iter().position(|k| matches!(*k, Child::Token(t) if c.text_of(t) == "("));
            match open.and_then(|i| kids.get(i + 1)) {
                Some(Child::Node(arg)) if c.kind(*arg) == Arg => {
                    let s = c.trimmed(*arg);
                    s == "0" || s == "32'd0"
                }
                _ => false,
            }
        }
    };
    if unbounded {
        s.add(
            "SV-CON-007",
            "UnboundedMailboxInstantiation",
            Severity::Warning,
            format!(
                "Mailbox '{var}' is instantiated with an unbounded queue limit (bound = 0 or omitted). Per IEEE 1800 § 15.4.1, unbounded mailboxes allow producers to allocate unlimited transaction memory without blocking, risking memory exhaustion and masking downstream testbench stalls. Specify a bounded capacity (e.g. 'new(16)')."
            ),
            c.loc(at),
        );
    }
}

fn unbounded_mailbox(c: &Cst, s: &mut Sink) {
    // Declarations and assignments are visited in source order, as slang's
    // visitor reaches them; findings are sorted afterwards anyway.
    for n in c.nodes(DataDecl) {
        let Some(ty) = c.tree.child(n, DataType) else { continue };
        if c.last_text(ty) != "mailbox" && !c.text(ty).contains("mailbox") {
            continue;
        }
        for d in c.tree.children(n).filter(|&d| c.kind(d) == Declarator) {
            let Some(eq) = c.tok(d, "=") else { continue };
            let Some(init) = c.node_after(d, eq) else { continue };
            let var = c.tree.child(d, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
            check_mailbox_new(c, s, init, &var);
        }
    }
    for (_, lhs, rhs) in assignments(c) {
        let left = c.last_text(lhs);
        if left.contains("mailbox") || left.ends_with("_mb") || left.ends_with("_mbox") || left.starts_with("mb_") {
            check_mailbox_new(c, s, rhs, left);
        }
    }
}
