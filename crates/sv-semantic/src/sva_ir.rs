//! Bound, typed SystemVerilog Assertion (SVA) sequence and property IR (IEEE 1800-2023 Clause 16).
//!
//! [`SvaBinder`] lowers concurrent assertions (`assert property`, `assume property`,
//! `cover property`, `cover sequence`, `restrict property`), named `sequence` and
//! `property` declarations (expanding calls with formal substitution), `let`
//! expressions, `default clocking`, and `default disable iff` into bound
//! [`BoundProperty`] / [`PropExpr`] / [`SeqExpr`] trees whose leaf boolean and clock
//! expressions live in an [`ExprArena`].
//!
//! Automaton construction (NFA/DFA compilation, Büchi liveness, `$past` history
//! expansion) intentionally stays in downstream formal consumers.

use sv_syntax::{Child, NodeId, SyntaxKind, Tree};

use crate::elab_ir::{ElabItemKind, ElaboratedDesign, InstanceId};
use crate::expr::{ExprArena, ExprBinder, ExprDiag, ExprId};
use crate::fx::FxMap;
use crate::model::Loc;
use crate::types::CVal;

/// Clocking edge sensitivity for an SVA clock event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SvaEdge {
    /// `posedge clk`
    Posedge,
    /// `negedge clk`
    Negedge,
    /// `edge clk` or bare `@(clk)`
    AnyEdge,
}

/// Bound SVA clock event (`@(posedge clk [iff guard])`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SvaClock {
    /// Edge polarity.
    pub edge: SvaEdge,
    /// Bound clock signal expression in [`ExprArena`].
    pub signal: ExprId,
    /// Optional `iff` guard expression in [`ExprArena`].
    pub iff: Option<ExprId>,
}

/// Upper cycle bound on a delay or repetition range (`Const(n)` or `$` [`CycleBound::Unbounded`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CycleBound {
    /// Constant cycle count `n`.
    Const(u32),
    /// Unbounded `$`.
    Unbounded,
}

/// Closed cycle interval `[min : max]` (`##N`, `##[a:b]`, `##[a:$]`, `[*n:m]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CycleRange {
    /// Minimum cycle count.
    pub min: u32,
    /// Maximum cycle count or `$`.
    pub max: CycleBound,
}

impl CycleRange {
    /// Exact cycle count `[n : n]`.
    pub const fn exact(n: u32) -> Self {
        Self { min: n, max: CycleBound::Const(n) }
    }

    /// Unbounded interval `[min : $]`.
    pub const fn unbounded(min: u32) -> Self {
        Self { min, max: CycleBound::Unbounded }
    }
}

/// Sequence repetition flavor (`[*]`, `[->]`, `[=]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RepeatKind {
    /// Consecutive repetition `[*n]`, `[*n:m]`, `[*]`, `[+]`.
    Consecutive,
    /// Goto non-consecutive repetition `[->n]`, `[->n:m]`.
    Goto,
    /// Non-consecutive repetition `[=n]`, `[=n:m]`.
    NonConsecutive,
}

/// Bound SVA sequence expression (IEEE 1800-2023 §16.7 to §16.9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeqExpr {
    /// Single-cycle boolean condition (`ExprId` coerced to 1-bit in [`ExprArena`]).
    Bool(ExprId),
    /// Cycle delay `##range rhs` (`lhs == None`) or `lhs ##range rhs` (`range != 0..0`).
    Delay {
        /// Cycle delay range.
        range: CycleRange,
        /// Optional preceding sequence (`None` for leading `##N rhs`).
        lhs: Option<Box<SeqExpr>>,
        /// Delayed sequence.
        rhs: Box<SeqExpr>,
    },
    /// Zero-cycle sequence fusion `lhs ##0 rhs`.
    Fusion(Box<SeqExpr>, Box<SeqExpr>),
    /// Postfix sequence repetition `seq[*...]`, `seq[->...]`, `seq[=...]`.
    Repeat {
        /// Repetition operator kind.
        kind: RepeatKind,
        /// Cycle/match count range.
        range: CycleRange,
        /// Repeated sequence operand.
        seq: Box<SeqExpr>,
    },
    /// Non-length-matching sequence conjunction `s1 and s2`.
    And(Box<SeqExpr>, Box<SeqExpr>),
    /// Sequence disjunction `s1 or s2`.
    Or(Box<SeqExpr>, Box<SeqExpr>),
    /// Length-matching sequence conjunction `s1 intersect s2`.
    Intersect(Box<SeqExpr>, Box<SeqExpr>),
    /// Sequence containment `s1 within s2`.
    Within(Box<SeqExpr>, Box<SeqExpr>),
    /// Boolean invariant across sequence match `cond throughout seq`.
    Throughout {
        /// Boolean condition held on every tick of `seq`.
        cond: ExprId,
        /// Monitored sequence.
        seq: Box<SeqExpr>,
    },
    /// `first_match(seq)`.
    FirstMatch(Box<SeqExpr>),
    /// Explicitly clocked sub-sequence `@(posedge clk) seq`.
    Clocked {
        /// Clock event.
        clock: SvaClock,
        /// Inner sequence.
        seq: Box<SeqExpr>,
    },
}

/// Bound SVA property expression (IEEE 1800-2023 §16.12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropExpr {
    /// Sequence promoted to property (`weak(seq)` / plain sequence).
    Seq(SeqExpr),
    /// Property negation `not p`.
    Not(Box<PropExpr>),
    /// Explicit `strong(seq)`.
    Strong(SeqExpr),
    /// Explicit `weak(seq)`.
    Weak(SeqExpr),
    /// Property conjunction `p1 and p2`.
    And(Box<PropExpr>, Box<PropExpr>),
    /// Property disjunction `p1 or p2`.
    Or(Box<PropExpr>, Box<PropExpr>),
    /// Property `p1 implies p2`.
    Implies(Box<PropExpr>, Box<PropExpr>),
    /// Property equivalence `p1 iff p2`.
    Iff(Box<PropExpr>, Box<PropExpr>),
    /// Overlapped implication `lhs |-> rhs`.
    OverlappedImplication {
        /// Antecedent sequence.
        lhs: SeqExpr,
        /// Consequent property.
        rhs: Box<PropExpr>,
    },
    /// Non-overlapped implication `lhs |=> rhs`.
    NonOverlappedImplication {
        /// Antecedent sequence.
        lhs: SeqExpr,
        /// Consequent property.
        rhs: Box<PropExpr>,
    },
    /// Followed-by property `lhs #-# rhs` (`overlapped = true`) or `lhs #=# rhs` (`false`).
    FollowedBy {
        /// `true` for `#-#`, `false` for `#=#`.
        overlapped: bool,
        /// Prefix sequence.
        lhs: SeqExpr,
        /// Continuation property.
        rhs: Box<PropExpr>,
    },
    /// `nexttime [n] p` (`strong = false`) or `s_nexttime [n] p` (`strong = true`).
    Nexttime {
        /// Strong vs weak liveness flavor.
        strong: bool,
        /// Number of clock ticks (`1` by default).
        cycles: u32,
        /// Target property.
        prop: Box<PropExpr>,
    },
    /// `always [range] p` (`strong = false`) or `s_always [range] p` (`strong = true`).
    Always {
        /// Strong vs weak flavor.
        strong: bool,
        /// Active cycle window (`0..$` for unbounded `always`).
        range: CycleRange,
        /// Inner property.
        prop: Box<PropExpr>,
    },
    /// `eventually [range] p` (`strong = false`) or `s_eventually [range] p` (`strong = true`).
    Eventually {
        /// Strong vs weak flavor.
        strong: bool,
        /// Active cycle window (`0..$` for unbounded `s_eventually`).
        range: CycleRange,
        /// Inner property.
        prop: Box<PropExpr>,
    },
    /// `p1 until p2`, `p1 s_until p2`, `p1 until_with p2`, `p1 s_until_with p2`.
    Until {
        /// `true` for `s_until` / `s_until_with`.
        strong: bool,
        /// `true` for `until_with` / `s_until_with`.
        overlapping: bool,
        /// Left property held until `rhs`.
        lhs: Box<PropExpr>,
        /// Terminating property.
        rhs: Box<PropExpr>,
    },
    /// Conditional property `if (cond) then_prop [else else_prop]`.
    IfElse {
        /// 1-bit condition expression.
        cond: ExprId,
        /// Property when `cond` holds.
        then_prop: Box<PropExpr>,
        /// Optional property when `cond` is false.
        else_prop: Option<Box<PropExpr>>,
    },
    /// Abort property `accept_on (cond) p`, `reject_on (cond) p`, `sync_accept_on`, `sync_reject_on`.
    AcceptOn {
        /// `true` for `sync_accept_on` / `sync_reject_on`.
        sync: bool,
        /// `true` for `reject_on` / `sync_reject_on`.
        reject: bool,
        /// Abort condition expression.
        cond: ExprId,
        /// Guarded property.
        prop: Box<PropExpr>,
    },
    /// Explicitly clocked sub-property `@(posedge clk) prop`.
    Clocked {
        /// Clock event.
        clock: SvaClock,
        /// Inner property.
        prop: Box<PropExpr>,
    },
}

/// Verification directive kind on a [`BoundProperty`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropKind {
    /// `assert property`
    Assert,
    /// `assume property`
    Assume,
    /// `cover property` / `cover sequence`
    Cover,
    /// `restrict property`
    Restrict,
}

/// Complete bound concurrent SVA assertion, assumption, cover, or restriction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundProperty {
    /// Verification directive kind.
    pub kind: PropKind,
    /// Optional statement label (`lbl: assert property (...)`).
    pub label: Option<String>,
    /// Elaborated instance owning this property.
    pub instance: InstanceId,
    /// Lexical generate-block prefix inside `instance` (`""` at module root).
    pub scope_prefix: String,
    /// Effective top-level clock (explicit `@(...)`, procedural block clock, or `default clocking`).
    pub clock: Option<SvaClock>,
    /// Effective asynchronous reset / abort condition (explicit `disable iff` or `default disable iff`).
    pub disable_iff: Option<ExprId>,
    /// Bound property body.
    pub expr: PropExpr,
    /// Source location (`1`-based line/column).
    pub loc: Loc,
}

const MAX_SVA_EXPAND_DEPTH: usize = 32;

#[derive(Clone, Debug)]
enum SvaValue {
    Seq(SeqExpr),
    Prop { prop: PropExpr, disable_iff: Option<ExprId> },
}

impl SvaValue {
    fn into_seq(self) -> Result<SeqExpr, &'static str> {
        match self {
            SvaValue::Seq(s) => Ok(s),
            SvaValue::Prop { disable_iff: Some(_), .. } => {
                Err("`disable iff` is only legal at the top level of a property")
            }
            SvaValue::Prop { disable_iff: None, .. } => {
                Err("property operator cannot appear inside a sequence operand")
            }
        }
    }
}

#[derive(Clone, Debug)]
struct ActualSubst {
    /// Pre-bound expression if the actual was a plain expression (usable inside boolean subexpressions).
    expr_id: Option<ExprId>,
    /// Pre-bound SVA sequence/property value for direct sequence/property parameter references.
    sva_val: SvaValue,
}

#[derive(Clone, Debug)]
enum PrefixPropOp {
    Not,
    Nexttime { strong: bool, cycles: u32 },
    Always { strong: bool, range: CycleRange },
    Eventually { strong: bool, range: CycleRange },
    AcceptOn { sync: bool, reject: bool, cond: ExprId },
}

impl PrefixPropOp {
    fn prefix_bp(&self) -> u8 {
        match self {
            Self::Not => 55,
            Self::Nexttime { .. } => 24,
            Self::Always { .. } | Self::Eventually { .. } => 8,
            Self::AcceptOn { .. } => 5,
        }
    }
}

#[derive(Clone, Debug)]
enum ChainItem<'t> {
    Delay(CycleRange, Loc),
    PrefixOp(PrefixPropOp, Loc),
    BinOp(&'t str, Loc),
    Atom(NodeId, Vec<NodeId>),
}

/// Binder turning concurrent SVA syntax nodes into [`BoundProperty`] trees backed by an [`ExprArena`].
pub struct SvaBinder<'a> {
    /// Parsed CST trees of the compilation.
    pub trees: &'a [Tree],
    /// Optional out-of-line CST tree at index `trees.len()` (used by `bind_property_in_scope`).
    pub extra_tree: Option<&'a Tree>,
    /// Elaborated design arena.
    pub design: &'a ElaboratedDesign,
    /// Shared expression arena for boolean leaves, clocks, and `disable iff` guards.
    pub arena: &'a mut ExprArena,
    /// Fail-closed diagnostics emitted while binding SVA properties.
    pub diags: Vec<ExprDiag>,
    cur_pkg: Option<String>,
}

impl<'a> SvaBinder<'a> {
    /// Creates a new [`SvaBinder`] over `(trees, design, arena)`.
    pub fn new(trees: &'a [Tree], design: &'a ElaboratedDesign, arena: &'a mut ExprArena) -> Self {
        Self { trees, extra_tree: None, design, arena, diags: Vec::new(), cur_pkg: None }
    }

    /// Attaches an out-of-line [`Tree`] addressable at index `self.trees.len()`.
    pub fn with_extra_tree(mut self, extra: &'a Tree) -> Self {
        self.extra_tree = Some(extra);
        self
    }

    /// Borrows CST tree `idx` (falling back to `extra_tree` when `idx == trees.len()`).
    ///
    /// # Panics
    ///
    /// Panics if `idx > self.trees.len()`, or if `idx == self.trees.len()` and `self.extra_tree` is `None`.
    #[inline]
    pub fn tree(&self, idx: usize) -> &'a Tree {
        if idx == self.trees.len() {
            self.extra_tree.expect("extra_tree index")
        } else {
            &self.trees[idx]
        }
    }

    /// Binds every concurrent assertion inside `inst_id` (both module/generate-level and procedural).
    ///
    /// Assertions that fail binding are omitted from the returned vector after recording one or
    /// more [`ExprDiag`] entries in `self.diags`. Callers must inspect `self.diags` after calling
    /// this method rather than treating a shorter returned vector as clean completion.
    pub fn bind_instance_properties(&mut self, inst_id: InstanceId) -> Vec<BoundProperty> {
        let items = self.design.instance(inst_id).items.clone();
        let mut out = Vec::new();
        for item in &items {
            match item.kind {
                ElabItemKind::ConcurrentAssert => {
                    if let Some(bp) = self.bind_concurrent_assert(
                        item.tree,
                        item.node,
                        inst_id,
                        &item.scope_prefix,
                        &item.gen_consts,
                        None,
                    ) {
                        out.push(bp);
                    }
                }
                ElabItemKind::ProceduralBlock => {
                    let inherited_clk = self.extract_procedural_clock(
                        item.tree,
                        item.node,
                        inst_id,
                        &item.scope_prefix,
                        &item.gen_consts,
                    );
                    let mut ca_nodes = Vec::new();
                    Self::collect_procedural_concurrent_asserts(self.tree(item.tree), item.node, &mut ca_nodes);
                    for ca in ca_nodes {
                        if let Some(bp) = self.bind_concurrent_assert(
                            item.tree,
                            ca,
                            inst_id,
                            &item.scope_prefix,
                            &item.gen_consts,
                            inherited_clk.clone(),
                        ) {
                            out.push(bp);
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Binds one `ConcurrentAssert` CST node (`assert property (...)`, `assume property (...)`, etc.).
    ///
    /// Returns `None` and appends one or more [`ExprDiag`] entries to `self.diags` on any syntax,
    /// elaboration, or typing failure. Callers must inspect `self.diags` whenever `None` is returned.
    pub fn bind_concurrent_assert(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        inherited_clock: Option<SvaClock>,
    ) -> Option<BoundProperty> {
        let loc = self.loc_of(tree_idx, node, inst_id, scope_prefix, gen_consts);
        let t = self.tree(tree_idx);
        let mut label =
            t.child(node, SyntaxKind::StmtLabel).map(|l| t.bare_text(l).trim_end_matches(':').trim().to_string());
        // If enclosed in a labeled parent statement (`lbl: assert property ...`), check own StmtLabel tokens too.
        if label.as_deref() == Some("") {
            label = None;
        }
        let own = Self::own_toks(t, node);
        let kw = own
            .iter()
            .copied()
            .find(|k| matches!(*k, "assert" | "assume" | "cover" | "restrict" | "expect"))
            .unwrap_or("assert");
        let kind = match kw {
            "assert" => PropKind::Assert,
            "assume" => PropKind::Assume,
            "cover" => PropKind::Cover,
            "restrict" => PropKind::Restrict,
            "expect" => {
                self.unsup(loc, "'expect' procedural blocking property statement is not synthesizable/formal");
                return None;
            }
            _ => PropKind::Assert,
        };
        let Some(prop_spec) = t.child(node, SyntaxKind::PropertyExpr) else {
            self.err(loc, "missing property specification in concurrent assertion");
            return None;
        };
        let substs = FxMap::default();
        let (explicit_clk, explicit_dis, mut expr) =
            self.bind_property_spec(tree_idx, prop_spec, inst_id, scope_prefix, gen_consts, &substs, 0)?;

        // Lift leading `PropExpr::Clocked` if written inside parentheses `@(posedge clk) ...`.
        let inner_clk = match &expr {
            PropExpr::Clocked { clock, .. } if explicit_clk.is_none() => Some(clock.clone()),
            _ => None,
        };
        if inner_clk.is_some() {
            if let PropExpr::Clocked { prop, .. } = expr {
                expr = *prop;
            }
        }

        let clock = explicit_clk
            .or(inner_clk)
            .or(inherited_clock)
            .or_else(|| self.resolve_default_clocking(inst_id, scope_prefix, gen_consts, 0));
        let disable_iff = explicit_dis.or_else(|| self.resolve_default_disable(inst_id, scope_prefix, gen_consts));

        Some(BoundProperty {
            kind,
            label,
            instance: inst_id,
            scope_prefix: scope_prefix.to_string(),
            clock,
            disable_iff,
            expr,
            loc,
        })
    }

    fn unwrap_sub_prop(&mut self, val: SvaValue, loc: Loc) -> Option<PropExpr> {
        match val {
            SvaValue::Seq(s) => Some(PropExpr::Seq(s)),
            SvaValue::Prop { prop, disable_iff: None } => Some(prop),
            SvaValue::Prop { disable_iff: Some(_), .. } => {
                self.err(loc, "`disable iff` is only legal at the top level of a property");
                None
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn bind_property_spec(
        &mut self,
        tree_idx: usize,
        prop_spec: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
        depth: usize,
    ) -> Option<(Option<SvaClock>, Option<ExprId>, PropExpr)> {
        let t = self.tree(tree_idx);
        let mut clock = None;
        if let Some(tc) = t.child(prop_spec, SyntaxKind::TimingControl) {
            clock = Some(self.bind_clock_timing(tree_idx, tc, inst_id, scope_prefix, gen_consts, substs, 0)?);
        }
        let mut disable_iff = None;
        if let Some(di) = t.child(prop_spec, SyntaxKind::DisableIff) {
            if let Some(pe) = Self::expr_kids(t, di).into_iter().next() {
                disable_iff = Some(self.bind_bool_expr(tree_idx, pe, inst_id, scope_prefix, gen_consts, substs)?);
            }
        }
        let loc = self.loc_of(tree_idx, prop_spec, inst_id, scope_prefix, gen_consts);
        let Some(seq_node) = t.child(prop_spec, SyntaxKind::SequenceExpr) else {
            self.err(loc, "missing property expression");
            return None;
        };
        let val = self.bind_seq_or_prop(tree_idx, seq_node, inst_id, scope_prefix, gen_consts, substs, depth)?;
        match val {
            SvaValue::Seq(s) => Some((clock, disable_iff, PropExpr::Seq(s))),
            SvaValue::Prop { prop, disable_iff: inner_dis } => {
                if inner_dis.is_some() && disable_iff.is_some() {
                    self.err(loc, "`disable iff` is only legal at the top level of a property");
                    return None;
                }
                Some((clock, disable_iff.or(inner_dis), prop))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn bind_seq_or_prop(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
        depth: usize,
    ) -> Option<SvaValue> {
        let t = self.tree(tree_idx);
        if matches!(t.kind(node), SyntaxKind::RangeList | SyntaxKind::DistItem) {
            let loc = self.loc_of(tree_idx, node, inst_id, scope_prefix, gen_consts);
            self.err(loc, "dist expressions in assertions are unsupported");
            return None;
        }
        if t.kind(node) != SyntaxKind::SequenceExpr {
            let e = self.bind_bool_expr(tree_idx, node, inst_id, scope_prefix, gen_consts, substs)?;
            return Some(SvaValue::Seq(SeqExpr::Bool(e)));
        }

        let loc = self.loc_of(tree_idx, node, inst_id, scope_prefix, gen_consts);
        let mut chain: Vec<ChainItem<'a>> = Vec::new();
        self.flatten_chain(tree_idx, node, inst_id, scope_prefix, gen_consts, substs, &mut chain)?;
        if chain.is_empty() {
            self.err(loc, "empty SVA sequence or property expression");
            return None;
        }
        let mut cursor = 0usize;
        let val =
            self.parse_chain(&chain, &mut cursor, 0, tree_idx, inst_id, scope_prefix, gen_consts, substs, depth)?;
        if cursor < chain.len() {
            let tail_loc = self.chain_item_loc(&chain[cursor], tree_idx, inst_id, scope_prefix, gen_consts);
            self.err(tail_loc, "unexpected trailing SVA expression or operator");
            return None;
        }
        Some(val)
    }

    fn chain_item_loc(
        &mut self,
        item: &ChainItem<'_>,
        tree_idx: usize,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
    ) -> Loc {
        match *item {
            ChainItem::Delay(_, loc) | ChainItem::PrefixOp(_, loc) | ChainItem::BinOp(_, loc) => loc,
            ChainItem::Atom(n, _) => self.loc_of(tree_idx, n, inst_id, scope_prefix, gen_consts),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn flatten_chain(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
        chain: &mut Vec<ChainItem<'a>>,
    ) -> Option<()> {
        let t = self.tree(tree_idx);
        let kids = Self::expr_kids(t, node);
        let own = Self::own_toks(t, node);

        if own.contains(&"dist")
            || kids.iter().any(|&c| matches!(t.kind(c), SyntaxKind::RangeList | SyntaxKind::DistItem))
        {
            let dloc = self.loc_of(tree_idx, node, inst_id, scope_prefix, gen_consts);
            self.err(dloc, "dist expressions in assertions are unsupported");
            return None;
        }

        // 1. Greedy unary prefix property operators (`PROP_UNARY` and `ACCEPT_ON`) flatten into `PrefixOp`
        // followed by their trailing `SequenceExpr` so Pratt `parse_chain` applies IEEE 1800 Table 16-1 precedence.
        if let Some(&first_tok) = own.first() {
            match first_tok {
                "not" => {
                    let loc = self.loc_of(tree_idx, node, inst_id, scope_prefix, gen_consts);
                    let inner = *kids.last()?;
                    chain.push(ChainItem::PrefixOp(PrefixPropOp::Not, loc));
                    return self.flatten_chain(tree_idx, inner, inst_id, scope_prefix, gen_consts, substs, chain);
                }
                "nexttime" | "s_nexttime" => {
                    let loc = self.loc_of(tree_idx, node, inst_id, scope_prefix, gen_consts);
                    let strong = first_tok == "s_nexttime";
                    let cycles = match t.child(node, SyntaxKind::Repetition) {
                        Some(rep) => {
                            let rloc = self.loc_of(tree_idx, rep, inst_id, scope_prefix, gen_consts);
                            let own_rep = Self::own_toks(t, rep);
                            if own_rep.contains(&":")
                                || own_rep.contains(&"$")
                                || own_rep.contains(&"*")
                                || own_rep.contains(&"+")
                            {
                                self.err(
                                    rloc,
                                    format!("'{first_tok}' requires a single constant cycle count, not a range"),
                                );
                                return None;
                            }
                            let r = self.eval_cycle_range(tree_idx, rep, inst_id, scope_prefix, gen_consts, substs)?;
                            if r.max != CycleBound::Const(r.min) {
                                self.err(
                                    rloc,
                                    format!("'{first_tok}' requires a single constant cycle count, not a range"),
                                );
                                return None;
                            }
                            r.min
                        }
                        None => 1,
                    };
                    let inner = *kids.iter().rfind(|&&c| t.kind(c) == SyntaxKind::SequenceExpr)?;
                    chain.push(ChainItem::PrefixOp(PrefixPropOp::Nexttime { strong, cycles }, loc));
                    return self.flatten_chain(tree_idx, inner, inst_id, scope_prefix, gen_consts, substs, chain);
                }
                "always" | "s_always" | "eventually" | "s_eventually" => {
                    let loc = self.loc_of(tree_idx, node, inst_id, scope_prefix, gen_consts);
                    let strong = first_tok.starts_with("s_");
                    let is_always = first_tok.ends_with("always");
                    let range = match t.child(node, SyntaxKind::Repetition) {
                        Some(rep) => self.eval_cycle_range(tree_idx, rep, inst_id, scope_prefix, gen_consts, substs)?,
                        None => CycleRange::unbounded(0),
                    };
                    let inner = *kids.iter().rfind(|&&c| t.kind(c) == SyntaxKind::SequenceExpr)?;
                    let op = if is_always {
                        PrefixPropOp::Always { strong, range }
                    } else {
                        PrefixPropOp::Eventually { strong, range }
                    };
                    chain.push(ChainItem::PrefixOp(op, loc));
                    return self.flatten_chain(tree_idx, inner, inst_id, scope_prefix, gen_consts, substs, chain);
                }
                "accept_on" | "reject_on" | "sync_accept_on" | "sync_reject_on" => {
                    let loc = self.loc_of(tree_idx, node, inst_id, scope_prefix, gen_consts);
                    let sync = first_tok.starts_with("sync_");
                    let reject = first_tok.contains("reject");
                    let cond_node = *kids.first()?;
                    let cond = self.bind_bool_expr(tree_idx, cond_node, inst_id, scope_prefix, gen_consts, substs)?;
                    let inner = *kids.last()?;
                    chain.push(ChainItem::PrefixOp(PrefixPropOp::AcceptOn { sync, reject, cond }, loc));
                    return self.flatten_chain(tree_idx, inner, inst_id, scope_prefix, gen_consts, substs, chain);
                }
                "strong" | "weak" if own.contains(&"(") => {
                    chain.push(ChainItem::Atom(node, Vec::new()));
                    return Some(());
                }
                "first_match" | "if" | "case" | "(" | "posedge" | "negedge" | "edge" => {
                    chain.push(ChainItem::Atom(node, Vec::new()));
                    return Some(());
                }
                _ => {}
            }
        }

        // 2. Self-contained clocked operand `@(posedge clk) prop_or_seq`.
        if kids.len() == 2
            && t.kind(kids[0]) == SyntaxKind::TimingControl
            && t.kind(kids[1]) == SyntaxKind::SequenceExpr
            && !t.bare_text(kids[0]).trim_start().starts_with("##")
        {
            chain.push(ChainItem::Atom(node, Vec::new()));
            return Some(());
        }

        // 3. Leaf expression operand (`SequenceExpr` wrapping a non-`SequenceExpr` CST expression node).
        if own.is_empty()
            && kids.len() == 1
            && !matches!(t.kind(kids[0]), SyntaxKind::SequenceExpr | SyntaxKind::TimingControl)
        {
            chain.push(ChainItem::Atom(node, Vec::new()));
            return Some(());
        }

        // 4. `property_expr` chain of `TimingControl(##...)`, operand `SequenceExpr`s, postfix `Repetition`s, and binary ops.
        let raw_children = &t.node(node).children;
        for (idx, ch) in raw_children.iter().enumerate() {
            match *ch {
                Child::Token(ti) => {
                    let tok = &t.tokens[ti as usize];
                    if tok.kind.is_trivia() {
                        continue;
                    }
                    let txt = &*tok.text;
                    let loc = self.loc_of_tok(tree_idx, ti, inst_id, scope_prefix, gen_consts);
                    if is_sva_binary_op(txt) {
                        chain.push(ChainItem::BinOp(txt, loc));
                    } else {
                        self.err(loc, format!("unexpected token '{txt}' in SVA expression"));
                        return None;
                    }
                }
                Child::Node(c) => {
                    match t.kind(c) {
                        SyntaxKind::Attribute => {}
                        SyntaxKind::RangeList | SyntaxKind::DistItem => {
                            let loc = self.loc_of(tree_idx, c, inst_id, scope_prefix, gen_consts);
                            self.err(loc, "dist expressions in assertions are unsupported");
                            return None;
                        }
                        SyntaxKind::TimingControl => {
                            let loc = self.loc_of(tree_idx, c, inst_id, scope_prefix, gen_consts);
                            let r = self.eval_delay_timing(tree_idx, c, inst_id, scope_prefix, gen_consts, substs)?;
                            chain.push(ChainItem::Delay(r, loc));
                        }
                        SyntaxKind::Repetition => {
                            if let Some(ChainItem::Atom(_, reps)) = chain.last_mut() {
                                reps.push(c);
                            } else {
                                let loc = self.loc_of(tree_idx, c, inst_id, scope_prefix, gen_consts);
                                self.err(loc, "repetition can only apply to a sequence operand");
                                return None;
                            }
                        }
                        SyntaxKind::SequenceExpr => {
                            let followed_by_rep = raw_children[(idx + 1)..]
                            .iter()
                            .find(|next_ch| match **next_ch {
                                Child::Token(ti) => !t.tokens[ti as usize].kind.is_trivia(),
                                Child::Node(nc) => t.kind(nc) != SyntaxKind::Attribute,
                            })
                            .is_some_and(|next_ch| matches!(*next_ch, Child::Node(nc) if t.kind(nc) == SyntaxKind::Repetition));
                            if followed_by_rep {
                                chain.push(ChainItem::Atom(c, Vec::new()));
                            } else {
                                self.flatten_chain(tree_idx, c, inst_id, scope_prefix, gen_consts, substs, chain)?;
                            }
                        }
                        _ => {
                            chain.push(ChainItem::Atom(c, Vec::new()));
                        }
                    }
                }
            }
        }
        Some(())
    }

    #[allow(clippy::too_many_arguments)]
    fn bind_atom_node(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
        depth: usize,
    ) -> Option<SvaValue> {
        let t = self.tree(tree_idx);
        if t.kind(node) != SyntaxKind::SequenceExpr {
            let e = self.bind_bool_expr(tree_idx, node, inst_id, scope_prefix, gen_consts, substs)?;
            return Some(SvaValue::Seq(SeqExpr::Bool(e)));
        }

        let kids = Self::expr_kids(t, node);
        let own = Self::own_toks(t, node);
        let loc = self.loc_of(tree_idx, node, inst_id, scope_prefix, gen_consts);

        if own.contains(&"dist")
            || kids.iter().any(|&c| matches!(t.kind(c), SyntaxKind::RangeList | SyntaxKind::DistItem))
        {
            let dloc = self.loc_of(tree_idx, node, inst_id, scope_prefix, gen_consts);
            self.err(dloc, "dist expressions in assertions are unsupported");
            return None;
        }

        if let Some(&first_tok) = own.first() {
            match first_tok {
                "strong" | "weak" if own.contains(&"(") => {
                    let inner = *kids.iter().find(|&&c| t.kind(c) == SyntaxKind::SequenceExpr)?;
                    let v = self.bind_seq_or_prop(tree_idx, inner, inst_id, scope_prefix, gen_consts, substs, depth)?;
                    let seq = match v.into_seq() {
                        Ok(s) => s,
                        Err(msg) => {
                            self.err(loc, msg);
                            return None;
                        }
                    };
                    return Some(SvaValue::Prop {
                        prop: if first_tok == "strong" { PropExpr::Strong(seq) } else { PropExpr::Weak(seq) },
                        disable_iff: None,
                    });
                }
                "first_match" => {
                    if own.contains(&",") || kids.len() > 1 {
                        self.unsup(loc, "sequence match-item assignments inside first_match(...) are not supported");
                        return None;
                    }
                    let inner = *kids.first()?;
                    let v = self.bind_seq_or_prop(tree_idx, inner, inst_id, scope_prefix, gen_consts, substs, depth)?;
                    let seq = match v.into_seq() {
                        Ok(s) => s,
                        Err(msg) => {
                            self.err(loc, msg);
                            return None;
                        }
                    };
                    return Some(SvaValue::Seq(SeqExpr::FirstMatch(Box::new(seq))));
                }
                "if" => {
                    let cond_node = *kids.first()?;
                    let cond = self.bind_bool_expr(tree_idx, cond_node, inst_id, scope_prefix, gen_consts, substs)?;
                    let seq_kids: Vec<NodeId> =
                        kids.iter().copied().skip(1).filter(|&c| t.kind(c) == SyntaxKind::SequenceExpr).collect();
                    let then_node = *seq_kids.first()?;
                    let then_loc = self.loc_of(tree_idx, then_node, inst_id, scope_prefix, gen_consts);
                    let then_val =
                        self.bind_seq_or_prop(tree_idx, then_node, inst_id, scope_prefix, gen_consts, substs, depth)?;
                    let then_prop = self.unwrap_sub_prop(then_val, then_loc)?;
                    let else_prop = if let Some(&el) = seq_kids.get(1) {
                        let el_loc = self.loc_of(tree_idx, el, inst_id, scope_prefix, gen_consts);
                        let el_val =
                            self.bind_seq_or_prop(tree_idx, el, inst_id, scope_prefix, gen_consts, substs, depth)?;
                        Some(Box::new(self.unwrap_sub_prop(el_val, el_loc)?))
                    } else {
                        None
                    };
                    return Some(SvaValue::Prop {
                        prop: PropExpr::IfElse { cond, then_prop: Box::new(then_prop), else_prop },
                        disable_iff: None,
                    });
                }
                "case" => {
                    return self.bind_property_case(tree_idx, node, inst_id, scope_prefix, gen_consts, substs, depth);
                }
                "(" => {
                    if own.contains(&",") || kids.len() > 1 {
                        self.unsup(loc, "sequence local-variable match items '(seq, v = expr)' are not supported");
                        return None;
                    }
                    let inner = *kids.first()?;
                    return self.bind_seq_or_prop(tree_idx, inner, inst_id, scope_prefix, gen_consts, substs, depth);
                }
                "posedge" | "negedge" | "edge" => {
                    self.err(loc, format!("unexpected event keyword '{first_tok}' outside clocking event"));
                    return None;
                }
                _ => {
                    // A prefix unary operator wrapped directly inside postfix repetition (`(not a)` vs ungrouped)
                    // delegates back to `bind_seq_or_prop`.
                    return self.bind_seq_or_prop(tree_idx, node, inst_id, scope_prefix, gen_consts, substs, depth);
                }
            }
        }

        // Clocked operand `@(posedge clk) prop_or_seq`.
        if kids.len() == 2
            && t.kind(kids[0]) == SyntaxKind::TimingControl
            && t.kind(kids[1]) == SyntaxKind::SequenceExpr
            && !t.bare_text(kids[0]).trim_start().starts_with("##")
        {
            let clk = self.bind_clock_timing(tree_idx, kids[0], inst_id, scope_prefix, gen_consts, substs, 0)?;
            let inner_loc = self.loc_of(tree_idx, kids[1], inst_id, scope_prefix, gen_consts);
            let inner = self.bind_seq_or_prop(tree_idx, kids[1], inst_id, scope_prefix, gen_consts, substs, depth)?;
            return Some(match inner {
                SvaValue::Seq(s) => SvaValue::Seq(SeqExpr::Clocked { clock: clk, seq: Box::new(s) }),
                other => {
                    let p = self.unwrap_sub_prop(other, inner_loc)?;
                    SvaValue::Prop { prop: PropExpr::Clocked { clock: clk, prop: Box::new(p) }, disable_iff: None }
                }
            });
        }

        // Leaf expression operand (`SequenceExpr` wrapping a non-`SequenceExpr` CST expression node).
        if kids.len() == 1 && !matches!(t.kind(kids[0]), SyntaxKind::SequenceExpr | SyntaxKind::TimingControl) {
            return self.bind_leaf_operand(tree_idx, kids[0], inst_id, scope_prefix, gen_consts, substs, depth);
        }

        self.bind_seq_or_prop(tree_idx, node, inst_id, scope_prefix, gen_consts, substs, depth)
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_chain(
        &mut self,
        chain: &[ChainItem<'_>],
        cursor: &mut usize,
        min_bp: u8,
        tree_idx: usize,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
        depth: usize,
    ) -> Option<SvaValue> {
        let first = chain.get(*cursor)?.clone();
        *cursor += 1;
        let mut lhs = match first {
            ChainItem::Delay(range, loc) => {
                let rhs_val =
                    self.parse_chain(chain, cursor, 81, tree_idx, inst_id, scope_prefix, gen_consts, substs, depth)?;
                let rhs_seq = match rhs_val.into_seq() {
                    Ok(s) => s,
                    Err(msg) => {
                        self.err(loc, msg);
                        return None;
                    }
                };
                SvaValue::Seq(SeqExpr::Delay { range, lhs: None, rhs: Box::new(rhs_seq) })
            }
            ChainItem::PrefixOp(op, loc) => {
                let p_bp = op.prefix_bp();
                let rhs_val =
                    self.parse_chain(chain, cursor, p_bp, tree_idx, inst_id, scope_prefix, gen_consts, substs, depth)?;
                let inner_prop = self.unwrap_sub_prop(rhs_val, loc)?;
                let prop = match op {
                    PrefixPropOp::Not => PropExpr::Not(Box::new(inner_prop)),
                    PrefixPropOp::Nexttime { strong, cycles } => {
                        PropExpr::Nexttime { strong, cycles, prop: Box::new(inner_prop) }
                    }
                    PrefixPropOp::Always { strong, range } => {
                        PropExpr::Always { strong, range, prop: Box::new(inner_prop) }
                    }
                    PrefixPropOp::Eventually { strong, range } => {
                        PropExpr::Eventually { strong, range, prop: Box::new(inner_prop) }
                    }
                    PrefixPropOp::AcceptOn { sync, reject, cond } => {
                        PropExpr::AcceptOn { sync, reject, cond, prop: Box::new(inner_prop) }
                    }
                };
                SvaValue::Prop { prop, disable_iff: None }
            }
            ChainItem::Atom(op_node, reps) => {
                let mut val =
                    self.bind_atom_node(tree_idx, op_node, inst_id, scope_prefix, gen_consts, substs, depth)?;
                for rep in reps {
                    let loc = self.loc_of(tree_idx, rep, inst_id, scope_prefix, gen_consts);
                    let seq = match val.into_seq() {
                        Ok(s) => s,
                        Err(msg) => {
                            self.err(loc, msg);
                            return None;
                        }
                    };
                    let (kind, range) =
                        self.eval_repetition(tree_idx, rep, inst_id, scope_prefix, gen_consts, substs)?;
                    val = SvaValue::Seq(SeqExpr::Repeat { kind, range, seq: Box::new(seq) });
                }
                val
            }
            ChainItem::BinOp(op, loc) => {
                self.err(loc, format!("unexpected SVA binary operator '{op}'"));
                return None;
            }
        };

        while let Some(next) = chain.get(*cursor) {
            let (l_bp, r_bp) = match next {
                ChainItem::Delay(_, _) => (80u8, 81u8),
                ChainItem::BinOp(op, _) => sva_infix_bp(op),
                ChainItem::Atom(_, _) | ChainItem::PrefixOp(_, _) => break,
            };
            if l_bp < min_bp {
                break;
            }
            let op_item = next.clone();
            *cursor += 1;
            let rhs =
                self.parse_chain(chain, cursor, r_bp, tree_idx, inst_id, scope_prefix, gen_consts, substs, depth)?;
            lhs = match op_item {
                ChainItem::Delay(range, loc) => {
                    let l_seq = match lhs.into_seq() {
                        Ok(s) => s,
                        Err(msg) => {
                            self.err(loc, msg);
                            return None;
                        }
                    };
                    let r_seq = match rhs.into_seq() {
                        Ok(s) => s,
                        Err(msg) => {
                            self.err(loc, msg);
                            return None;
                        }
                    };
                    if range.min == 0 && range.max == CycleBound::Const(0) {
                        SvaValue::Seq(SeqExpr::Fusion(Box::new(l_seq), Box::new(r_seq)))
                    } else {
                        SvaValue::Seq(SeqExpr::Delay { range, lhs: Some(Box::new(l_seq)), rhs: Box::new(r_seq) })
                    }
                }
                ChainItem::BinOp(op, loc) => self.combine_binop(op, lhs, rhs, loc)?,
                ChainItem::Atom(_, _) | ChainItem::PrefixOp(_, _) => unreachable!(),
            };
        }
        Some(lhs)
    }

    fn combine_binop(&mut self, op: &str, lhs: SvaValue, rhs: SvaValue, loc: Loc) -> Option<SvaValue> {
        match op {
            "throughout" => {
                let l_seq = match lhs.into_seq() {
                    Ok(s) => s,
                    Err(msg) => {
                        self.err(loc, msg);
                        return None;
                    }
                };
                let SeqExpr::Bool(cond) = l_seq else {
                    self.err(loc, "left operand of 'throughout' must be a boolean expression");
                    return None;
                };
                let r_seq = match rhs.into_seq() {
                    Ok(s) => s,
                    Err(msg) => {
                        self.err(loc, msg);
                        return None;
                    }
                };
                Some(SvaValue::Seq(SeqExpr::Throughout { cond, seq: Box::new(r_seq) }))
            }
            "within" | "intersect" => {
                let l_seq = match lhs.into_seq() {
                    Ok(s) => s,
                    Err(msg) => {
                        self.err(loc, msg);
                        return None;
                    }
                };
                let r_seq = match rhs.into_seq() {
                    Ok(s) => s,
                    Err(msg) => {
                        self.err(loc, msg);
                        return None;
                    }
                };
                Some(SvaValue::Seq(if op == "within" {
                    SeqExpr::Within(Box::new(l_seq), Box::new(r_seq))
                } else {
                    SeqExpr::Intersect(Box::new(l_seq), Box::new(r_seq))
                }))
            }
            "and" | "or" => match (lhs, rhs) {
                (SvaValue::Seq(a), SvaValue::Seq(b)) => Some(SvaValue::Seq(if op == "and" {
                    SeqExpr::And(Box::new(a), Box::new(b))
                } else {
                    SeqExpr::Or(Box::new(a), Box::new(b))
                })),
                (a, b) => {
                    let pa = self.unwrap_sub_prop(a, loc)?;
                    let pb = self.unwrap_sub_prop(b, loc)?;
                    let prop = if op == "and" {
                        PropExpr::And(Box::new(pa), Box::new(pb))
                    } else {
                        PropExpr::Or(Box::new(pa), Box::new(pb))
                    };
                    Some(SvaValue::Prop { prop, disable_iff: None })
                }
            },
            "|->" | "|=>" | "#-#" | "#=#" => {
                let l_seq = match lhs.into_seq() {
                    Ok(s) => s,
                    Err(msg) => {
                        if msg.contains("disable iff") {
                            self.err(loc, msg);
                        } else {
                            self.err(loc, format!("antecedent of '{op}' must be a sequence, not a property"));
                        }
                        return None;
                    }
                };
                let r_prop = Box::new(self.unwrap_sub_prop(rhs, loc)?);
                let prop = match op {
                    "|->" => PropExpr::OverlappedImplication { lhs: l_seq, rhs: r_prop },
                    "|=>" => PropExpr::NonOverlappedImplication { lhs: l_seq, rhs: r_prop },
                    "#-#" => PropExpr::FollowedBy { overlapped: true, lhs: l_seq, rhs: r_prop },
                    _ => PropExpr::FollowedBy { overlapped: false, lhs: l_seq, rhs: r_prop },
                };
                Some(SvaValue::Prop { prop, disable_iff: None })
            }
            "until" | "s_until" | "until_with" | "s_until_with" => {
                let pa = self.unwrap_sub_prop(lhs, loc)?;
                let pb = self.unwrap_sub_prop(rhs, loc)?;
                Some(SvaValue::Prop {
                    prop: PropExpr::Until {
                        strong: op.starts_with("s_"),
                        overlapping: op.ends_with("_with"),
                        lhs: Box::new(pa),
                        rhs: Box::new(pb),
                    },
                    disable_iff: None,
                })
            }
            "implies" => {
                let pa = self.unwrap_sub_prop(lhs, loc)?;
                let pb = self.unwrap_sub_prop(rhs, loc)?;
                Some(SvaValue::Prop { prop: PropExpr::Implies(Box::new(pa), Box::new(pb)), disable_iff: None })
            }
            "iff" => {
                let pa = self.unwrap_sub_prop(lhs, loc)?;
                let pb = self.unwrap_sub_prop(rhs, loc)?;
                Some(SvaValue::Prop { prop: PropExpr::Iff(Box::new(pa), Box::new(pb)), disable_iff: None })
            }
            _ => {
                self.err(loc, format!("unsupported SVA binary operator '{op}'"));
                None
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn bind_leaf_operand(
        &mut self,
        tree_idx: usize,
        mut expr_node: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
        depth: usize,
    ) -> Option<SvaValue> {
        let t = self.tree(tree_idx);
        while t.kind(expr_node) == SyntaxKind::ParenExpr {
            let kids = Self::expr_kids(t, expr_node);
            if kids.len() == 1 {
                expr_node = kids[0];
            } else {
                break;
            }
        }

        // 1. Bare or package-qualified identifier: check formal substitution or zero-arg named `sequence`/`property`/`let`.
        if t.kind(expr_node) == SyntaxKind::NameExpr {
            let raw = t.bare_text(expr_node);
            let ident = raw.trim().strip_prefix('\\').unwrap_or(raw.trim());
            if let Some(sub) = substs.get(ident) {
                return Some(sub.sva_val.clone());
            }
            if let Some((decl_kind, d_tree, d_node, d_prefix, d_consts, d_pkg)) =
                self.find_sva_decl(inst_id, scope_prefix, None, ident)
            {
                return self.expand_sva_decl(
                    decl_kind,
                    d_tree,
                    d_node,
                    inst_id,
                    &d_prefix,
                    &d_consts,
                    d_pkg,
                    tree_idx,
                    None,
                    scope_prefix,
                    gen_consts,
                    substs,
                    depth,
                );
            }
        } else if t.kind(expr_node) == SyntaxKind::ScopedExpr {
            let toks: Vec<&str> = t.token_range(expr_node).into_iter().map(|ti| &*t.tokens[ti as usize].text).collect();
            if let [pkg, "::", raw] = toks.as_slice() {
                let ident = raw.strip_prefix('\\').unwrap_or(raw);
                if let Some((decl_kind, d_tree, d_node, d_prefix, d_consts, d_pkg)) =
                    self.find_sva_decl(inst_id, scope_prefix, Some(pkg), ident)
                {
                    return self.expand_sva_decl(
                        decl_kind,
                        d_tree,
                        d_node,
                        inst_id,
                        &d_prefix,
                        &d_consts,
                        d_pkg,
                        tree_idx,
                        None,
                        scope_prefix,
                        gen_consts,
                        substs,
                        depth,
                    );
                }
            }
        }

        // 2. Call expression `seq_name(a, b)`, `pkg::prop_name(a, b)`, `prop_name(.x(req), .y(ack))`, or `let_name(x)`.
        if t.kind(expr_node) == SyntaxKind::CallExpr {
            let kids = Self::expr_kids(t, expr_node);
            if let Some(&callee) = kids.first() {
                let target: Option<(Option<String>, String)> = match t.kind(callee) {
                    SyntaxKind::NameExpr => {
                        let raw = t.bare_text(callee);
                        Some((None, raw.trim().strip_prefix('\\').unwrap_or(raw.trim()).to_string()))
                    }
                    SyntaxKind::ScopedExpr => {
                        let toks: Vec<&str> =
                            t.token_range(callee).into_iter().map(|ti| &*t.tokens[ti as usize].text).collect();
                        match toks.as_slice() {
                            [pkg, "::", raw] => {
                                Some((Some((*pkg).to_string()), raw.strip_prefix('\\').unwrap_or(raw).to_string()))
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                };
                if let Some((pkg_opt, ident)) = target {
                    if let Some((decl_kind, d_tree, d_node, d_prefix, d_consts, d_pkg)) =
                        self.find_sva_decl(inst_id, scope_prefix, pkg_opt.as_deref(), &ident)
                    {
                        let arg_list = t.child(expr_node, SyntaxKind::ArgList);
                        return self.expand_sva_decl(
                            decl_kind,
                            d_tree,
                            d_node,
                            inst_id,
                            &d_prefix,
                            &d_consts,
                            d_pkg,
                            tree_idx,
                            arg_list,
                            scope_prefix,
                            gen_consts,
                            substs,
                            depth,
                        );
                    }
                }
            }
        }

        let b = self.bind_bool_expr(tree_idx, expr_node, inst_id, scope_prefix, gen_consts, substs)?;
        Some(SvaValue::Seq(SeqExpr::Bool(b)))
    }

    #[allow(clippy::type_complexity)]
    fn find_sva_decl(
        &self,
        inst_id: InstanceId,
        scope_prefix: &str,
        explicit_pkg: Option<&str>,
        ident: &str,
    ) -> Option<(ElabItemKind, usize, NodeId, String, Vec<(String, CVal)>, Option<String>)> {
        let is_sva_kind = |k: ElabItemKind| {
            matches!(k, ElabItemKind::SequenceDecl | ElabItemKind::PropertyDecl | ElabItemKind::LetDecl)
        };
        let pkg_consts_of = |p: &crate::elab_ir::PackageScope| -> Vec<(String, CVal)> {
            let mut out = Vec::with_capacity(p.params.len() + p.enum_consts.len());
            for (nm, cv, _, _) in &p.params {
                out.push((nm.clone(), cv.clone()));
            }
            for (nm, cv, _) in &p.enum_consts {
                out.push((nm.clone(), cv.clone()));
            }
            out
        };

        if let Some(pkg_name) = explicit_pkg {
            let p = if pkg_name == "$unit" { &self.design.unit_scope } else { self.design.packages.get(pkg_name)? };
            for &(ref nm, k, tr, nd, _) in &p.subroutines {
                if is_sva_kind(k) && nm == ident {
                    return Some((k, tr, nd, String::new(), pkg_consts_of(p), Some(pkg_name.to_string())));
                }
            }
            return None;
        }

        if let Some(cur_p) = &self.cur_pkg {
            if let Some(p) = (cur_p == "$unit")
                .then_some(&self.design.unit_scope)
                .or_else(|| self.design.packages.get(cur_p.as_str()))
            {
                for &(ref nm, k, tr, nd, _) in &p.subroutines {
                    if is_sva_kind(k) && nm == ident {
                        return Some((k, tr, nd, String::new(), pkg_consts_of(p), Some(cur_p.clone())));
                    }
                }
                for (imp_pkg, imp_item) in &p.imports {
                    if imp_item.as_deref().is_none_or(|x| x == ident) {
                        if let Some(ip) = self.design.packages.get(imp_pkg) {
                            for &(ref nm, k, tr, nd, _) in &ip.subroutines {
                                if is_sva_kind(k) && nm == ident {
                                    return Some((k, tr, nd, String::new(), pkg_consts_of(ip), Some(imp_pkg.clone())));
                                }
                            }
                        }
                    }
                }
            }
        } else {
            let inst = self.design.instance(inst_id);
            let mut prefix = scope_prefix;
            loop {
                for item in &inst.items {
                    if item.scope_prefix != prefix {
                        continue;
                    }
                    if is_sva_kind(item.kind)
                        && Self::decl_name(self.tree(item.tree), item.node).as_deref() == Some(ident)
                    {
                        return Some((
                            item.kind,
                            item.tree,
                            item.node,
                            item.scope_prefix.clone(),
                            item.gen_consts.clone(),
                            None,
                        ));
                    }
                }
                for (sp, imp_pkg, imp_item) in &inst.imports {
                    if sp == prefix && imp_item.as_deref().is_none_or(|x| x == ident) {
                        if let Some(p) = self.design.packages.get(imp_pkg) {
                            for &(ref nm, k, tr, nd, _) in &p.subroutines {
                                if is_sva_kind(k) && nm == ident {
                                    return Some((k, tr, nd, String::new(), pkg_consts_of(p), Some(imp_pkg.clone())));
                                }
                            }
                        }
                    }
                }
                if prefix.is_empty() {
                    break;
                }
                prefix = match prefix.rfind('.') {
                    Some(i) => &prefix[..i],
                    None => "",
                };
            }
        }

        for &(ref nm, k, tr, nd, _) in &self.design.unit_scope.subroutines {
            if is_sva_kind(k) && nm == ident {
                return Some((
                    k,
                    tr,
                    nd,
                    String::new(),
                    pkg_consts_of(&self.design.unit_scope),
                    Some("$unit".to_string()),
                ));
            }
        }
        for (imp_pkg, imp_item) in &self.design.unit_scope.imports {
            if imp_item.as_deref().is_none_or(|x| x == ident) {
                if let Some(p) = self.design.packages.get(imp_pkg) {
                    for &(ref nm, k, tr, nd, _) in &p.subroutines {
                        if is_sva_kind(k) && nm == ident {
                            return Some((k, tr, nd, String::new(), pkg_consts_of(p), Some(imp_pkg.clone())));
                        }
                    }
                }
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn expand_sva_decl(
        &mut self,
        decl_kind: ElabItemKind,
        d_tree: usize,
        d_node: NodeId,
        inst_id: InstanceId,
        d_prefix: &str,
        d_consts: &[(String, CVal)],
        d_pkg: Option<String>,
        call_tree: usize,
        arg_list: Option<NodeId>,
        call_prefix: &str,
        call_consts: &[(String, CVal)],
        caller_substs: &FxMap<String, ActualSubst>,
        depth: usize,
    ) -> Option<SvaValue> {
        let loc = self.loc_of(d_tree, d_node, inst_id, d_prefix, d_consts);
        if depth >= MAX_SVA_EXPAND_DEPTH {
            self.err(loc, "recursive sequence/property instantiation exceeds maximum depth (32)");
            return None;
        }
        let call_loc = arg_list.map(|al| self.loc_of(call_tree, al, inst_id, call_prefix, call_consts)).unwrap_or(loc);
        let dt = self.tree(d_tree);
        let formals = extract_sva_formals(dt, d_node);
        let slots = crate::expr::parse_arg_list_slots(self.tree(call_tree), arg_list);
        let formal_names: Vec<&str> = formals.iter().map(|(n, _)| n.as_str()).collect();
        let callee_desc = match decl_kind {
            ElabItemKind::SequenceDecl => "sequence",
            ElabItemKind::PropertyDecl => "property",
            _ => "'let' construct",
        };
        let mapped = crate::expr::resolve_call_arg_map(&slots, &formal_names, callee_desc, call_loc, &mut self.diags)?;

        let mut next_substs = FxMap::default();
        for (pos, (fname, default_node)) in formals.iter().enumerate() {
            if let Some(act_node) = mapped[pos] {
                let bound = self.bind_actual_arg(
                    call_tree,
                    act_node,
                    inst_id,
                    call_prefix,
                    call_consts,
                    caller_substs,
                    depth + 1,
                )?;
                next_substs.insert(fname.clone(), bound);
            } else if let Some(def_node) = *default_node {
                let saved_pkg = std::mem::replace(&mut self.cur_pkg, d_pkg.clone());
                let bound_opt =
                    self.bind_actual_arg(d_tree, def_node, inst_id, d_prefix, d_consts, &FxMap::default(), depth + 1);
                self.cur_pkg = saved_pkg;
                next_substs.insert(fname.clone(), bound_opt?);
            } else {
                self.err(call_loc, format!("missing actual argument for formal '{fname}' in SVA instantiation"));
                return None;
            }
        }

        let saved_pkg = std::mem::replace(&mut self.cur_pkg, d_pkg);
        let res = (|| -> Option<SvaValue> {
            let dt = self.tree(d_tree);
            if decl_kind == ElabItemKind::LetDecl {
                let body_expr = Self::expr_kids(dt, d_node)
                    .into_iter()
                    .rfind(|&c| !matches!(dt.kind(c), SyntaxKind::Name | SyntaxKind::PortList))?;
                let b = self.bind_bool_expr(d_tree, body_expr, inst_id, d_prefix, d_consts, &next_substs)?;
                return Some(SvaValue::Seq(SeqExpr::Bool(b)));
            }

            let Some(prop_spec) = dt.child(d_node, SyntaxKind::PropertyExpr) else {
                self.err(loc, "empty sequence/property declaration body");
                return None;
            };
            let (clk, dis, prop) =
                self.bind_property_spec(d_tree, prop_spec, inst_id, d_prefix, d_consts, &next_substs, depth + 1)?;
            if decl_kind == ElabItemKind::SequenceDecl {
                if dis.is_some() {
                    self.err(loc, "`disable iff` is only legal at the top level of a property");
                    return None;
                }
                let PropExpr::Seq(mut s) = prop else {
                    self.err(loc, "property operator is illegal inside a sequence declaration");
                    return None;
                };
                if let Some(clock) = clk {
                    s = SeqExpr::Clocked { clock, seq: Box::new(s) };
                }
                return Some(SvaValue::Seq(s));
            }
            let mut p = prop;
            if let Some(clock) = clk {
                p = PropExpr::Clocked { clock, prop: Box::new(p) };
            }
            Some(SvaValue::Prop { prop: p, disable_iff: dis })
        })();
        self.cur_pkg = saved_pkg;
        res
    }

    #[allow(clippy::too_many_arguments)]
    fn bind_actual_arg(
        &mut self,
        tree_idx: usize,
        act_node: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
        depth: usize,
    ) -> Option<ActualSubst> {
        let t = self.tree(tree_idx);
        if t.kind(act_node) == SyntaxKind::SequenceExpr {
            let sva_val =
                self.bind_seq_or_prop(tree_idx, act_node, inst_id, scope_prefix, gen_consts, substs, depth)?;
            return Some(ActualSubst { expr_id: None, sva_val });
        }
        // Check if `act_node` is a bare or scoped identifier referencing a formal parameter or named sequence/property.
        if t.kind(act_node) == SyntaxKind::NameExpr {
            let raw = t.bare_text(act_node);
            let ident = raw.trim().strip_prefix('\\').unwrap_or(raw.trim());
            if let Some(existing) = substs.get(ident) {
                return Some(existing.clone());
            }
            if self.find_sva_decl(inst_id, scope_prefix, None, ident).is_some() {
                let sva_val =
                    self.bind_leaf_operand(tree_idx, act_node, inst_id, scope_prefix, gen_consts, substs, depth)?;
                return Some(ActualSubst { expr_id: None, sva_val });
            }
        } else if t.kind(act_node) == SyntaxKind::ScopedExpr {
            let toks: Vec<&str> = t.token_range(act_node).into_iter().map(|ti| &*t.tokens[ti as usize].text).collect();
            if let [pkg, "::", raw] = toks.as_slice() {
                let ident = raw.strip_prefix('\\').unwrap_or(raw);
                if self.find_sva_decl(inst_id, scope_prefix, Some(pkg), ident).is_some() {
                    let sva_val =
                        self.bind_leaf_operand(tree_idx, act_node, inst_id, scope_prefix, gen_consts, substs, depth)?;
                    return Some(ActualSubst { expr_id: None, sva_val });
                }
            }
        }
        // Bind untyped expression (preserving `'0`/`'1`/`'x`/`'z` as `UnbasedUnsized` for context-determined uses)
        // alongside a 1-bit boolean view for direct `SeqExpr::Bool` usage.
        let (raw_expr, cond_expr, b_diags) = {
            let mut binder = self.make_expr_binder(inst_id, scope_prefix, gen_consts, substs);
            let raw_expr = binder.bind_untyped_actual(tree_idx, act_node);
            let cond_expr = binder.bind_condition(tree_idx, act_node);
            (raw_expr, cond_expr, binder.diags)
        };
        self.diags.extend(b_diags);
        let expr_id = raw_expr?;
        let bool_id = cond_expr?;
        Some(ActualSubst { expr_id: Some(expr_id), sva_val: SvaValue::Seq(SeqExpr::Bool(bool_id)) })
    }

    #[allow(clippy::too_many_arguments)]
    fn bind_property_case(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
        depth: usize,
    ) -> Option<SvaValue> {
        let t = self.tree(tree_idx);
        let mut items: Vec<(Vec<NodeId>, NodeId)> = Vec::new();
        let mut default_prop: Option<NodeId> = None;
        let mut sel_node: Option<NodeId> = None;
        let mut cur_pats: Vec<NodeId> = Vec::new();
        let mut in_default = false;
        let mut after_colon = false;

        for ch in &t.node(node).children {
            match *ch {
                Child::Token(ti) => {
                    let tok = &t.tokens[ti as usize];
                    if tok.kind.is_trivia() {
                        continue;
                    }
                    match &*tok.text {
                        "default" => in_default = true,
                        ":" => after_colon = true,
                        ";" => {
                            cur_pats.clear();
                            in_default = false;
                            after_colon = false;
                        }
                        _ => {}
                    }
                }
                Child::Node(c) => {
                    if t.kind(c) == SyntaxKind::Attribute {
                        continue;
                    }
                    if sel_node.is_none() {
                        sel_node = Some(c);
                    } else if t.kind(c) == SyntaxKind::SequenceExpr && (after_colon || in_default) {
                        if in_default {
                            default_prop = Some(c);
                        } else {
                            items.push((std::mem::take(&mut cur_pats), c));
                        }
                    } else {
                        cur_pats.push(c);
                    }
                }
            }
        }
        let sel = sel_node?;
        let mut acc: Option<PropExpr> = match default_prop {
            Some(dp) => {
                let dloc = self.loc_of(tree_idx, dp, inst_id, scope_prefix, gen_consts);
                let dval = self.bind_seq_or_prop(tree_idx, dp, inst_id, scope_prefix, gen_consts, substs, depth)?;
                Some(self.unwrap_sub_prop(dval, dloc)?)
            }
            None => None,
        };
        for (pats, prop_node) in items.into_iter().rev() {
            let ploc = self.loc_of(tree_idx, prop_node, inst_id, scope_prefix, gen_consts);
            let bval = self.bind_seq_or_prop(tree_idx, prop_node, inst_id, scope_prefix, gen_consts, substs, depth)?;
            let branch_prop = self.unwrap_sub_prop(bval, ploc)?;
            let mut cond_acc: Option<ExprId> = None;
            for pat in pats {
                let eq = self.bind_case_eq(tree_idx, sel, pat, inst_id, scope_prefix, gen_consts, substs)?;
                cond_acc = Some(match cond_acc {
                    None => eq,
                    Some(prev) => {
                        let loc = self.arena.get(eq).loc;
                        self.arena.alloc(
                            crate::expr::ExprKind::Binary { op: crate::expr::BinaryOp::LogOr, lhs: prev, rhs: eq },
                            crate::elab_ir::RtlType::Integral { width: 1, signed: false, four_state: true },
                            None,
                            loc,
                        )
                    }
                });
            }
            if let Some(cond) = cond_acc {
                acc = Some(PropExpr::IfElse { cond, then_prop: Box::new(branch_prop), else_prop: acc.map(Box::new) });
            }
        }
        acc.map(|prop| SvaValue::Prop { prop, disable_iff: None })
    }

    #[allow(clippy::too_many_arguments)]
    fn bind_case_eq(
        &mut self,
        tree_idx: usize,
        sel: NodeId,
        pat: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
    ) -> Option<ExprId> {
        let (s, p, b_diags) = {
            let mut binder = self.make_expr_binder(inst_id, scope_prefix, gen_consts, substs);
            let s = binder.bind_self(tree_idx, sel);
            let p = binder.bind_self(tree_idx, pat);
            (s, p, binder.diags)
        };
        self.diags.extend(b_diags);
        let (s, p) = (s?, p?);
        let loc = self.arena.get(p).loc;
        let (sw, ss, s4) = self.arena.get(s).ty.integral_shape()?;
        let (pw, ps, p4) = self.arena.get(p).ty.integral_shape()?;
        let cw = sw.max(pw);
        let cs = ss && ps;
        let c4 = s4 || p4;
        let lhs = self.arena.coerce(s, cw, cs, c4, loc);
        let rhs = self.arena.coerce(p, cw, cs, c4, loc);
        Some(self.arena.alloc(
            crate::expr::ExprKind::Binary { op: crate::expr::BinaryOp::Eq, lhs, rhs },
            crate::elab_ir::RtlType::Integral { width: 1, signed: false, four_state: c4 },
            None,
            loc,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn bind_clock_timing(
        &mut self,
        tree_idx: usize,
        tc: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
        clk_depth: usize,
    ) -> Option<SvaClock> {
        let loc = self.loc_of(tree_idx, tc, inst_id, scope_prefix, gen_consts);
        if clk_depth >= MAX_SVA_EXPAND_DEPTH {
            self.err(loc, "cyclic or overly deep clocking block reference exceeds maximum depth (32)");
            return None;
        }
        let t = self.tree(tree_idx);
        let events: Vec<NodeId> = t.children(tc).filter(|&c| t.kind(c) == SyntaxKind::EventExpr).collect();
        if events.len() > 1 {
            self.unsup(loc, "multi-event clocking `@(e1 or e2)` inside SVA property is not supported");
            return None;
        }
        if let Some(&ev) = events.first() {
            // Unwrap nested parenthesized `EventExpr` if present.
            let inner_ev = t.child(ev, SyntaxKind::EventExpr).unwrap_or(ev);
            let own = Self::own_toks(t, inner_ev);
            let edge = if own.contains(&"posedge") {
                SvaEdge::Posedge
            } else if own.contains(&"negedge") {
                SvaEdge::Negedge
            } else {
                SvaEdge::AnyEdge
            };
            let kids = Self::expr_kids(t, inner_ev);
            let sig_node = *kids.first()?;
            if edge == SvaEdge::AnyEdge && t.kind(sig_node) == SyntaxKind::NameExpr {
                let cb_name = t.bare_text(sig_node).trim().to_string();
                let saved_diags = self.diags.len();
                if let Some(cb_clk) =
                    self.resolve_named_clocking(inst_id, scope_prefix, gen_consts, &cb_name, clk_depth + 1)
                {
                    return Some(cb_clk);
                }
                if self.diags.len() > saved_diags {
                    return None;
                }
            }
            let signal = self.bind_self_expr(tree_idx, sig_node, inst_id, scope_prefix, gen_consts, substs)?;
            let iff = if let Some(&iff_node) = kids.get(1) {
                Some(self.bind_bool_expr(tree_idx, iff_node, inst_id, scope_prefix, gen_consts, substs)?)
            } else {
                None
            };
            return Some(SvaClock { edge, signal, iff });
        }
        // Bare `@cb` or `@clk`: `TimingControl` has a `NameExpr` child.
        if let Some(ne) = t.child(tc, SyntaxKind::NameExpr) {
            let name = t.bare_text(ne).trim().to_string();
            let saved_diags = self.diags.len();
            if let Some(cb_clk) = self.resolve_named_clocking(inst_id, scope_prefix, gen_consts, &name, clk_depth + 1) {
                return Some(cb_clk);
            }
            if self.diags.len() > saved_diags {
                return None;
            }
            let signal = self.bind_self_expr(tree_idx, ne, inst_id, scope_prefix, gen_consts, substs)?;
            return Some(SvaClock { edge: SvaEdge::AnyEdge, signal, iff: None });
        }
        self.err(loc, "invalid SVA clocking event");
        None
    }

    fn eval_delay_timing(
        &mut self,
        tree_idx: usize,
        tc: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
    ) -> Option<CycleRange> {
        let loc = self.loc_of(tree_idx, tc, inst_id, scope_prefix, gen_consts);
        let t = self.tree(tree_idx);
        let own = Self::own_toks(t, tc);
        if own.contains(&"*") {
            return Some(CycleRange::unbounded(0));
        }
        if own.contains(&"+") {
            return Some(CycleRange::unbounded(1));
        }
        let has_colon = own.contains(&":");
        let has_dollar = own.contains(&"$");
        let kids = Self::expr_kids(t, tc);
        let Some(&k0) = kids.first() else {
            self.err(loc, "missing cycle delay value after '##'");
            return None;
        };
        let min = self.eval_delay_bound_node(tree_idx, k0, inst_id, scope_prefix, gen_consts, substs)?;
        if has_dollar {
            return Some(CycleRange::unbounded(min));
        }
        if has_colon {
            let Some(&k1) = kids.get(1) else {
                self.err(loc, "missing upper cycle bound in '##[min:max]'");
                return None;
            };
            let max = self.eval_delay_bound_node(tree_idx, k1, inst_id, scope_prefix, gen_consts, substs)?;
            if max < min {
                self.err(loc, format!("SVA cycle delay range [{min}:{max}] has max < min"));
                return None;
            }
            return Some(CycleRange { min, max: CycleBound::Const(max) });
        }
        Some(CycleRange::exact(min))
    }

    fn eval_delay_bound_node(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
    ) -> Option<u32> {
        let t = self.tree(tree_idx);
        // `delay_value` in `sv-syntax` wraps `##PARAM` as `Literal` containing a `NameExpr` child.
        let target = if t.kind(node) == SyntaxKind::Literal {
            t.child(node, SyntaxKind::NameExpr).unwrap_or(node)
        } else {
            node
        };
        let loc = self.loc_of(tree_idx, target, inst_id, scope_prefix, gen_consts);
        let e = self.bind_self_expr(tree_idx, target, inst_id, scope_prefix, gen_consts, substs)?;
        let Some(cv) = self.arena.get(e).cval.clone() else {
            self.err(loc, "SVA cycle count must be an elaboration-time constant");
            return None;
        };
        let Some(count) = crate::expr::cval_nonneg_u32(&cv, crate::types::MAX_SVA_CYCLES) else {
            self.err(
                loc,
                format!("SVA cycle count must be a non-negative integer within 0..={}", crate::types::MAX_SVA_CYCLES),
            );
            return None;
        };
        Some(count)
    }

    fn eval_repetition(
        &mut self,
        tree_idx: usize,
        rep: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
    ) -> Option<(RepeatKind, CycleRange)> {
        let t = self.tree(tree_idx);
        let own = Self::own_toks(t, rep);
        if own.contains(&"+") {
            return Some((RepeatKind::Consecutive, CycleRange::unbounded(1)));
        }
        let kind = if own.contains(&"->") {
            RepeatKind::Goto
        } else if own.contains(&"=") {
            RepeatKind::NonConsecutive
        } else {
            RepeatKind::Consecutive
        };
        let kids = Self::expr_kids(t, rep);
        if kids.is_empty() {
            return Some((kind, CycleRange::unbounded(0)));
        }
        let range = self.eval_cycle_range(tree_idx, rep, inst_id, scope_prefix, gen_consts, substs)?;
        Some((kind, range))
    }

    fn eval_cycle_range(
        &mut self,
        tree_idx: usize,
        rep: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
    ) -> Option<CycleRange> {
        let loc = self.loc_of(tree_idx, rep, inst_id, scope_prefix, gen_consts);
        let t = self.tree(tree_idx);
        let own = Self::own_toks(t, rep);
        let kids = Self::expr_kids(t, rep);
        let min = self.eval_delay_bound_node(tree_idx, *kids.first()?, inst_id, scope_prefix, gen_consts, substs)?;
        if own.contains(&"$") {
            return Some(CycleRange::unbounded(min));
        }
        if own.contains(&":") {
            let max = self.eval_delay_bound_node(tree_idx, *kids.get(1)?, inst_id, scope_prefix, gen_consts, substs)?;
            if max < min {
                self.err(loc, format!("SVA repetition range [{min}:{max}] has max < min"));
                return None;
            }
            return Some(CycleRange { min, max: CycleBound::Const(max) });
        }
        Some(CycleRange::exact(min))
    }

    fn resolve_default_clocking(
        &mut self,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        clk_depth: usize,
    ) -> Option<SvaClock> {
        if clk_depth >= MAX_SVA_EXPAND_DEPTH {
            let loc = self.design.instance(inst_id).loc;
            self.err(loc, "cyclic or overly deep clocking block reference exceeds maximum depth (32)");
            return None;
        }
        let items = self.design.instance(inst_id).items.clone();
        let mut prefix = scope_prefix;
        loop {
            for item in &items {
                if item.kind == ElabItemKind::ClockingDecl && item.scope_prefix == prefix {
                    let t = self.tree(item.tree);
                    let own = Self::own_toks(t, item.node);
                    if own.contains(&"default") {
                        if let Some(tc) = t.child(item.node, SyntaxKind::TimingControl) {
                            return self.bind_clock_timing(
                                item.tree,
                                tc,
                                inst_id,
                                &item.scope_prefix,
                                &item.gen_consts,
                                &FxMap::default(),
                                clk_depth + 1,
                            );
                        }
                        if let Some(nm) = Self::decl_name(t, item.node) {
                            return self.resolve_named_clocking(inst_id, prefix, gen_consts, &nm, clk_depth + 1);
                        }
                    }
                }
            }
            if prefix.is_empty() {
                break;
            }
            prefix = match prefix.rfind('.') {
                Some(i) => &prefix[..i],
                None => "",
            };
        }
        None
    }

    fn resolve_named_clocking(
        &mut self,
        inst_id: InstanceId,
        scope_prefix: &str,
        _gen_consts: &[(String, CVal)],
        cb_name: &str,
        clk_depth: usize,
    ) -> Option<SvaClock> {
        let items = self.design.instance(inst_id).items.clone();
        let mut prefix = scope_prefix;
        loop {
            for item in &items {
                if item.kind == ElabItemKind::ClockingDecl && item.scope_prefix == prefix {
                    let t = self.tree(item.tree);
                    if Self::decl_name(t, item.node).as_deref() == Some(cb_name) {
                        if clk_depth >= MAX_SVA_EXPAND_DEPTH {
                            self.err(
                                item.loc,
                                "cyclic or overly deep clocking block reference exceeds maximum depth (32)",
                            );
                            return None;
                        }
                        if let Some(tc) = t.child(item.node, SyntaxKind::TimingControl) {
                            return self.bind_clock_timing(
                                item.tree,
                                tc,
                                inst_id,
                                &item.scope_prefix,
                                &item.gen_consts,
                                &FxMap::default(),
                                clk_depth + 1,
                            );
                        }
                        self.err(item.loc, format!("clocking block '{cb_name}' has no clocking event"));
                        return None;
                    }
                }
            }
            if prefix.is_empty() {
                break;
            }
            prefix = match prefix.rfind('.') {
                Some(i) => &prefix[..i],
                None => "",
            };
        }
        None
    }

    fn resolve_default_disable(
        &mut self,
        inst_id: InstanceId,
        scope_prefix: &str,
        _gen_consts: &[(String, CVal)],
    ) -> Option<ExprId> {
        let items = self.design.instance(inst_id).items.clone();
        let mut prefix = scope_prefix;
        loop {
            for item in &items {
                if item.kind == ElabItemKind::DefaultDisable && item.scope_prefix == prefix {
                    let t = self.tree(item.tree);
                    if let Some(expr_node) = Self::expr_kids(t, item.node).into_iter().next() {
                        return self.bind_bool_expr(
                            item.tree,
                            expr_node,
                            inst_id,
                            &item.scope_prefix,
                            &item.gen_consts,
                            &FxMap::default(),
                        );
                    }
                }
            }
            if prefix.is_empty() {
                break;
            }
            prefix = match prefix.rfind('.') {
                Some(i) => &prefix[..i],
                None => "",
            };
        }
        None
    }

    fn extract_procedural_clock(
        &mut self,
        tree_idx: usize,
        proc_node: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
    ) -> Option<SvaClock> {
        let t = self.tree(tree_idx);
        let own = Self::own_toks(t, proc_node);
        if !own.iter().any(|k| matches!(*k, "always_ff" | "always")) {
            return None;
        }
        let timed = t.child(proc_node, SyntaxKind::TimedStmt)?;
        let tc = t.child(timed, SyntaxKind::TimingControl)?;
        let events: Vec<NodeId> = t.children(tc).filter(|&c| t.kind(c) == SyntaxKind::EventExpr).collect();
        if events.is_empty() {
            return None;
        }
        // In `always_ff @(posedge clk or negedge rst_n)`, the first event is the primary clock unless
        // an `if (!rst_n)` async reset branch names one of the events — for SVA clock inference (§16.14.6),
        // pick the first event when single-event, or the event not tested as an async reset condition!
        let chosen_ev = if events.len() == 1 {
            events[0]
        } else {
            let body_stmt = Self::expr_kids(t, timed).into_iter().nth(1);
            let rst_name = body_stmt.and_then(|s| extract_if_reset_ident(t, s));
            events
                .iter()
                .copied()
                .find(|&ev| {
                    let inner = t.child(ev, SyntaxKind::EventExpr).unwrap_or(ev);
                    let sig_txt = Self::expr_kids(t, inner)
                        .first()
                        .map(|&n| t.bare_text(n).trim().to_string())
                        .unwrap_or_default();
                    rst_name.as_deref() != Some(sig_txt.as_str())
                })
                .unwrap_or(events[0])
        };
        let inner = t.child(chosen_ev, SyntaxKind::EventExpr).unwrap_or(chosen_ev);
        let ev_own = Self::own_toks(t, inner);
        let edge = if ev_own.contains(&"posedge") {
            SvaEdge::Posedge
        } else if ev_own.contains(&"negedge") {
            SvaEdge::Negedge
        } else {
            SvaEdge::AnyEdge
        };
        let sig_node = *Self::expr_kids(t, inner).first()?;
        let saved_diags = self.diags.len();
        let signal = self.bind_self_expr(tree_idx, sig_node, inst_id, scope_prefix, gen_consts, &FxMap::default());
        if signal.is_none() {
            // Leave procedural clock diagnostics to the procedural binder so we don't duplicate them.
            self.diags.truncate(saved_diags);
            return None;
        }
        Some(SvaClock { edge, signal: signal?, iff: None })
    }

    fn collect_procedural_concurrent_asserts(t: &Tree, node: NodeId, out: &mut Vec<NodeId>) {
        for c in t.children(node) {
            if t.kind(c) == SyntaxKind::ConcurrentAssert {
                out.push(c);
            } else {
                Self::collect_procedural_concurrent_asserts(t, c, out);
            }
        }
    }

    fn make_expr_binder<'b>(
        &'b mut self,
        inst_id: InstanceId,
        scope_prefix: &'b str,
        gen_consts: &'b [(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
    ) -> ExprBinder<'b> {
        let mut binder = ExprBinder::new(self.trees, self.design, inst_id, scope_prefix, gen_consts, self.arena);
        binder.extra_tree = self.extra_tree;
        if let Some(pkg_name) = &self.cur_pkg {
            let p_opt = if pkg_name == "$unit" {
                Some(&self.design.unit_scope)
            } else {
                self.design.packages.get(pkg_name.as_str())
            };
            if let Some(p) = p_opt {
                for (tnm, tty) in &p.typedefs {
                    binder.local_typedefs.insert(tnm.clone(), tty.clone());
                }
            }
        }
        for (k, v) in substs {
            if let Some(eid) = v.expr_id {
                binder.expr_substs.insert(k.clone(), eid);
            }
        }
        binder
    }

    fn bind_bool_expr(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
    ) -> Option<ExprId> {
        let (res, b_diags) = {
            let mut binder = self.make_expr_binder(inst_id, scope_prefix, gen_consts, substs);
            let res = binder.bind_condition(tree_idx, node);
            (res, binder.diags)
        };
        self.diags.extend(b_diags);
        res
    }

    fn bind_self_expr(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        substs: &FxMap<String, ActualSubst>,
    ) -> Option<ExprId> {
        let (res, b_diags) = {
            let mut binder = self.make_expr_binder(inst_id, scope_prefix, gen_consts, substs);
            let res = binder.bind_self(tree_idx, node);
            (res, binder.diags)
        };
        self.diags.extend(b_diags);
        res
    }

    fn loc_of(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        inst_id: InstanceId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
    ) -> Loc {
        let mut binder = ExprBinder::new(self.trees, self.design, inst_id, scope_prefix, gen_consts, self.arena);
        binder.extra_tree = self.extra_tree;
        binder.loc_of(tree_idx, node)
    }

    fn loc_of_tok(
        &mut self,
        tree_idx: usize,
        tok_idx: u32,
        _inst_id: InstanceId,
        _scope_prefix: &str,
        _gen_consts: &[(String, CVal)],
    ) -> Loc {
        let mut locator = if self.design.files.is_empty() {
            crate::Locator::default()
        } else {
            crate::Locator::new(self.design.files.clone())
        };
        let raw = locator.loc(tree_idx, self.tree(tree_idx), tok_idx);
        let path = locator.files().get(raw.file as usize).cloned().unwrap_or_default();
        let file_id = self.design.files.iter().position(|f| f == &path).map_or(raw.file, |i| i as u32);
        Loc { file: file_id, line: raw.line, column: raw.column, expansion: raw.expansion }
    }

    fn err(&mut self, loc: Loc, msg: impl Into<String>) {
        self.diags.push(ExprDiag { loc, message: msg.into(), unsupported: false });
    }

    fn unsup(&mut self, loc: Loc, msg: impl Into<String>) {
        self.diags.push(ExprDiag { loc, message: msg.into(), unsupported: true });
    }

    fn expr_kids(t: &Tree, node: NodeId) -> Vec<NodeId> {
        t.children(node).filter(|&c| t.kind(c) != SyntaxKind::Attribute).collect()
    }

    fn own_toks(t: &Tree, node: NodeId) -> Vec<&str> {
        t.node(node)
            .children
            .iter()
            .filter_map(|c| match *c {
                Child::Token(i) if !t.tokens[i as usize].kind.is_trivia() => Some(&*t.tokens[i as usize].text),
                _ => None,
            })
            .collect()
    }

    fn decl_name(t: &Tree, decl: NodeId) -> Option<String> {
        let n = t.child(decl, SyntaxKind::Name)?;
        let last = *t.token_range(n).last()?;
        let raw = &*t.tokens[last as usize].text;
        Some(raw.strip_prefix('\\').unwrap_or(raw).to_string())
    }
}

fn is_sva_binary_op(tok: &str) -> bool {
    matches!(
        tok,
        "and"
            | "or"
            | "intersect"
            | "throughout"
            | "within"
            | "iff"
            | "until"
            | "s_until"
            | "until_with"
            | "s_until_with"
            | "implies"
            | "|->"
            | "|=>"
            | "#-#"
            | "#=#"
    )
}

/// Returns `(left_bp, right_bp)` per IEEE 1800-2023 Table 16-1 operator precedence.
fn sva_infix_bp(op: &str) -> (u8, u8) {
    match op {
        "throughout" => (71, 70),
        "within" => (65, 66),
        "intersect" => (60, 61),
        "and" => (50, 51),
        "or" => (40, 41),
        "until" | "s_until" | "until_with" | "s_until_with" => (26, 25),
        "|->" | "|=>" | "#-#" | "#=#" => (21, 20),
        "implies" => (16, 15),
        "iff" => (12, 11),
        _ => (1, 2),
    }
}

fn extract_sva_formals(t: &Tree, decl: NodeId) -> Vec<(String, Option<NodeId>)> {
    let mut out = Vec::new();
    let Some(pl) = t.child(decl, SyntaxKind::PortList) else { return out };
    for p in t.children(pl).filter(|&c| t.kind(c) == SyntaxKind::Port) {
        for d in t.children(p).filter(|&c| t.kind(c) == SyntaxKind::Declarator) {
            let Some(nm_node) = t.child(d, SyntaxKind::Name) else { continue };
            let Some(&last) = t.token_range(nm_node).last() else { continue };
            let raw = &*t.tokens[last as usize].text;
            let name = raw.strip_prefix('\\').unwrap_or(raw).to_string();
            let def = t
                .children(d)
                .find(|&c| !matches!(t.kind(c), SyntaxKind::Name | SyntaxKind::Dim | SyntaxKind::Attribute));
            out.push((name, def));
        }
    }
    out
}

fn extract_if_reset_ident(t: &Tree, stmt: NodeId) -> Option<String> {
    let target = if t.kind(stmt) == SyntaxKind::SeqBlock {
        t.children(stmt).find(|&c| t.kind(c) == SyntaxKind::IfStmt)?
    } else if t.kind(stmt) == SyntaxKind::IfStmt {
        stmt
    } else {
        return None;
    };
    let cond = t.children(target).find(|&c| t.kind(c) != SyntaxKind::Attribute)?;
    let raw = t.bare_text(cond);
    let trimmed = raw
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim()
        .trim_start_matches('!')
        .trim_start_matches('~')
        .trim();
    let ident: String = trimmed.chars().take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_').collect();
    (!ident.is_empty()).then_some(ident)
}

/// Resolves an `instance_path` (`"top.u0"` or `"top.u0.g[0]"`) into `(InstanceId, scope_prefix)`.
pub fn resolve_scope_path(design: &ElaboratedDesign, instance_path: &str) -> Option<(InstanceId, String)> {
    let trimmed = instance_path.trim();
    if let Some(id) = design.instance_by_path(trimmed) {
        return Some((id, String::new()));
    }
    // Check if `trimmed` points to a generate scope inside an elaborated instance (`"top.u0.g[0]"`).
    let mut cur = trimmed;
    while let Some((parent_path, tail)) = cur.rsplit_once('.') {
        if let Some(id) = design.instance_by_path(parent_path) {
            let suffix = &trimmed[(parent_path.len() + 1)..];
            let inst = design.instance(id);
            let exists = inst.symbols.iter().any(|&s| {
                let rp = &design.symbol(s).rel_path;
                rp == suffix || rp.starts_with(&format!("{suffix}."))
            }) || inst
                .items
                .iter()
                .any(|it| it.scope_prefix == suffix || it.scope_prefix.starts_with(&format!("{suffix}.")));
            if exists {
                return Some((id, suffix.to_string()));
            }
        }
        cur = parent_path;
        let _ = tail;
    }
    None
}
