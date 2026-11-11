//! Bound, typed expression IR and IEEE 1800-2023 §11.6 / §11.8 binder.
//!
//! [`ExprBinder`] resolves every identifier fail-closed against an
//! [`ElaboratedDesign`] (local variables, generate constants, per-instance
//! symbols, interface ports/instances, enum constants, package imports, `$unit`,
//! and hierarchical paths), computes self-determined and context-determined
//! [`RtlType`]s, inserts explicit [`ExprKind::Resize`], [`ExprKind::CastSign`],
//! and [`ExprKind::CastFourState`] conversion nodes, and folds 4-state
//! constant subexpressions via [`CVal`] without collapsing `x`/`z` bits.

use sv_syntax::{NodeId, SyntaxKind, Tree};

use crate::elab_ir::{ElabItemKind, ElaboratedDesign, InstanceId, PackedField, PortDir, RtlType, SymbolId, SymbolKind};
use crate::expr_types::{
    assigned_shape, binary_shape, is_unbased_unsized_tok, literal_type, ARITH, ASSIGN, COMPARE, LOGICAL, SHIFT,
};
use crate::fx::FxMap;
use crate::model::Loc;
use crate::types::{range_len_u32, BitState, CVal, MAX_ARRAY_ELEMS, MAX_BIT_WIDTH, MAX_SVA_CYCLES};
use crate::Locator;

/// Dense arena index for a [`BoundExpr`] inside an [`ExprArena`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExprId(pub u32);

/// Identifier for a procedural local variable (loop induction variable or inlined subroutine local/arg).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalId(pub u32);

/// Unary and reduction operators in [`ExprKind::Unary`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// Unary `+`
    Plus,
    /// Two's-complement negation `-`
    Neg,
    /// Bitwise negation `~`
    BitNot,
    /// Logical negation `!`
    LogNot,
    /// Reduction `&`
    RedAnd,
    /// Reduction `~&`
    RedNand,
    /// Reduction `|`
    RedOr,
    /// Reduction `~|`
    RedNor,
    /// Reduction `^`
    RedXor,
    /// Reduction `~^` / `^~`
    RedXnor,
}

/// Binary operators in [`ExprKind::Binary`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Mod,
    /// `**`
    Pow,
    /// `&`
    BitAnd,
    /// `|`
    BitOr,
    /// `^`
    BitXor,
    /// `~^` / `^~`
    BitXnor,
    /// `&&`
    LogAnd,
    /// `||`
    LogOr,
    /// `->`
    LogImp,
    /// `<->`
    LogEqv,
    /// `<<` / `<<<`
    Shl,
    /// Logical right shift `>>`
    LShr,
    /// Arithmetic right shift `>>>` (sign-shifts when LHS is signed, logical when unsigned)
    AShr,
    /// `==`
    Eq,
    /// `!=`
    Neq,
    /// `===`
    CaseEq,
    /// `!==`
    CaseNeq,
    /// `==?`
    WildEq,
    /// `!=?`
    WildNeq,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
}

/// Set-membership element in `expr inside { ... }`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsideItem {
    /// Single value or wildcard pattern (`a ==? item`).
    Single(ExprId),
    /// Closed value range `[lo : hi]` (`lo <= expr && expr <= hi`).
    Range(ExprId, ExprId),
}

/// Built-in synthesizable / query system functions in [`ExprKind::SysCall`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SysFunc {
    /// `$clog2(x)`
    Clog2,
    /// `$bits(x)`
    Bits,
    /// `$countones(x)`
    CountOnes,
    /// `$onehot(x)`
    OneHot,
    /// `$onehot0(x)`
    OneHot0,
    /// `$isunknown(x)`
    IsUnknown,
    /// `$signed(x)`
    Signed,
    /// `$unsigned(x)`
    Unsigned,
    /// `$size(x [, dim])`
    Size,
    /// `$high(x [, dim])`
    High,
    /// `$low(x [, dim])`
    Low,
    /// `$left(x [, dim])`
    Left,
    /// `$right(x [, dim])`
    Right,
    /// `$increment(x [, dim])`
    Increment,
}

/// Sampled-value functions preserved for formal consumers (`$past`, `$rose`, `$fell`, `$stable`, `$changed`, `$sampled`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SampledKind {
    /// `$sampled(e)`
    Sampled,
    /// `$past(e [, n [, en [, clk]]])`
    Past,
    /// `$rose(e [, clk])`
    Rose,
    /// `$fell(e [, clk])`
    Fell,
    /// `$stable(e [, clk])`
    Stable,
    /// `$changed(e [, clk])`
    Changed,
}

/// Variant payload of a [`BoundExpr`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    /// Reference to an elaborated per-instance [`SymbolId`].
    Symbol(SymbolId),
    /// Reference to a procedural local variable (`for` loop variable or subroutine local/formal).
    Local(LocalId),
    /// 4-state constant literal or folded constant value (`x`/`z` bits preserved).
    Const(CVal),
    /// Unbased unsized fill literal (`'0`, `'1`, `'x`, `'z`) prior to context sizing.
    UnbasedUnsized(BitState),
    /// Unary or reduction operation.
    Unary {
        /// Operator.
        op: UnaryOp,
        /// Operand.
        arg: ExprId,
    },
    /// Binary operation (with operands already explicitly converted to operator context).
    Binary {
        /// Operator.
        op: BinaryOp,
        /// Left operand.
        lhs: ExprId,
        /// Right operand.
        rhs: ExprId,
    },
    /// Conditional mux `cond ? then_val : else_val`.
    Cond {
        /// Condition expression (1-bit or non-zero test).
        cond: ExprId,
        /// True branch (already converted to result type).
        then_val: ExprId,
        /// False branch (already converted to result type).
        else_val: ExprId,
    },
    /// Set membership `expr inside { items }`.
    Inside {
        /// Tested expression.
        expr: ExprId,
        /// Membership items/ranges.
        items: Vec<InsideItem>,
    },
    /// Bit concatenation `{a, b, ...}` (MSB first).
    Concat(Vec<ExprId>),
    /// Replication `{count{expr}}`.
    Replicate {
        /// Non-negative constant repeat count.
        count: u32,
        /// Inner concatenated value.
        expr: ExprId,
    },
    /// Packed or unpacked `struct`/`union` member access `base.field`.
    Member {
        /// Aggregate base expression.
        base: ExprId,
        /// Field name.
        field: String,
        /// Bit LSB offset and width when `base` is a packed `struct` or `union`.
        packed_slice: Option<(u32, u32)>,
    },
    /// Single bit-select or packed-array element select `base[index]`.
    BitSelect {
        /// Packed vector or `PackedArray` base.
        base: ExprId,
        /// Bit or element index expression.
        index: ExprId,
        /// Declared `(left, right)` bounds of `base` (`(w-1, 0)` by default, `(0, w-1)` for ascending).
        bounds: (i128, i128),
        /// Bit width of one selected element (`1` for bit-select, `elem.bit_width()` for `PackedArray`).
        elem_width: u32,
    },
    /// Constant part-select `base[msb:lsb]`.
    PartSelect {
        /// Packed base expression.
        base: ExprId,
        /// Left constant bound in source.
        msb: i128,
        /// Right constant bound in source.
        lsb: i128,
        /// Declared `(left, right)` bounds of `base`.
        bounds: (i128, i128),
        /// Element bit width (`1` for plain vectors).
        elem_width: u32,
    },
    /// Variable indexed part-select `base[base_idx +: width]` (`ascending = true`) or `base[base_idx -: width]`.
    IndexedPartSelect {
        /// Packed base expression.
        base: ExprId,
        /// Dynamic start index expression.
        base_idx: ExprId,
        /// Constant positive slice count.
        width: u32,
        /// `true` for `+:`, `false` for `-:`.
        ascending: bool,
        /// Declared `(left, right)` bounds of `base`.
        bounds: (i128, i128),
        /// Element bit width (`1` for plain vectors).
        elem_width: u32,
    },
    /// Unpacked array element index `mem[index]`.
    ArrayIndex {
        /// Unpacked array base expression.
        base: ExprId,
        /// Element index expression.
        index: ExprId,
        /// Declared `(left, right)` bounds of the indexed unpacked dimension.
        dim: (i128, i128),
    },
    /// Explicit width conversion (zero-extend when `!signed`, sign-extend when `signed`, or truncate when `width < arg.width`).
    Resize {
        /// Source expression.
        arg: ExprId,
        /// Target bit width.
        width: u32,
        /// `true` for sign-extension when widening; `false` for zero-extension.
        signed: bool,
    },
    /// Explicit signedness conversion (`$signed`, `$unsigned`, `signed'(x)`, or IEEE 11.8 context conversion).
    CastSign {
        /// Source expression.
        arg: ExprId,
        /// Target signedness.
        signed: bool,
    },
    /// Explicit 2-state / 4-state conversion (`bit'(x)` collapses `x`/`z` to `0`; `logic'(b)` widens to 4-state).
    CastFourState {
        /// Source expression.
        arg: ExprId,
        /// Target 4-state flag.
        four_state: bool,
    },
    /// Built-in system function call (`$countones`, `$onehot`, `$onehot0`, `$isunknown`, ...).
    SysCall {
        /// System function kind.
        func: SysFunc,
        /// Bound argument expressions.
        args: Vec<ExprId>,
    },
    /// Sampled-value function (`$past`, `$rose`, `$fell`, `$stable`, `$changed`, `$sampled`).
    Sampled {
        /// Sampled function kind.
        kind: SampledKind,
        /// Monitored value expression.
        expr: ExprId,
        /// Cycle lookback depth (`>= 1` for `$past`, `1` for edge functions, `0` for `$sampled`).
        depth: u32,
        /// Optional clock-gating enable expression on `$past(e, n, gate)`.
        gate: Option<ExprId>,
        /// Optional explicit clock signal expression.
        clock: Option<ExprId>,
        /// Explicit clock edge polarity (`Some(true)` for `posedge`, `Some(false)` for `negedge`).
        clock_posedge: Option<bool>,
    },
    /// Call to a user-defined `function` (in module, generate block, package, or `$unit`).
    FuncCall {
        /// Callee CST `(tree, node)`.
        callee: (usize, NodeId),
        /// Owning elaborated instance (`None` for package or `$unit` functions).
        callee_inst: Option<InstanceId>,
        /// Generate scope prefix of the function declaration.
        callee_scope_prefix: String,
        /// Function name.
        name: String,
        /// Bound argument expressions in formal order.
        args: Vec<ExprId>,
    },
    /// Struct or array assignment pattern `'{...}`, resolved to `(optional_key, expr)` pairs.
    AssignPattern(Vec<(Option<String>, ExprId)>),
}

/// One arena-allocated bound, typed expression node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundExpr {
    /// Arena ID (`arena.exprs[id.0]`).
    pub id: ExprId,
    /// Expression kind.
    pub kind: ExprKind,
    /// Resolved expression type (`Integral`, `PackedStruct`, `UnpackedArray`, ...).
    pub ty: RtlType,
    /// Folded 4-state constant value when constant (`x`/`z` preserved).
    pub cval: Option<CVal>,
    /// Source location (`1`-based line/column).
    pub loc: Loc,
}

/// Diagnostic emitted by [`ExprBinder`] on fail-closed binding or typing errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExprDiag {
    /// Source location.
    pub loc: Loc,
    /// Human-readable error message.
    pub message: String,
    /// `true` when the construct is valid IEEE 1800 SV but outside synthesizable/formal RTL.
    pub unsupported: bool,
}

/// Procedural local variable binding supplied to [`ExprBinder`] (loop variable or subroutine local).
#[derive(Clone, Debug)]
pub struct LocalBinding {
    /// Procedural local ID.
    pub id: LocalId,
    /// Declared type.
    pub ty: RtlType,
    /// Optional explicit 1-D packed bounds `(left, right)`.
    pub packed_bounds: Option<(i128, i128)>,
    /// Optional known constant value (such as during static loop unrolling).
    pub cval: Option<CVal>,
}

/// Arena storing [`BoundExpr`] nodes for an elaborated design or lowering pass.
#[derive(Clone, Debug, Default)]
pub struct ExprArena {
    /// Dense table of bound expressions (`ExprId(i)` -> `exprs[i]`).
    pub exprs: Vec<BoundExpr>,
}

impl ExprArena {
    /// Creates an empty [`ExprArena`].
    pub fn new() -> Self {
        Self { exprs: Vec::new() }
    }

    /// Borrows a [`BoundExpr`] by [`ExprId`].
    ///
    /// # Panics
    ///
    /// Panics if `id` is out of bounds for `self.exprs`.
    pub fn get(&self, id: ExprId) -> &BoundExpr {
        &self.exprs[id.0 as usize]
    }

    /// Allocates a [`BoundExpr`] in the arena and returns its [`ExprId`].
    pub fn alloc(&mut self, kind: ExprKind, ty: RtlType, cval: Option<CVal>, loc: Loc) -> ExprId {
        let id = ExprId(self.exprs.len() as u32);
        self.exprs.push(BoundExpr { id, kind, ty, cval, loc });
        id
    }

    /// Allocates a 4-state constant [`BoundExpr`].
    pub fn alloc_const(&mut self, cval: CVal, four_state: bool, loc: Loc) -> ExprId {
        let cv = if four_state { cval } else { cval.two_state() };
        let ty = RtlType::Integral { width: cv.w.max(1), signed: cv.s, four_state };
        self.alloc(ExprKind::Const(cv.clone()), ty, Some(cv), loc)
    }

    /// Wraps `expr` in explicit [`ExprKind::CastSign`], [`ExprKind::Resize`], and
    /// [`ExprKind::CastFourState`] nodes so its type matches `(target_w, target_s, target_four)`
    /// following IEEE 1800-2023 §11.8.2 (signedness propagates before width extension).
    pub fn coerce(&mut self, mut expr: ExprId, target_w: u32, target_s: bool, target_four: bool, loc: Loc) -> ExprId {
        let target_w = target_w.max(1);
        // Unbased unsized fill literal (`'0`, `'1`, `'x`, `'z`) expands directly to `target_w` bits.
        if let ExprKind::UnbasedUnsized(bs) = self.get(expr).kind {
            let cv = fill_cval(bs, target_w, target_s, target_four);
            return self.alloc(
                ExprKind::Const(cv.clone()),
                RtlType::Integral { width: target_w, signed: target_s, four_state: target_four },
                Some(cv),
                loc,
            );
        }
        let Some((mut cur_w, mut cur_s, cur_four)) = self.get(expr).ty.integral_shape() else {
            return expr;
        };
        let mut cur_cval = self.get(expr).cval.clone();

        // Step 1 (IEEE 11.8.2): when widening (`target_w > cur_w`), convert signedness FIRST so
        // an unsigned context zero-extends a signed operand instead of sign-extending it!
        if target_w >= cur_w && cur_s != target_s {
            cur_cval = cur_cval.map(|c| c.cast(cur_w, target_s));
            expr = self.alloc(
                ExprKind::CastSign { arg: expr, signed: target_s },
                RtlType::Integral { width: cur_w, signed: target_s, four_state: cur_four },
                cur_cval.clone(),
                loc,
            );
            cur_s = target_s;
        }

        // Step 2: resize width (widening uses `cur_s`, narrowing truncates low `target_w` bits).
        if cur_w != target_w {
            let ext_signed = cur_s;
            cur_cval = cur_cval.map(|c| c.cast(target_w, if target_w > cur_w { ext_signed } else { target_s }));
            let next_s = if target_w > cur_w { ext_signed } else { cur_s };
            expr = self.alloc(
                ExprKind::Resize { arg: expr, width: target_w, signed: ext_signed },
                RtlType::Integral { width: target_w, signed: next_s, four_state: cur_four },
                cur_cval.clone(),
                loc,
            );
            cur_w = target_w;
            cur_s = next_s;
        }

        // Step 3: when narrowing (`target_w < old_w`), apply signedness change after truncation if needed.
        if cur_s != target_s {
            cur_cval = cur_cval.map(|c| c.cast(cur_w, target_s));
            expr = self.alloc(
                ExprKind::CastSign { arg: expr, signed: target_s },
                RtlType::Integral { width: cur_w, signed: target_s, four_state: cur_four },
                cur_cval.clone(),
                loc,
            );
            cur_s = target_s;
        }

        // Step 4: explicit 2-state <-> 4-state conversion.
        if cur_four != target_four {
            cur_cval = cur_cval.map(|c| if target_four { c } else { c.two_state() });
            expr = self.alloc(
                ExprKind::CastFourState { arg: expr, four_state: target_four },
                RtlType::Integral { width: cur_w, signed: cur_s, four_state: target_four },
                cur_cval,
                loc,
            );
        }
        expr
    }
}

fn fill_cval(bs: BitState, width: u32, signed: bool, four_state: bool) -> CVal {
    let w = width.max(1);
    match bs {
        BitState::Zero => CVal::new(w, signed, 0),
        BitState::One => CVal::new(w, signed, -1),
        BitState::X => {
            if four_state {
                CVal::all_x(w, signed)
            } else {
                CVal::new(w, signed, 0)
            }
        }
        BitState::Z => {
            if four_state {
                CVal::all_z(w, signed)
            } else {
                CVal::new(w, signed, 0)
            }
        }
    }
}

/// Context-aware IEEE 1800 §11.6 / §11.8 expression binder for one elaborated scope.
pub struct ExprBinder<'a> {
    /// Parsed CST trees of the compilation.
    pub trees: &'a [Tree],
    /// Optional out-of-line CST tree at index `trees.len()` (e.g. `bind_property_in_scope` buffer).
    pub extra_tree: Option<&'a Tree>,
    /// Elaborated design arena.
    pub design: &'a ElaboratedDesign,
    /// Active elaborated instance.
    pub inst_id: InstanceId,
    /// Active lexical generate prefix (`""` or `"g[0]"`).
    pub scope_prefix: &'a str,
    /// Active generate-loop / generate-block constants `(name, CVal)`.
    pub gen_consts: &'a [(String, CVal)],
    /// Active procedural local variables (innermost frame wins).
    pub locals: FxMap<String, LocalBinding>,
    /// Pre-bound expression substitutions (e.g. parameterized SVA sequence/property arguments or `let`).
    pub expr_substs: FxMap<String, ExprId>,
    /// Active subroutine local `typedef`s.
    pub local_typedefs: FxMap<String, RtlType>,
    /// Shared expression arena.
    pub arena: &'a mut ExprArena,
    /// Collected fail-closed binding/typing diagnostics.
    pub diags: Vec<ExprDiag>,
    let_depth: usize,
    locator: Locator,
}

impl<'a> ExprBinder<'a> {
    /// Creates a new [`ExprBinder`] scoped to `(design, inst_id, scope_prefix)`.
    pub fn new(
        trees: &'a [Tree],
        design: &'a ElaboratedDesign,
        inst_id: InstanceId,
        scope_prefix: &'a str,
        gen_consts: &'a [(String, CVal)],
        arena: &'a mut ExprArena,
    ) -> Self {
        let locator = if design.files.is_empty() { Locator::default() } else { Locator::new(design.files.clone()) };
        Self {
            trees,
            extra_tree: None,
            design,
            inst_id,
            scope_prefix,
            gen_consts,
            locals: FxMap::default(),
            expr_substs: FxMap::default(),
            local_typedefs: FxMap::default(),
            arena,
            diags: Vec::new(),
            let_depth: 0,
            locator,
        }
    }

    /// Registers a procedural local variable (loop induction variable or subroutine local/arg).
    pub fn insert_local(&mut self, name: impl Into<String>, binding: LocalBinding) {
        self.locals.insert(name.into(), binding);
    }

    /// Borrows CST tree `idx`, falling back to `extra_tree` when `idx == trees.len()`.
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

    /// Binds `node` in self-determined context.
    ///
    /// Returns `None` and records one or more diagnostics in `self.diags` on any binding or typing error.
    pub fn bind_self(&mut self, tree_idx: usize, node: NodeId) -> Option<ExprId> {
        let raw = self.bind_node(tree_idx, node, None)?;
        if let ExprKind::UnbasedUnsized(bs) = self.arena.get(raw).kind {
            let loc = self.arena.get(raw).loc;
            let four = matches!(bs, BitState::X | BitState::Z);
            return Some(self.arena.coerce(raw, 1, false, four, loc));
        }
        Some(raw)
    }

    /// Binds an untyped `let` or SVA actual expression without premature 1-bit coercion so
    /// unbased unsized fill literals (`'0`, `'1`, `'x`, `'z`) remain [`ExprKind::UnbasedUnsized`]
    /// until sized by an enclosing context-determined expression.
    ///
    /// Returns `None` and records diagnostics in `self.diags` on any binding or typing error.
    pub fn bind_untyped_actual(&mut self, tree_idx: usize, node: NodeId) -> Option<ExprId> {
        self.bind_node(tree_idx, node, None)
    }

    /// Binds `node` in a context of `(width, signed)` using IEEE 1800 context-determined sizing rules.
    ///
    /// Returns `None` and records one or more diagnostics in `self.diags` on any binding or typing error.
    pub fn bind_with_context(&mut self, tree_idx: usize, node: NodeId, width: u32, signed: bool) -> Option<ExprId> {
        let saved_diags = self.diags.len();
        let peek_four = self.peek_self_shape(tree_idx, node).map(|sh| sh.2).unwrap_or(false);
        if self.diags.len() > saved_diags {
            return None;
        }
        let loc = self.loc_of(tree_idx, node);
        let bound = self.bind_node(tree_idx, node, Some((width, signed, peek_four)))?;
        let Some((_, _, b_four)) = self.arena.get(bound).ty.integral_shape() else {
            self.err(loc, "expression in integral context must be integral or packed");
            return None;
        };
        Some(self.arena.coerce(bound, width, signed, b_four, loc))
    }

    /// Binds `node` as an RHS assigned to `lhs_ty`, propagating `max(lhs_w, rhs_w)` with `rhs`'s
    /// signedness into context-determined operators before converting the result to `lhs_ty`.
    ///
    /// Returns `None` and records one or more diagnostics in `self.diags` on any binding or typing error.
    pub fn bind_assigned(&mut self, tree_idx: usize, node: NodeId, lhs_ty: &RtlType) -> Option<ExprId> {
        let loc = self.loc_of(tree_idx, node);
        match lhs_ty {
            RtlType::NonSynthesizable(reason) => {
                self.err(loc, format!("cannot assign to non-synthesizable type '{reason}'"));
                return None;
            }
            RtlType::Void | RtlType::Event | RtlType::Interface { .. } => {
                self.err(loc, "invalid assignment target type");
                return None;
            }
            _ => {}
        }
        if self.tree(tree_idx).kind(node) == SyntaxKind::AssignPattern {
            return self.bind_assign_pattern(tree_idx, node, Some(lhs_ty));
        }
        let Some(lhs_shape) = lhs_ty.integral_shape() else {
            let rhs = self.bind_self(tree_idx, node)?;
            let rhs_ty = &self.arena.get(rhs).ty;
            if !types_assignment_compatible(lhs_ty, rhs_ty) {
                self.err(loc, "incompatible types in unpacked aggregate assignment");
                return None;
            }
            return Some(rhs);
        };
        let is_unbased = self.is_unbased_literal(tree_idx, node);
        let saved_diags = self.diags.len();
        let peek = self.peek_self_shape(tree_idx, node);
        if self.diags.len() > saved_diags {
            return None;
        }
        let rhs_self = peek.unwrap_or(lhs_shape);
        let ctx = assigned_shape(lhs_shape, rhs_self, is_unbased);
        let bound = self.bind_node(tree_idx, node, Some(ctx))?;
        if self.arena.get(bound).ty.integral_shape().is_none() {
            self.err(loc, "cannot assign unpacked aggregate value to integral target");
            return None;
        }
        Some(self.arena.coerce(bound, lhs_shape.0, lhs_shape.1, lhs_shape.2, loc))
    }

    /// Binds `node` and converts it to a 1-bit condition expression (`expr != 0` when multi-bit).
    ///
    /// Returns `None` and records one or more diagnostics in `self.diags` on any binding or typing error.
    pub fn bind_condition(&mut self, tree_idx: usize, node: NodeId) -> Option<ExprId> {
        let raw = self.bind_self(tree_idx, node)?;
        let loc = self.arena.get(raw).loc;
        let Some((w, _, four)) = self.arena.get(raw).ty.integral_shape() else {
            self.err(loc, "condition expression must be integral or packed");
            return None;
        };
        if w == 1 {
            return Some(raw);
        }
        let cv = self.arena.get(raw).cval.as_ref().and_then(|c| CVal::eval_unary("|", c.clone()));
        Some(self.arena.alloc(
            ExprKind::Unary { op: UnaryOp::RedOr, arg: raw },
            RtlType::Integral { width: 1, signed: false, four_state: four },
            cv,
            loc,
        ))
    }

    /// Computes the 1-based source [`Loc`] (indexing `design.files`) of CST `node`.
    pub fn loc_of(&mut self, tree_idx: usize, node: NodeId) -> Loc {
        let t = self.tree(tree_idx);
        let Some((tok, _)) = t.token_bounds(node) else {
            return Loc::default();
        };
        let raw = self.locator.loc(tree_idx, t, tok);
        let path = self.locator.files().get(raw.file as usize).cloned().unwrap_or_default();
        let file_id = self.design.files.iter().position(|f| f == &path).map_or(raw.file, |i| i as u32);
        Loc { file: file_id, line: raw.line, column: raw.column, expansion: raw.expansion }
    }

    fn err(&mut self, loc: Loc, msg: impl Into<String>) {
        self.diags.push(ExprDiag { loc, message: msg.into(), unsupported: false });
    }

    fn unsup(&mut self, loc: Loc, msg: impl Into<String>) {
        self.diags.push(ExprDiag { loc, message: msg.into(), unsupported: true });
    }

    fn is_unbased_literal(&self, tree_idx: usize, mut node: NodeId) -> bool {
        let t = self.tree(tree_idx);
        while t.kind(node) == SyntaxKind::ParenExpr {
            let Some(inner) = Self::expr_kids(t, node).into_iter().next() else { return false };
            node = inner;
        }
        if t.kind(node) == SyntaxKind::NameExpr {
            let raw = t.bare_text(node);
            let ident = raw.trim().strip_prefix('\\').unwrap_or(raw.trim());
            if let Some(&sub_id) = self.expr_substs.get(ident) {
                return matches!(self.arena.get(sub_id).kind, ExprKind::UnbasedUnsized(_));
            }
            return false;
        }
        if t.kind(node) != SyntaxKind::Literal {
            return false;
        }
        t.token_bounds(node).is_some_and(|(lo, _)| is_unbased_unsized_tok(&t.tokens[lo as usize].text))
    }

    fn expr_kids(t: &Tree, node: NodeId) -> Vec<NodeId> {
        t.children(node).filter(|&c| t.kind(c) != SyntaxKind::Attribute).collect()
    }

    fn own_toks(t: &Tree, node: NodeId) -> Vec<&str> {
        t.node(node)
            .children
            .iter()
            .filter_map(|c| match *c {
                sv_syntax::Child::Token(i) => {
                    let tok = &t.tokens[i as usize];
                    (!tok.kind.is_trivia()).then_some(&*tok.text)
                }
                sv_syntax::Child::Node(_) => None,
            })
            .collect()
    }

    /// Lightweight self-determined `(width, signed, four_state)` computation before context pushdown.
    pub fn peek_self_shape(&mut self, tree_idx: usize, node: NodeId) -> Option<(u32, bool, bool)> {
        let saved_len = self.arena.exprs.len();
        let saved_diags = self.diags.len();
        let bound_opt = self.bind_node(tree_idx, node, None);
        let res = bound_opt.and_then(|id| self.arena.get(id).ty.integral_shape());
        self.arena.exprs.truncate(saved_len);
        if bound_opt.is_some() {
            self.diags.truncate(saved_diags);
        }
        res
    }

    fn bind_node(&mut self, tree_idx: usize, node: NodeId, ctx: Option<(u32, bool, bool)>) -> Option<ExprId> {
        let t = self.tree(tree_idx);
        let loc = self.loc_of(tree_idx, node);
        let kids = Self::expr_kids(t, node);
        match t.kind(node) {
            SyntaxKind::ParenExpr => {
                let inner = *kids.first()?;
                self.bind_node(tree_idx, inner, ctx)
            }
            SyntaxKind::Literal => self.bind_literal(tree_idx, node, ctx, loc),
            SyntaxKind::NameExpr => self.bind_name(tree_idx, node, ctx, loc),
            SyntaxKind::ScopedExpr => self.bind_scoped(tree_idx, node, ctx, loc),
            SyntaxKind::PrefixExpr => self.bind_prefix(tree_idx, node, &kids, ctx, loc),
            SyntaxKind::BinaryExpr => self.bind_binary(tree_idx, node, &kids, ctx, loc),
            SyntaxKind::CondExpr => self.bind_cond(tree_idx, &kids, ctx, loc),
            SyntaxKind::InsideExpr => self.bind_inside(tree_idx, node, &kids, ctx, loc),
            SyntaxKind::ConcatExpr => self.bind_concat(tree_idx, node, &kids, ctx, loc),
            SyntaxKind::SelectExpr => self.bind_select(tree_idx, node, &kids, ctx, loc),
            SyntaxKind::MemberExpr => self.bind_member(tree_idx, node, &kids, ctx, loc),
            SyntaxKind::CastExpr => self.bind_cast(tree_idx, node, &kids, ctx, loc),
            SyntaxKind::CallExpr => self.bind_call(tree_idx, node, &kids, ctx, loc),
            SyntaxKind::AssignPattern => self.bind_assign_pattern(tree_idx, node, None),
            SyntaxKind::PostfixExpr => {
                self.err(
                    loc,
                    "increment/decrement inside subexpression is not synthesizable; use as standalone statement",
                );
                None
            }
            SyntaxKind::TimedExpr => {
                self.unsup(loc, "intra-assignment timing control is not synthesizable in RTL");
                None
            }
            other => {
                self.unsup(loc, format!("unsupported expression construct '{other:?}'"));
                None
            }
        }
    }

    fn apply_ctx(&mut self, id: ExprId, ctx: Option<(u32, bool, bool)>, propagates: bool, loc: Loc) -> ExprId {
        match ctx {
            Some((cw, cs, c4)) if propagates => self.arena.coerce(id, cw, cs, c4, loc),
            _ => id,
        }
    }

    fn bind_literal(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        ctx: Option<(u32, bool, bool)>,
        loc: Loc,
    ) -> Option<ExprId> {
        let raw = self.tree(tree_idx).bare_text(node);
        let trimmed = raw.trim();
        if trimmed == "null" {
            self.err(loc, "'null' handle literal is not synthesizable in RTL");
            return None;
        }
        if is_unbased_unsized_tok(trimmed) {
            let bs = match trimmed.chars().nth(1).unwrap_or('0') {
                '1' => BitState::One,
                'x' | 'X' => BitState::X,
                'z' | 'Z' => BitState::Z,
                _ => BitState::Zero,
            };
            if let Some((cw, cs, c4)) = ctx {
                let cv = fill_cval(bs, cw, cs, c4);
                return Some(self.arena.alloc(
                    ExprKind::Const(cv.clone()),
                    RtlType::Integral { width: cw.max(1), signed: cs, four_state: c4 },
                    Some(cv),
                    loc,
                ));
            }
            let four = matches!(bs, BitState::X | BitState::Z);
            let cv = fill_cval(bs, 1, false, four);
            return Some(self.arena.alloc(
                ExprKind::UnbasedUnsized(bs),
                RtlType::Integral { width: 1, signed: false, four_state: four },
                Some(cv),
                loc,
            ));
        }
        let Some(ti) = literal_type(trimmed) else {
            self.err(loc, format!("invalid or unsupported literal '{trimmed}'"));
            return None;
        };
        if matches!(ti.text.as_str(), "real" | "realtime") {
            self.err(loc, format!("non-synthesizable {} literal '{trimmed}'", ti.text));
            return None;
        }
        let (w, s, four) = ti.integral?;
        let Some(parsed) = CVal::parse_literal(trimmed) else {
            self.err(loc, format!("cannot evaluate literal '{trimmed}'"));
            return None;
        };
        let cv = CVal::four(w.max(1), s, parsed.bits_v(), parsed.u);
        let cv = if four { cv } else { cv.two_state() };
        let id = self.arena.alloc(
            ExprKind::Const(cv.clone()),
            RtlType::Integral { width: w.max(1), signed: s, four_state: four },
            Some(cv),
            loc,
        );
        // Sized/unsized integer literals are self-determined unless coerced by caller, EXCEPT
        // when caller explicitly passed `ctx` to `coerce` — wait: in `bind_node`, `ctx` is passed
        // down to children of `+`, `?:`, etc., so leaf literals MUST be coerced to `ctx` if `ctx` is `Some`!
        Some(self.apply_ctx(id, ctx, ctx.is_some(), loc))
    }

    fn bind_name(&mut self, tree_idx: usize, node: NodeId, ctx: Option<(u32, bool, bool)>, loc: Loc) -> Option<ExprId> {
        let t = self.tree(tree_idx);
        let raw = t.bare_text(node);
        let ident = raw.trim().strip_prefix('\\').unwrap_or(raw.trim());
        if matches!(ident, "this" | "super" | "null") {
            self.err(loc, format!("class keyword '{ident}' is not synthesizable in RTL"));
            return None;
        }
        if ident.starts_with('$') && ident != "$" {
            return self.bind_syscall(tree_idx, ident, &[], ctx, loc);
        }

        // 0. Pre-bound expression substitution (SVA formal argument or `let` parameter).
        if let Some(&sub_id) = self.expr_substs.get(ident) {
            return Some(self.apply_ctx(sub_id, ctx, ctx.is_some(), loc));
        }

        // 1. Procedural local variable (`for` loop variable or subroutine local/arg).
        if let Some(lb) = self.locals.get(ident).cloned() {
            if let RtlType::NonSynthesizable(reason) = &lb.ty {
                self.err(loc, format!("variable '{ident}' has non-synthesizable type '{reason}'"));
                return None;
            }
            let id = self.arena.alloc(ExprKind::Local(lb.id), lb.ty.clone(), lb.cval, loc);
            return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
        }

        // 2. Active `genvar` / generate-scope constant.
        if let Some((_, cv)) = self.gen_consts.iter().find(|(n, _)| n == ident) {
            let cv = cv.clone();
            // Also check if there is an elaborated symbol in `inst` so typed parameter symbols keep their `RtlType`.
            let inst = self.design.instance(self.inst_id);
            if let Some(sid) = inst.resolve_lexical(self.scope_prefix, ident) {
                let sym = self.design.symbol(sid);
                if matches!(sym.kind, SymbolKind::Param { .. } | SymbolKind::Genvar) {
                    let ty = sym.ty.clone();
                    let val = sym.cval.clone().unwrap_or(cv);
                    let id = self.arena.alloc(ExprKind::Const(val.clone()), ty, Some(val), loc);
                    return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
                }
            }
            let ty = RtlType::Integral { width: cv.w.max(1), signed: cv.s, four_state: !cv.known() };
            let id = self.arena.alloc(ExprKind::Const(cv.clone()), ty, Some(cv), loc);
            return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
        }

        // 3. Per-instance lexical symbol (innermost generate prefix outward).
        let inst = self.design.instance(self.inst_id);
        if let Some(sid) = inst.resolve_lexical(self.scope_prefix, ident) {
            return self.finish_symbol_ref(sid, ctx, loc);
        }

        // 4. Per-instance enum constant.
        if let Some((cv, ty)) = inst.resolve_enum(self.scope_prefix, ident) {
            let cv = cv.clone();
            let ty = ty.clone();
            let id = self.arena.alloc(ExprKind::Const(cv.clone()), ty, Some(cv), loc);
            return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
        }

        // 5. Imported package / `$unit` constant or enum value.
        if let Some((cv, ty)) = self.design.resolve_imported_const(self.inst_id, self.scope_prefix, ident) {
            let cv = cv.clone();
            let ty = ty.clone();
            let id = self.arena.alloc(ExprKind::Const(cv.clone()), ty, Some(cv), loc);
            return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
        }

        // 6. Zero-arg (or all-default-arg) `let` declaration (`let do_push = push && !full;`).
        if let Some((lt_tree, lt_node, lt_inst, lt_prefix, lt_consts)) = self.resolve_let_decl(None, ident) {
            return self.expand_let_decl(lt_tree, lt_node, lt_inst, &lt_prefix, &lt_consts, tree_idx, None, ctx, loc);
        }

        // Fail-closed: NEVER invent an implicit net for an undeclared identifier!
        self.err(loc, format!("undeclared identifier '{ident}'"));
        None
    }

    fn finish_symbol_ref(&mut self, sid: SymbolId, ctx: Option<(u32, bool, bool)>, loc: Loc) -> Option<ExprId> {
        let sym = self.design.symbol(sid);
        if let RtlType::NonSynthesizable(reason) = &sym.ty {
            self.err(loc, format!("signal '{}' has non-synthesizable type '{reason}'", sym.full_path));
            return None;
        }
        let ty = sym.ty.clone();
        let id = match (&sym.kind, &sym.cval) {
            (SymbolKind::Param { .. } | SymbolKind::Genvar, Some(cv)) if ty.is_packed() => {
                self.arena.alloc(ExprKind::Const(cv.clone()), ty, Some(cv.clone()), loc)
            }
            _ => self.arena.alloc(ExprKind::Symbol(sid), ty, None, loc),
        };
        Some(self.apply_ctx(id, ctx, ctx.is_some(), loc))
    }

    fn bind_scoped(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        ctx: Option<(u32, bool, bool)>,
        loc: Loc,
    ) -> Option<ExprId> {
        let t = self.tree(tree_idx);
        let toks: Vec<&str> = t.token_range(node).into_iter().map(|ti| &*t.tokens[ti as usize].text).collect();
        if let [pkg, "::", ident] = toks.as_slice() {
            let clean_ident = ident.strip_prefix('\\').unwrap_or(ident);
            if let Some((cv, ty)) = self.design.resolve_scoped_const(pkg, clean_ident) {
                let cv = cv.clone();
                let ty = ty.clone();
                let id = self.arena.alloc(ExprKind::Const(cv.clone()), ty, Some(cv), loc);
                return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
            }
            if let Some((lt_tree, lt_node, lt_inst, lt_prefix, lt_consts)) =
                self.resolve_let_decl(Some(pkg), clean_ident)
            {
                return self
                    .expand_let_decl(lt_tree, lt_node, lt_inst, &lt_prefix, &lt_consts, tree_idx, None, ctx, loc);
            }
            self.err(loc, format!("undeclared scoped identifier '{pkg}::{clean_ident}'"));
            return None;
        }
        self.err(loc, format!("unsupported scoped expression '{}'", t.bare_text(node)));
        None
    }

    fn bind_prefix(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        kids: &[NodeId],
        ctx: Option<(u32, bool, bool)>,
        loc: Loc,
    ) -> Option<ExprId> {
        let t = self.tree(tree_idx);
        let own = Self::own_toks(t, node);
        let op_str = *own.first()?;
        let arg_node = *kids.first()?;
        if matches!(op_str, "++" | "--") {
            self.err(
                loc,
                "prefix increment/decrement inside subexpression is not synthesizable; use as standalone statement",
            );
            return None;
        }
        match op_str {
            "+" | "-" | "~" => {
                let own_shape = self.peek_self_shape(tree_idx, arg_node)?;
                let target = ctx.unwrap_or(own_shape);
                let arg = self.bind_node(tree_idx, arg_node, Some(target))?;
                let arg = self.arena.coerce(arg, target.0, target.1, target.2, loc);
                let uop = match op_str {
                    "+" => return Some(arg),
                    "-" => UnaryOp::Neg,
                    _ => UnaryOp::BitNot,
                };
                let cval = self
                    .arena
                    .get(arg)
                    .cval
                    .as_ref()
                    .and_then(|c| CVal::eval_unary(op_str, c.clone()))
                    .map(|c| if target.2 { c } else { c.two_state() });
                Some(self.arena.alloc(
                    ExprKind::Unary { op: uop, arg },
                    RtlType::Integral { width: target.0, signed: target.1, four_state: target.2 },
                    cval,
                    loc,
                ))
            }
            "!" | "&" | "~&" | "|" | "~|" | "^" | "~^" | "^~" => {
                let arg = self.bind_self(tree_idx, arg_node)?;
                let Some((_, _, four)) = self.arena.get(arg).ty.integral_shape() else {
                    self.err(loc, format!("operand of '{op_str}' must be integral"));
                    return None;
                };
                let uop = match op_str {
                    "!" => UnaryOp::LogNot,
                    "&" => UnaryOp::RedAnd,
                    "~&" => UnaryOp::RedNand,
                    "|" => UnaryOp::RedOr,
                    "~|" => UnaryOp::RedNor,
                    "^" => UnaryOp::RedXor,
                    _ => UnaryOp::RedXnor,
                };
                let cval = self
                    .arena
                    .get(arg)
                    .cval
                    .as_ref()
                    .and_then(|c| CVal::eval_unary(op_str, c.clone()))
                    .map(|c| if four { c } else { c.two_state() });
                let id = self.arena.alloc(
                    ExprKind::Unary { op: uop, arg },
                    RtlType::Integral { width: 1, signed: false, four_state: four },
                    cval,
                    loc,
                );
                Some(self.apply_ctx(id, ctx, ctx.is_some(), loc))
            }
            _ => {
                self.err(loc, format!("unsupported prefix operator '{op_str}'"));
                None
            }
        }
    }

    fn bind_binary(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        kids: &[NodeId],
        ctx: Option<(u32, bool, bool)>,
        loc: Loc,
    ) -> Option<ExprId> {
        let t = self.tree(tree_idx);
        let own = Self::own_toks(t, node);
        let op_str = *own.first()?;
        let (lhs_node, rhs_node) = (*kids.first()?, *kids.get(1)?);

        if ASSIGN.contains(&op_str) {
            self.err(loc, format!("assignment operator '{op_str}' inside expression is not synthesizable"));
            return None;
        }

        if COMPARE.contains(&op_str) {
            let ls = self.peek_self_shape(tree_idx, lhs_node)?;
            let rs = self.peek_self_shape(tree_idx, rhs_node)?;
            let op_ctx = binary_shape(ls, rs, false);
            let lhs = self.bind_node(tree_idx, lhs_node, Some(op_ctx))?;
            let lhs = self.arena.coerce(lhs, op_ctx.0, op_ctx.1, op_ctx.2, loc);
            let rhs = self.bind_node(tree_idx, rhs_node, Some(op_ctx))?;
            let rhs = self.arena.coerce(rhs, op_ctx.0, op_ctx.1, op_ctx.2, loc);

            // Fail-closed rule for wildcard equality (`==?`, `!=?`): RHS must fold to a constant!
            if matches!(op_str, "==?" | "!=?") && self.arena.get(rhs).cval.is_none() {
                self.err(
                    loc,
                    format!("wildcard equality '{op_str}' requires a constant right-hand operand in synthesizable RTL"),
                );
                return None;
            }

            let four = !matches!(op_str, "===" | "!==") && (ls.2 || rs.2);
            let bop = match op_str {
                "==" => BinaryOp::Eq,
                "!=" => BinaryOp::Neq,
                "===" => BinaryOp::CaseEq,
                "!==" => BinaryOp::CaseNeq,
                "==?" => BinaryOp::WildEq,
                "!=?" => BinaryOp::WildNeq,
                "<" => BinaryOp::Lt,
                "<=" => BinaryOp::Le,
                ">" => BinaryOp::Gt,
                _ => BinaryOp::Ge,
            };
            let cval = match (&self.arena.get(lhs).cval, &self.arena.get(rhs).cval) {
                (Some(a), Some(b)) => {
                    CVal::eval_binary(op_str, a.clone(), b.clone()).map(|c| if four { c } else { c.two_state() })
                }
                _ => None,
            };
            let id = self.arena.alloc(
                ExprKind::Binary { op: bop, lhs, rhs },
                RtlType::Integral { width: 1, signed: false, four_state: four },
                cval,
                loc,
            );
            return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
        }

        if LOGICAL.contains(&op_str) {
            let lhs = self.bind_condition(tree_idx, lhs_node)?;
            let rhs = self.bind_condition(tree_idx, rhs_node)?;
            let four = self.arena.get(lhs).ty.is_four_state() || self.arena.get(rhs).ty.is_four_state();
            let bop = match op_str {
                "&&" => BinaryOp::LogAnd,
                "||" => BinaryOp::LogOr,
                "->" => BinaryOp::LogImp,
                _ => BinaryOp::LogEqv,
            };
            let cval = match (&self.arena.get(lhs).cval, &self.arena.get(rhs).cval) {
                (Some(a), Some(b)) => {
                    CVal::eval_binary(op_str, a.clone(), b.clone()).map(|c| if four { c } else { c.two_state() })
                }
                _ => None,
            };
            let id = self.arena.alloc(
                ExprKind::Binary { op: bop, lhs, rhs },
                RtlType::Integral { width: 1, signed: false, four_state: four },
                cval,
                loc,
            );
            return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
        }

        if SHIFT.contains(&op_str) {
            let ls = self.peek_self_shape(tree_idx, lhs_node)?;
            let self_sh = if op_str == "**" { (ls.0, ls.1, true) } else { ls };
            let target = match ctx {
                Some((cw, cs, c4)) => (cw, cs, c4 || (op_str == "**")),
                None => self_sh,
            };
            let lhs = self.bind_node(tree_idx, lhs_node, Some(target))?;
            let lhs = self.arena.coerce(lhs, target.0, target.1, target.2, loc);
            // RHS of shift / power is strictly self-determined per IEEE 1800 §11.6.1!
            let rhs = self.bind_self(tree_idx, rhs_node)?;
            if self.arena.get(rhs).ty.integral_shape().is_none() {
                self.err(loc, format!("shift/power amount of '{op_str}' must be integral"));
                return None;
            }
            let bop = match op_str {
                "<<" | "<<<" => BinaryOp::Shl,
                ">>" => BinaryOp::LShr,
                ">>>" => {
                    if target.1 {
                        BinaryOp::AShr
                    } else {
                        BinaryOp::LShr
                    }
                }
                _ => BinaryOp::Pow,
            };
            let cval = match (&self.arena.get(lhs).cval, &self.arena.get(rhs).cval) {
                (Some(a), Some(b)) => CVal::eval_binary(op_str, a.clone(), b.clone()).map(|c| {
                    let c = c.cast(target.0, target.1);
                    if target.2 {
                        c
                    } else {
                        c.two_state()
                    }
                }),
                _ => None,
            };
            if op_str == "**" && cval.is_none() && self.arena.get(rhs).cval.is_none() {
                self.unsup(loc, "exponentiation '**' with non-constant exponent is not synthesizable");
                return None;
            }
            let out = self.arena.alloc(
                ExprKind::Binary { op: bop, lhs, rhs },
                RtlType::Integral { width: target.0, signed: target.1, four_state: target.2 },
                cval,
                loc,
            );
            return Some(match ctx {
                Some((cw, cs, c4)) => self.arena.coerce(out, cw, cs, c4, loc),
                None => out,
            });
        }

        if ARITH.contains(&op_str) {
            let ls = self.peek_self_shape(tree_idx, lhs_node)?;
            let rs = self.peek_self_shape(tree_idx, rhs_node)?;
            let force_four = matches!(op_str, "/" | "%");
            let own = binary_shape(ls, rs, force_four);
            let target = match ctx {
                Some((cw, cs, c4)) => (cw.max(own.0), cs, c4 || own.2),
                None => own,
            };
            let lhs = self.bind_node(tree_idx, lhs_node, Some(target))?;
            let lhs = self.arena.coerce(lhs, target.0, target.1, target.2, loc);
            let rhs = self.bind_node(tree_idx, rhs_node, Some(target))?;
            let rhs = self.arena.coerce(rhs, target.0, target.1, target.2, loc);
            let bop = match op_str {
                "+" => BinaryOp::Add,
                "-" => BinaryOp::Sub,
                "*" => BinaryOp::Mul,
                "/" => BinaryOp::Div,
                "%" => BinaryOp::Mod,
                "&" => BinaryOp::BitAnd,
                "|" => BinaryOp::BitOr,
                "^" => BinaryOp::BitXor,
                _ => BinaryOp::BitXnor,
            };
            let cval = match (&self.arena.get(lhs).cval, &self.arena.get(rhs).cval) {
                (Some(a), Some(b)) => CVal::eval_binary(op_str, a.clone(), b.clone()).map(|c| {
                    let c = c.cast(target.0, target.1);
                    if target.2 {
                        c
                    } else {
                        c.two_state()
                    }
                }),
                _ => None,
            };
            let out = self.arena.alloc(
                ExprKind::Binary { op: bop, lhs, rhs },
                RtlType::Integral { width: target.0, signed: target.1, four_state: target.2 },
                cval,
                loc,
            );
            return Some(match ctx {
                Some((cw, cs, c4)) => self.arena.coerce(out, cw, cs, c4, loc),
                None => out,
            });
        }

        self.err(loc, format!("unsupported binary operator '{op_str}'"));
        None
    }

    fn bind_cond(
        &mut self,
        tree_idx: usize,
        kids: &[NodeId],
        ctx: Option<(u32, bool, bool)>,
        loc: Loc,
    ) -> Option<ExprId> {
        let (c_node, t_node, e_node) = (*kids.first()?, *kids.get(1)?, *kids.get(2)?);
        let cond = self.bind_condition(tree_idx, c_node)?;
        let d0 = self.diags.len();
        let ts = self.peek_self_shape(tree_idx, t_node);
        if self.diags.len() > d0 {
            return None;
        }
        let d1 = self.diags.len();
        let es = self.peek_self_shape(tree_idx, e_node);
        if self.diags.len() > d1 {
            return None;
        }
        if ts.is_some() != es.is_some() {
            self.err(loc, "incompatible packed and unpacked branch types in conditional expression");
            return None;
        }
        if let (Some(ts), Some(es)) = (ts, es) {
            let own = binary_shape(ts, es, false);
            let target = match ctx {
                Some((cw, cs, c4)) => (cw.max(own.0), cs, c4 || own.2 || self.arena.get(cond).ty.is_four_state()),
                None => (own.0, own.1, own.2 || self.arena.get(cond).ty.is_four_state()),
            };
            let then_val = self.bind_node(tree_idx, t_node, Some(target))?;
            let then_val = self.arena.coerce(then_val, target.0, target.1, target.2, loc);
            let else_val = self.bind_node(tree_idx, e_node, Some(target))?;
            let else_val = self.arena.coerce(else_val, target.0, target.1, target.2, loc);
            let cval = match &self.arena.get(cond).cval {
                Some(cc) => match cc.truth() {
                    Some(true) => self.arena.get(then_val).cval.clone(),
                    Some(false) => self.arena.get(else_val).cval.clone(),
                    None => match (&self.arena.get(then_val).cval, &self.arena.get(else_val).cval) {
                        (Some(a), Some(b)) => Some(CVal::merge(a, b)),
                        _ => None,
                    },
                },
                None => None,
            };
            let out = self.arena.alloc(
                ExprKind::Cond { cond, then_val, else_val },
                RtlType::Integral { width: target.0, signed: target.1, four_state: target.2 },
                cval,
                loc,
            );
            return Some(match ctx {
                Some((cw, cs, c4)) => self.arena.coerce(out, cw, cs, c4, loc),
                None => out,
            });
        }
        // Unpacked aggregate conditional (`cond ? struct_a : struct_b`).
        let then_val = self.bind_self(tree_idx, t_node)?;
        let else_val = self.bind_self(tree_idx, e_node)?;
        let res_ty = self.arena.get(then_val).ty.clone();
        let else_ty = &self.arena.get(else_val).ty;
        if !types_assignment_compatible(&res_ty, else_ty) {
            self.err(loc, "incompatible unpacked branch types in conditional expression");
            return None;
        }
        Some(self.arena.alloc(ExprKind::Cond { cond, then_val, else_val }, res_ty, None, loc))
    }

    fn bind_inside(
        &mut self,
        tree_idx: usize,
        _node: NodeId,
        kids: &[NodeId],
        ctx: Option<(u32, bool, bool)>,
        loc: Loc,
    ) -> Option<ExprId> {
        let lhs_node = *kids.first()?;
        let lhs_shape = self.peek_self_shape(tree_idx, lhs_node)?;
        let mut max_shape = lhs_shape;
        let t = self.tree(tree_idx);
        let range_items: Vec<NodeId> = kids[1..]
            .iter()
            .flat_map(|&k| if t.kind(k) == SyntaxKind::RangeList { Self::expr_kids(t, k) } else { vec![k] })
            .collect();
        for &k in &range_items {
            let t = self.tree(tree_idx);
            if t.kind(k) == SyntaxKind::OpenRange {
                for sub in Self::expr_kids(t, k) {
                    if let Some(sh) = self.peek_self_shape(tree_idx, sub) {
                        max_shape = binary_shape(max_shape, sh, false);
                    }
                }
            } else if let Some(sh) = self.peek_self_shape(tree_idx, k) {
                max_shape = binary_shape(max_shape, sh, false);
            }
        }
        let expr = self.bind_node(tree_idx, lhs_node, Some(max_shape))?;
        let expr = self.arena.coerce(expr, max_shape.0, max_shape.1, max_shape.2, loc);
        let mut items = Vec::new();
        for &k in &range_items {
            let t = self.tree(tree_idx);
            if t.kind(k) == SyntaxKind::OpenRange {
                let rkids = Self::expr_kids(t, k);
                if rkids.len() == 2 {
                    let lo = self.bind_node(tree_idx, rkids[0], Some(max_shape))?;
                    let lo = self.arena.coerce(lo, max_shape.0, max_shape.1, max_shape.2, loc);
                    let hi = self.bind_node(tree_idx, rkids[1], Some(max_shape))?;
                    let hi = self.arena.coerce(hi, max_shape.0, max_shape.1, max_shape.2, loc);
                    items.push(InsideItem::Range(lo, hi));
                }
            } else {
                let it = self.bind_node(tree_idx, k, Some(max_shape))?;
                let it = self.arena.coerce(it, max_shape.0, max_shape.1, max_shape.2, loc);
                items.push(InsideItem::Single(it));
            }
        }
        // Fold constant `inside` when all items are constant.
        let cval = self.fold_inside(expr, &items);
        let id = self.arena.alloc(
            ExprKind::Inside { expr, items },
            RtlType::Integral { width: 1, signed: false, four_state: true },
            cval,
            loc,
        );
        Some(self.apply_ctx(id, ctx, ctx.is_some(), loc))
    }

    fn fold_inside(&self, expr: ExprId, items: &[InsideItem]) -> Option<CVal> {
        let ev = self.arena.get(expr).cval.as_ref()?;
        let mut any_unknown = false;
        for it in items {
            match it {
                InsideItem::Single(s) => {
                    let sv = self.arena.get(*s).cval.as_ref()?;
                    let m = CVal::eval_binary("==?", ev.clone(), sv.clone())?;
                    match m.truth() {
                        Some(true) => return Some(CVal::bool(true)),
                        Some(false) => {}
                        None => any_unknown = true,
                    }
                }
                InsideItem::Range(lo, hi) => {
                    let lv = self.arena.get(*lo).cval.as_ref()?;
                    let hv = self.arena.get(*hi).cval.as_ref()?;
                    let ge = CVal::eval_binary(">=", ev.clone(), lv.clone())?;
                    let le = CVal::eval_binary("<=", ev.clone(), hv.clone())?;
                    let both = CVal::eval_binary("&&", ge, le)?;
                    match both.truth() {
                        Some(true) => return Some(CVal::bool(true)),
                        Some(false) => {}
                        None => any_unknown = true,
                    }
                }
            }
        }
        Some(if any_unknown { CVal::all_x(1, false) } else { CVal::bool(false) })
    }

    fn bind_concat(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        kids: &[NodeId],
        ctx: Option<(u32, bool, bool)>,
        loc: Loc,
    ) -> Option<ExprId> {
        let t = self.tree(tree_idx);
        let own = Self::own_toks(t, node);
        if own.iter().any(|tok| matches!(*tok, "<<" | ">>")) {
            self.unsup(
                loc,
                "streaming concatenation '{<< ...}' / '{>> ...}' is not yet supported in RTL netlist lowering",
            );
            return None;
        }
        if kids.is_empty() {
            self.err(loc, "empty concatenation '{}' is illegal");
            return None;
        }
        let has_comma = own.contains(&",");
        // Replication `{N{a, b}}` has two children, no comma at outer level, and second child is `ConcatExpr`.
        if !has_comma && kids.len() == 2 && t.kind(kids[1]) == SyntaxKind::ConcatExpr {
            let count_expr = self.bind_self(tree_idx, kids[0])?;
            let Some(cnt_cval) = self.arena.get(count_expr).cval.clone() else {
                self.err(loc, "replication count must be a constant expression");
                return None;
            };
            let Some(count) = cval_nonneg_u32(&cnt_cval, MAX_ARRAY_ELEMS) else {
                self.err(loc, "replication count is negative, unknown, or exceeds maximum limit");
                return None;
            };
            if count == 0 {
                self.validate_zero_rep_concat(tree_idx, kids[1])?;
                self.err(loc, "zero replication count is only legal inside a non-empty concatenation");
                return None;
            }
            let inner = self.bind_self(tree_idx, kids[1])?;
            let Some((iw, _, ifour)) = self.arena.get(inner).ty.integral_shape() else {
                self.err(loc, "replicated operand must be integral/packed");
                return None;
            };
            let Some(total_w) = count.checked_mul(iw).filter(|w| (1..=MAX_BIT_WIDTH).contains(w)) else {
                self.err(loc, "replicated bit width exceeds maximum limit");
                return None;
            };
            let cval = self.arena.get(inner).cval.as_ref().map(|ic| {
                let mut acc = ic.clone();
                for _ in 1..count {
                    acc = CVal::concat(&acc, ic);
                }
                acc
            });
            let id = self.arena.alloc(
                ExprKind::Replicate { count, expr: inner },
                RtlType::Integral { width: total_w, signed: false, four_state: ifour },
                cval,
                loc,
            );
            return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
        }

        // Concatenation `{a, b, ...}`: operands are strictly self-determined per IEEE 1800 §11.6.1.
        let mut elems = Vec::with_capacity(kids.len());
        let mut total_w = 0u32;
        let mut four = false;
        let mut all_const = true;
        for &k in kids {
            if self.is_unbased_literal(tree_idx, k) {
                self.err(loc, "unsized unbased literal is illegal inside concatenation");
                return None;
            }
            if self.try_zero_replication(tree_idx, k)? {
                continue;
            }
            let e = self.bind_self(tree_idx, k)?;
            let Some((ew, _, e4)) = self.arena.get(e).ty.integral_shape() else {
                self.err(loc, "concatenation operand must be integral or packed");
                return None;
            };
            let Some(next_w) = total_w.checked_add(ew).filter(|&w| w <= MAX_BIT_WIDTH) else {
                self.err(loc, "concatenation bit width exceeds maximum limit");
                return None;
            };
            total_w = next_w;
            four |= e4;
            all_const &= self.arena.get(e).cval.is_some();
            elems.push(e);
        }
        if elems.is_empty() {
            self.err(loc, "concatenation with only zero-width replications is illegal");
            return None;
        }
        let cval = if all_const {
            let mut it = elems.iter().filter_map(|&e| self.arena.get(e).cval.clone());
            let first = it.next()?;
            Some(it.fold(first, |acc, x| CVal::concat(&acc, &x)))
        } else {
            None
        };
        let id = self.arena.alloc(
            ExprKind::Concat(elems),
            RtlType::Integral { width: total_w.max(1), signed: false, four_state: four },
            cval,
            loc,
        );
        Some(self.apply_ctx(id, ctx, ctx.is_some(), loc))
    }

    /// Checks whether `k` is a replication `{0{inner}}`. When `k` is a replication with count `0`,
    /// validates `inner` so inner errors still surface and returns `Some(true)`. Returns `Some(false)`
    /// when `k` is not a zero-count replication, or `None` when a replication error occurred.
    fn try_zero_replication(&mut self, tree_idx: usize, k: NodeId) -> Option<bool> {
        let t = self.tree(tree_idx);
        if t.kind(k) != SyntaxKind::ConcatExpr {
            return Some(false);
        }
        let own = Self::own_toks(t, k);
        let kids = Self::expr_kids(t, k);
        if own.contains(&",") || kids.len() != 2 || t.kind(kids[1]) != SyntaxKind::ConcatExpr {
            return Some(false);
        }
        let saved_len = self.arena.exprs.len();
        let count_expr = self.bind_self(tree_idx, kids[0])?;
        let Some(cnt_cval) = self.arena.get(count_expr).cval.clone() else {
            let kloc = self.loc_of(tree_idx, k);
            self.err(kloc, "replication count must be a constant expression");
            return None;
        };
        let Some(count) = cval_nonneg_u32(&cnt_cval, MAX_ARRAY_ELEMS) else {
            let kloc = self.loc_of(tree_idx, k);
            self.err(kloc, "replication count is negative, unknown, or exceeds maximum limit");
            return None;
        };
        if count != 0 {
            self.arena.exprs.truncate(saved_len);
            return Some(false);
        }
        self.validate_zero_rep_concat(tree_idx, kids[1])?;
        self.arena.exprs.truncate(saved_len);
        Some(true)
    }

    /// Validates the inner `ConcatExpr` of a `{0{...}}` replication so undeclared identifiers,
    /// unsized unbased literals, and non-integral operands inside `{0{...}}` emit diagnostics.
    fn validate_zero_rep_concat(&mut self, tree_idx: usize, concat_node: NodeId) -> Option<()> {
        let t = self.tree(tree_idx);
        let loc = self.loc_of(tree_idx, concat_node);
        let own = Self::own_toks(t, concat_node);
        let kids = Self::expr_kids(t, concat_node);
        if own.iter().any(|tok| matches!(*tok, "<<" | ">>")) {
            self.unsup(
                loc,
                "streaming concatenation '{<< ...}' / '{>> ...}' is not yet supported in RTL netlist lowering",
            );
            return None;
        }
        if kids.is_empty() {
            self.err(loc, "empty concatenation '{}' is illegal");
            return None;
        }
        if !own.contains(&",") && kids.len() == 2 && t.kind(kids[1]) == SyntaxKind::ConcatExpr {
            let is_zero = self.try_zero_replication(tree_idx, concat_node)?;
            if is_zero {
                return Some(());
            }
            let e = self.bind_self(tree_idx, concat_node)?;
            if self.arena.get(e).ty.integral_shape().is_none() {
                self.err(loc, "replicated operand must be integral/packed");
                return None;
            }
            return Some(());
        }
        for &k in &kids {
            if self.is_unbased_literal(tree_idx, k) {
                self.err(loc, "unsized unbased literal is illegal inside concatenation");
                return None;
            }
            if self.try_zero_replication(tree_idx, k)? {
                continue;
            }
            let e = self.bind_self(tree_idx, k)?;
            if self.arena.get(e).ty.integral_shape().is_none() {
                self.err(loc, "concatenation operand must be integral or packed");
                return None;
            }
        }
        Some(())
    }

    fn base_packed_bounds(&self, base_id: ExprId) -> (i128, i128) {
        let be = self.arena.get(base_id);
        match &be.ty {
            RtlType::PackedArray { left, right, .. } => (*left, *right),
            _ => {
                if let ExprKind::Symbol(sid) = be.kind {
                    if let Some(b) = self.design.symbol(sid).packed_bounds {
                        return b;
                    }
                }
                if let ExprKind::Local(lid) = be.kind {
                    for lb in self.locals.values() {
                        if lb.id == lid {
                            if let Some(b) = lb.packed_bounds {
                                return b;
                            }
                        }
                    }
                }
                let w = be.ty.bit_width().unwrap_or(1);
                ((w.saturating_sub(1)) as i128, 0)
            }
        }
    }

    fn bind_select(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        kids: &[NodeId],
        ctx: Option<(u32, bool, bool)>,
        loc: Loc,
    ) -> Option<ExprId> {
        let base_node = *kids.first()?;
        let base = self.bind_self(tree_idx, base_node)?;
        let base_ty = self.arena.get(base).ty.clone();
        let own = Self::own_toks(self.tree(tree_idx), node);

        // Unpacked array element index (`mem[idx]`).
        if let RtlType::UnpackedArray { elem, dims } = &base_ty {
            if own.iter().any(|tok| matches!(*tok, ":" | "+:" | "-:")) {
                self.unsup(loc, "unpacked array slice is not yet supported in RTL netlist lowering");
                return None;
            }
            let idx_node = *kids.get(1)?;
            let index = self.bind_self(tree_idx, idx_node)?;
            let dim = *dims.first().unwrap_or(&(0, 0));
            let res_ty = if dims.len() > 1 {
                RtlType::UnpackedArray { elem: elem.clone(), dims: dims[1..].to_vec() }
            } else {
                (**elem).clone()
            };
            let id = self.arena.alloc(ExprKind::ArrayIndex { base, index, dim }, res_ty, None, loc);
            return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
        }

        let Some((bw, _, four)) = base_ty.integral_shape() else {
            self.err(loc, "cannot bit-select or part-select non-packed value");
            return None;
        };
        let bounds = self.base_packed_bounds(base);
        let (elem_ty, elem_w) = match &base_ty {
            RtlType::PackedArray { elem, .. } => ((**elem).clone(), elem.bit_width().unwrap_or(1)),
            _ => (RtlType::Integral { width: 1, signed: false, four_state: four }, 1),
        };

        // Single bit-select or packed-array element select (`base[i]`).
        if !own.iter().any(|tok| matches!(*tok, ":" | "+:" | "-:")) {
            let idx_node = *kids.get(1)?;
            let index = self.bind_self(tree_idx, idx_node)?;
            let cval = match (&self.arena.get(base).cval, &self.arena.get(index).cval) {
                (Some(bc), Some(ic)) => {
                    if !ic.known() {
                        Some(if four {
                            CVal::all_x(elem_w, elem_ty.is_signed())
                        } else {
                            CVal::new(elem_w, elem_ty.is_signed(), 0)
                        })
                    } else {
                        match ic.v.to_i128().and_then(|idx| map_bit_offset(idx, bounds, elem_w, bw)) {
                            Some(bit_lo) => Some(bc.slice(bit_lo, elem_w, elem_ty.is_signed())),
                            None => Some(if four {
                                CVal::all_x(elem_w, elem_ty.is_signed())
                            } else {
                                CVal::new(elem_w, elem_ty.is_signed(), 0)
                            }),
                        }
                    }
                }
                _ => None,
            };
            let id =
                self.arena.alloc(ExprKind::BitSelect { base, index, bounds, elem_width: elem_w }, elem_ty, cval, loc);
            return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
        }

        // Constant part-select (`base[msb:lsb]`).
        if own.contains(&":") {
            let msb_expr = self.bind_self(tree_idx, *kids.get(1)?)?;
            let lsb_expr = self.bind_self(tree_idx, *kids.get(2)?)?;
            let (Some(mc), Some(lc)) = (&self.arena.get(msb_expr).cval, &self.arena.get(lsb_expr).cval) else {
                self.err(loc, "part-select bounds '[msb:lsb]' must be constant expressions (use '+:' or '-:' for variable part-selects)");
                return None;
            };
            if !mc.known() || !lc.known() {
                self.err(loc, "part-select bounds '[msb:lsb]' cannot contain x or z bits");
                return None;
            }
            let (Some(msb), Some(lsb)) = (mc.v.to_i128(), lc.v.to_i128()) else {
                self.err(loc, "part-select bounds '[msb:lsb]' exceed supported integer range");
                return None;
            };
            let Some(count) = range_len_u32(msb, lsb, MAX_BIT_WIDTH) else {
                self.err(loc, "part-select width exceeds maximum bit width");
                return None;
            };
            let Some(out_w) = count.checked_mul(elem_w).filter(|w| (1..=MAX_BIT_WIDTH).contains(w)) else {
                self.err(loc, "part-select bit width exceeds maximum limit");
                return None;
            };
            let cval =
                self.arena.get(base).cval.as_ref().map(|bc| slice_cval_bounds(bc, msb, lsb, bounds, elem_w, bw, four));
            let id = self.arena.alloc(
                ExprKind::PartSelect { base, msb, lsb, bounds, elem_width: elem_w },
                RtlType::Integral { width: out_w, signed: false, four_state: four },
                cval,
                loc,
            );
            return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
        }

        // Indexed part-select (`base[idx +: width]` or `base[idx -: width]`).
        let ascending = own.contains(&"+:");
        let base_idx = self.bind_self(tree_idx, *kids.get(1)?)?;
        let w_expr = self.bind_self(tree_idx, *kids.get(2)?)?;
        let Some(wc) = self.arena.get(w_expr).cval.clone() else {
            self.err(loc, "indexed part-select width must be a constant expression");
            return None;
        };
        let Some(count) = cval_pos_u32(&wc, MAX_BIT_WIDTH) else {
            self.err(loc, "indexed part-select width must be a positive constant within maximum bit width");
            return None;
        };
        let Some(out_w) = count.checked_mul(elem_w).filter(|w| (1..=MAX_BIT_WIDTH).contains(w)) else {
            self.err(loc, "indexed part-select bit width exceeds maximum limit");
            return None;
        };
        let cval = match (&self.arena.get(base).cval, &self.arena.get(base_idx).cval) {
            (Some(bc), Some(ic)) if ic.known() => {
                if let Some(start) = ic.v.to_i128() {
                    let span = (count as i128) - 1;
                    let end_opt = if ascending { start.checked_add(span) } else { start.checked_sub(span) };
                    end_opt.map(|end| {
                        let (msb, lsb) = if bounds.0 >= bounds.1 {
                            (start.max(end), start.min(end))
                        } else {
                            (start.min(end), start.max(end))
                        };
                        slice_cval_bounds(bc, msb, lsb, bounds, elem_w, bw, four)
                    })
                } else {
                    Some(if four { CVal::all_x(out_w, false) } else { CVal::new(out_w, false, 0) })
                }
            }
            _ => None,
        };
        let id = self.arena.alloc(
            ExprKind::IndexedPartSelect { base, base_idx, width: count, ascending, bounds, elem_width: elem_w },
            RtlType::Integral { width: out_w, signed: false, four_state: four },
            cval,
            loc,
        );
        Some(self.apply_ctx(id, ctx, ctx.is_some(), loc))
    }

    fn bind_member(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        kids: &[NodeId],
        ctx: Option<(u32, bool, bool)>,
        loc: Loc,
    ) -> Option<ExprId> {
        let t = self.tree(tree_idx);
        let own = Self::own_toks(t, node);
        let raw_field = *own.last()?;
        let field = raw_field.strip_prefix('\\').unwrap_or(raw_field);
        let base_node = *kids.first()?;

        // Check whether `base_node` is a local/instance/port aggregate FIRST before trying hierarchical path resolution!
        let saved_len = self.arena.exprs.len();
        let saved_diags = self.diags.len();
        let base_try = self.bind_node(tree_idx, base_node, None);
        if let Some(base_id) = base_try {
            let base_ty = self.arena.get(base_id).ty.clone();
            match &base_ty {
                RtlType::PackedStruct { .. } | RtlType::PackedUnion { .. } => {
                    let Some(pf) = base_ty.packed_field(field).cloned() else {
                        self.err(loc, format!("packed struct/union has no member '{field}'"));
                        return None;
                    };
                    let cval =
                        self.arena.get(base_id).cval.as_ref().map(|bc| bc.slice(pf.lo, pf.width, pf.ty.is_signed()));
                    let id = self.arena.alloc(
                        ExprKind::Member {
                            base: base_id,
                            field: field.to_string(),
                            packed_slice: Some((pf.lo, pf.width)),
                        },
                        pf.ty,
                        cval,
                        loc,
                    );
                    return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
                }
                RtlType::UnpackedStruct { fields, .. } => {
                    let Some((_, fty)) = fields.iter().find(|(n, _)| n == field).cloned() else {
                        self.err(loc, format!("unpacked struct/union has no member '{field}'"));
                        return None;
                    };
                    let id = self.arena.alloc(
                        ExprKind::Member { base: base_id, field: field.to_string(), packed_slice: None },
                        fty,
                        None,
                        loc,
                    );
                    return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
                }
                RtlType::Interface { .. } => {
                    if let ExprKind::Symbol(port_sid) = self.arena.get(base_id).kind {
                        if let Some((iface_inst, _mp)) =
                            resolve_interface_port(self.trees, self.design, self.inst_id, port_sid)
                        {
                            if let Some(target_sid) = self.design.instance(iface_inst).symbol(field) {
                                return self.finish_symbol_ref(target_sid, ctx, loc);
                            }
                            self.err(
                                loc,
                                format!(
                                    "interface '{}' has no member '{field}'",
                                    self.design.instance(iface_inst).definition
                                ),
                            );
                            return None;
                        }
                    }
                }
                RtlType::Enum { variants, .. } => {
                    // Built-in enum methods like `.first` / `.last` without parens:
                    match field {
                        "first" => {
                            if let Some((_, cv)) = variants.first() {
                                let id = self.arena.alloc(
                                    ExprKind::Const(cv.clone()),
                                    base_ty.clone(),
                                    Some(cv.clone()),
                                    loc,
                                );
                                return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
                            }
                        }
                        "last" => {
                            if let Some((_, cv)) = variants.last() {
                                let id = self.arena.alloc(
                                    ExprKind::Const(cv.clone()),
                                    base_ty.clone(),
                                    Some(cv.clone()),
                                    loc,
                                );
                                return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
                            }
                        }
                        "num" => {
                            let cv = CVal::new(32, true, variants.len() as i128);
                            let id = self.arena.alloc_const(cv, false, loc);
                            return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        // Roll back speculative `base_node` attempt and resolve as hierarchical / interface instance path (`u0.sig`, `g[0].x`, `bus_i.sig`).
        self.arena.exprs.truncate(saved_len);
        self.diags.truncate(saved_diags);

        if let Some(hier_path) = self.eval_hier_path(tree_idx, node) {
            if let Some(sid) = self.design.resolve_hierarchical(self.inst_id, self.scope_prefix, &hier_path) {
                return self.finish_symbol_ref(sid, ctx, loc);
            }
            // Also check enum constants in target child instance if `hier_path` is `<inst_path>.<enum_const>`.
            if let Some((inst_prefix, leaf)) = hier_path.rsplit_once('.') {
                if let Some(target_inst) = self.design.resolve_instance(self.inst_id, self.scope_prefix, inst_prefix) {
                    if let Some((cv, ty)) = self.design.instance(target_inst).resolve_enum("", leaf) {
                        let cv = cv.clone();
                        let ty = ty.clone();
                        let id = self.arena.alloc(ExprKind::Const(cv.clone()), ty, Some(cv), loc);
                        return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
                    }
                }
            }
        }

        self.err(
            loc,
            format!("cannot resolve hierarchical or member reference '{}'", self.tree(tree_idx).bare_text(node)),
        );
        None
    }

    /// Formats a hierarchical `MemberExpr` (`u0.g[i].sig`) evaluating constant generate-array indices `[i]`.
    fn eval_hier_path(&mut self, tree_idx: usize, node: NodeId) -> Option<String> {
        let t = self.tree(tree_idx);
        match t.kind(node) {
            SyntaxKind::NameExpr => {
                let raw = t.bare_text(node);
                Some(raw.trim().strip_prefix('\\').unwrap_or(raw.trim()).to_string())
            }
            SyntaxKind::MemberExpr => {
                let kids = Self::expr_kids(t, node);
                let prefix = self.eval_hier_path(tree_idx, *kids.first()?)?;
                let own = Self::own_toks(t, node);
                let leaf = own.last()?.strip_prefix('\\').unwrap_or(own.last()?);
                Some(format!("{prefix}.{leaf}"))
            }
            SyntaxKind::SelectExpr => {
                let kids = Self::expr_kids(t, node);
                let prefix = self.eval_hier_path(tree_idx, *kids.first()?)?;
                let idx_expr = self.bind_self(tree_idx, *kids.get(1)?)?;
                let idx_val = self.arena.get(idx_expr).cval.as_ref()?.i();
                Some(format!("{prefix}[{idx_val}]"))
            }
            _ => None,
        }
    }

    fn bind_cast(
        &mut self,
        tree_idx: usize,
        _node: NodeId,
        kids: &[NodeId],
        ctx: Option<(u32, bool, bool)>,
        loc: Loc,
    ) -> Option<ExprId> {
        let (target_node, inner_node) = (*kids.first()?, *kids.get(1)?);
        let t = self.tree(tree_idx);
        let target_text = t.bare_text(target_node);
        let trimmed = target_text.trim();

        // `signed'(x)` / `unsigned'(x)` / `const'(x)`
        if matches!(trimmed, "signed" | "unsigned" | "const") {
            let inner = self.bind_self(tree_idx, inner_node)?;
            if trimmed == "const" {
                return Some(self.apply_ctx(inner, ctx, ctx.is_some(), loc));
            }
            let Some((w, _, four)) = self.arena.get(inner).ty.integral_shape() else {
                self.err(loc, format!("operand of {trimmed}'(...) must be integral"));
                return None;
            };
            let id = self.arena.coerce(inner, w, trimmed == "signed", four, loc);
            return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
        }

        // Width cast `W'(x)` where `target_node` is a constant expression (Literal, NameExpr param, ParenExpr).
        if let Some(target_ty) = self.resolve_cast_data_type(tree_idx, target_node) {
            if let RtlType::NonSynthesizable(reason) = &target_ty {
                self.err(loc, format!("cast to non-synthesizable type '{reason}'"));
                return None;
            }
            let bound = self.bind_assigned(tree_idx, inner_node, &target_ty)?;
            // Preserve named `Enum` / `PackedStruct` type wrapper on the cast result!
            let cval = self.arena.get(bound).cval.clone();
            let out = if self.arena.get(bound).ty != target_ty && target_ty.is_packed() {
                let (tw, ts, t4) = target_ty.integral_shape().unwrap();
                let coerced = self.arena.coerce(bound, tw, ts, t4, loc);
                self.arena.exprs[coerced.0 as usize].ty = target_ty;
                coerced
            } else {
                let _ = cval;
                bound
            };
            return Some(self.apply_ctx(out, ctx, ctx.is_some(), loc));
        }

        // Otherwise evaluate `target_node` as a constant width expression `N'(x)`.
        let w_expr = self.bind_self(tree_idx, target_node)?;
        let Some(wc) = self.arena.get(w_expr).cval.clone() else {
            self.err(loc, format!("cast width '{trimmed}' must be a constant expression or known type"));
            return None;
        };
        let Some(target_w) = cval_pos_u32(&wc, MAX_BIT_WIDTH) else {
            self.err(loc, "cast width must be a positive constant within maximum bit width");
            return None;
        };
        let d0 = self.diags.len();
        let Some(inner_sh) = self.peek_self_shape(tree_idx, inner_node) else {
            if self.diags.len() == d0 {
                self.err(loc, "cast operand must be integral or packed");
            }
            return None;
        };
        let target_sh = (target_w, inner_sh.1, inner_sh.2);
        let bound = self.bind_assigned(
            tree_idx,
            inner_node,
            &RtlType::Integral { width: target_sh.0, signed: target_sh.1, four_state: target_sh.2 },
        )?;
        Some(self.apply_ctx(bound, ctx, ctx.is_some(), loc))
    }

    fn resolve_cast_data_type(&self, tree_idx: usize, node: NodeId) -> Option<RtlType> {
        let t = self.tree(tree_idx);
        let toks: Vec<&str> = t.token_range(node).into_iter().map(|ti| &*t.tokens[ti as usize].text).collect();
        match toks.as_slice() {
            ["logic"] | ["reg"] => Some(RtlType::Integral { width: 1, signed: false, four_state: true }),
            ["bit"] => Some(RtlType::Integral { width: 1, signed: false, four_state: false }),
            ["byte"] => Some(RtlType::Integral { width: 8, signed: true, four_state: false }),
            ["byte", "unsigned"] => Some(RtlType::Integral { width: 8, signed: false, four_state: false }),
            ["shortint"] => Some(RtlType::Integral { width: 16, signed: true, four_state: false }),
            ["shortint", "unsigned"] => Some(RtlType::Integral { width: 16, signed: false, four_state: false }),
            ["int"] => Some(RtlType::Integral { width: 32, signed: true, four_state: false }),
            ["int", "unsigned"] => Some(RtlType::Integral { width: 32, signed: false, four_state: false }),
            ["integer"] => Some(RtlType::Integral { width: 32, signed: true, four_state: true }),
            ["integer", "unsigned"] => Some(RtlType::Integral { width: 32, signed: false, four_state: true }),
            ["longint"] => Some(RtlType::Integral { width: 64, signed: true, four_state: false }),
            ["longint", "unsigned"] => Some(RtlType::Integral { width: 64, signed: false, four_state: false }),
            ["real"] | ["shortreal"] | ["realtime"] | ["string"] => {
                Some(RtlType::NonSynthesizable(toks[0].to_string()))
            }
            [pkg, "::", tname] => {
                self.design.resolve_named_type(self.inst_id, self.scope_prefix, Some(pkg), tname).cloned()
            }
            [tname] => {
                if let Some(ty) = self.local_typedefs.get(*tname) {
                    return Some(ty.clone());
                }
                self.design.resolve_named_type(self.inst_id, self.scope_prefix, None, tname).cloned()
            }
            _ => None,
        }
    }

    fn bind_call(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        kids: &[NodeId],
        ctx: Option<(u32, bool, bool)>,
        loc: Loc,
    ) -> Option<ExprId> {
        let callee_node = *kids.first()?;
        let t = self.tree(tree_idx);
        let al = t.child(node, SyntaxKind::ArgList);
        let slots = parse_arg_list_slots(t, al);

        // Method call on an enum (`e.first()`, `e.last()`, `e.num()`).
        if t.kind(callee_node) == SyntaxKind::MemberExpr {
            let mkids = Self::expr_kids(t, callee_node);
            let mname = *Self::own_toks(t, callee_node).last()?;
            if slots.is_empty() && matches!(mname, "first" | "last" | "num") {
                return self.bind_member(tree_idx, callee_node, &mkids, ctx, loc);
            }
        }

        let (pkg, callee_name) = match t.kind(callee_node) {
            SyntaxKind::NameExpr => {
                let raw = t.bare_text(callee_node);
                (None, raw.trim().strip_prefix('\\').unwrap_or(raw.trim()).to_string())
            }
            SyntaxKind::ScopedExpr => {
                let toks: Vec<&str> =
                    t.token_range(callee_node).into_iter().map(|ti| &*t.tokens[ti as usize].text).collect();
                if let [p, "::", nm] = toks.as_slice() {
                    (Some((*p).to_string()), nm.strip_prefix('\\').unwrap_or(nm).to_string())
                } else {
                    self.err(loc, format!("unsupported call target '{}'", t.bare_text(callee_node)));
                    return None;
                }
            }
            _ => {
                self.err(loc, format!("unsupported call target '{}'", t.bare_text(callee_node)));
                return None;
            }
        };

        if callee_name.starts_with('$') {
            return self.bind_syscall(tree_idx, &callee_name, &slots, ctx, loc);
        }

        // Parameterized `let` call (`let clamp(x) = ...;`) before function lookup.
        if let Some((lt_tree, lt_node, lt_inst, lt_prefix, lt_consts)) =
            self.resolve_let_decl(pkg.as_deref(), &callee_name)
        {
            return self.expand_let_decl(lt_tree, lt_node, lt_inst, &lt_prefix, &lt_consts, tree_idx, al, ctx, loc);
        }

        // User-defined `function` call (in current instance/generate scope, package import, or `$unit`).
        let resolved = self.resolve_function(pkg.as_deref(), &callee_name);
        let Some((fn_tree, fn_node, fn_inst, fn_scope_prefix)) = resolved else {
            self.err(loc, format!("undeclared function '{callee_name}'"));
            return None;
        };

        // Determine return type and formal port types of `fn_node`.
        let formals = extract_subroutine_formals(self.tree(fn_tree), fn_node);
        let formal_names: Vec<&str> = formals.iter().map(|f| f.name.as_str()).collect();
        let callee_desc = format!("function '{callee_name}'");
        let mapped = resolve_call_arg_map(&slots, &formal_names, &callee_desc, loc, &mut self.diags)?;

        let mut bound_args = Vec::with_capacity(formals.len());
        for (i, formal) in formals.iter().enumerate() {
            let fty = self.resolve_subroutine_dtype(fn_tree, formal.dtype, formal.decl, fn_inst, &fn_scope_prefix)?;
            if let Some(arg_n) = mapped[i] {
                let b = self.bind_assigned(tree_idx, arg_n, &fty)?;
                bound_args.push(b);
            } else if let Some(def_n) = formal.default_expr {
                let b = self.bind_assigned(fn_tree, def_n, &fty)?;
                bound_args.push(b);
            } else {
                self.err(loc, format!("missing argument '{}' in call to '{callee_name}'", formal.name));
                return None;
            }
        }

        let ret_ty = self.resolve_function_return_type(fn_tree, fn_node, fn_inst, &fn_scope_prefix)?;
        let id = self.arena.alloc(
            ExprKind::FuncCall {
                callee: (fn_tree, fn_node),
                callee_inst: fn_inst,
                callee_scope_prefix: fn_scope_prefix,
                name: callee_name,
                args: bound_args,
            },
            ret_ty,
            None,
            loc,
        );
        Some(self.apply_ctx(id, ctx, ctx.is_some(), loc))
    }

    fn bind_clock_arg(&mut self, tree_idx: usize, c_node: NodeId) -> Option<(ExprId, Option<bool>)> {
        let t = self.tree(tree_idx);
        if t.kind(c_node) == SyntaxKind::TimingControl {
            if let Some(ev) = t.children(c_node).find(|&k| t.kind(k) == SyntaxKind::EventExpr) {
                let inner = t.child(ev, SyntaxKind::EventExpr).unwrap_or(ev);
                let own = Self::own_toks(t, inner);
                let posedge = if own.contains(&"posedge") {
                    Some(true)
                } else if own.contains(&"negedge") {
                    Some(false)
                } else {
                    None
                };
                let sig = *Self::expr_kids(t, inner).first()?;
                return Some((self.bind_self(tree_idx, sig)?, posedge));
            }
            if let Some(ne) = t.child(c_node, SyntaxKind::NameExpr) {
                return Some((self.bind_self(tree_idx, ne)?, None));
            }
        }
        Some((self.bind_self(tree_idx, c_node)?, None))
    }

    fn resolve_function(&self, pkg: Option<&str>, name: &str) -> Option<(usize, NodeId, Option<InstanceId>, String)> {
        match self.design.resolve_subroutine(self.inst_id, self.scope_prefix, pkg, name) {
            Some((ElabItemKind::FunctionDecl, tr, nd, inst_opt, sp)) => Some((tr, nd, inst_opt, sp)),
            _ => None,
        }
    }

    #[allow(clippy::type_complexity)]
    fn resolve_let_decl(
        &self,
        pkg: Option<&str>,
        name: &str,
    ) -> Option<(usize, NodeId, InstanceId, String, Vec<(String, CVal)>)> {
        let decl_matches = |t: &Tree, nd: NodeId| -> bool {
            let Some(nm_n) = t.child(nd, SyntaxKind::Name) else { return false };
            let Some(&last) = t.token_range(nm_n).last() else { return false };
            let raw = &*t.tokens[last as usize].text;
            raw.strip_prefix('\\').unwrap_or(raw) == name
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
        if let Some(pkg_name) = pkg {
            let p = if pkg_name == "$unit" { &self.design.unit_scope } else { self.design.packages.get(pkg_name)? };
            for &(ref nm, k, tr, nd, _) in &p.subroutines {
                if k == ElabItemKind::LetDecl && nm == name {
                    return Some((tr, nd, self.inst_id, String::new(), pkg_consts_of(p)));
                }
            }
            return None;
        }
        let inst = self.design.instance(self.inst_id);
        let mut prefix = self.scope_prefix;
        loop {
            for item in &inst.items {
                if item.kind == ElabItemKind::LetDecl
                    && item.scope_prefix == prefix
                    && decl_matches(self.tree(item.tree), item.node)
                {
                    return Some((
                        item.tree,
                        item.node,
                        self.inst_id,
                        item.scope_prefix.clone(),
                        item.gen_consts.clone(),
                    ));
                }
            }
            for (sp, imp_pkg, imp_item) in &inst.imports {
                if sp == prefix && imp_item.as_deref().is_none_or(|x| x == name) {
                    if let Some(p) = self.design.packages.get(imp_pkg) {
                        for &(ref nm, k, tr, nd, _) in &p.subroutines {
                            if k == ElabItemKind::LetDecl && nm == name {
                                return Some((tr, nd, self.inst_id, String::new(), pkg_consts_of(p)));
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
        for &(ref nm, k, tr, nd, _) in &self.design.unit_scope.subroutines {
            if k == ElabItemKind::LetDecl && nm == name {
                return Some((tr, nd, self.inst_id, String::new(), pkg_consts_of(&self.design.unit_scope)));
            }
        }
        for (imp_pkg, imp_item) in &self.design.unit_scope.imports {
            if imp_item.as_deref().is_none_or(|x| x == name) {
                if let Some(p) = self.design.packages.get(imp_pkg) {
                    for &(ref nm, k, tr, nd, _) in &p.subroutines {
                        if k == ElabItemKind::LetDecl && nm == name {
                            return Some((tr, nd, self.inst_id, String::new(), pkg_consts_of(p)));
                        }
                    }
                }
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn expand_let_decl(
        &mut self,
        lt_tree: usize,
        lt_node: NodeId,
        lt_inst: InstanceId,
        lt_prefix: &str,
        lt_consts: &[(String, CVal)],
        call_tree: usize,
        arg_list: Option<NodeId>,
        ctx: Option<(u32, bool, bool)>,
        loc: Loc,
    ) -> Option<ExprId> {
        if self.let_depth >= 32 {
            self.err(loc, "recursive 'let' expansion exceeds maximum depth (32)");
            return None;
        }
        let dt = self.tree(lt_tree);
        // Extract formals `(name, optional_dtype, decl, default_expr)` from `PortList` if present.
        let mut formals: Vec<(String, Option<NodeId>, NodeId, Option<NodeId>)> = Vec::new();
        if let Some(pl) = dt.child(lt_node, SyntaxKind::PortList) {
            let mut cur_dt: Option<NodeId> = None;
            for p in dt.children(pl).filter(|&c| dt.kind(c) == SyntaxKind::Port) {
                if let Some(pdt) = dt.child(p, SyntaxKind::DataType) {
                    let is_untyped =
                        dt.token_range(pdt).into_iter().all(|ti| &*dt.tokens[ti as usize].text == "untyped");
                    cur_dt = if is_untyped { None } else { Some(pdt) };
                }
                for d in dt.children(p).filter(|&c| dt.kind(c) == SyntaxKind::Declarator) {
                    let Some(nm_n) = dt.child(d, SyntaxKind::Name) else { continue };
                    let Some(&last) = dt.token_range(nm_n).last() else { continue };
                    let raw = &*dt.tokens[last as usize].text;
                    let fname = raw.strip_prefix('\\').unwrap_or(raw).to_string();
                    let def = dt
                        .children(d)
                        .find(|&c| !matches!(dt.kind(c), SyntaxKind::Name | SyntaxKind::Dim | SyntaxKind::Attribute));
                    formals.push((fname, cur_dt, d, def));
                }
            }
        }

        let slots = parse_arg_list_slots(self.tree(call_tree), arg_list);
        let formal_names: Vec<&str> = formals.iter().map(|(n, _, _, _)| n.as_str()).collect();
        let mapped = resolve_call_arg_map(&slots, &formal_names, "'let' construct", loc, &mut self.diags)?;

        let mut substs = FxMap::default();
        for (pos, (fname, fdt, fdecl, def_node)) in formals.iter().enumerate() {
            let formal_ty = match *fdt {
                Some(dt_node) => Some(self.resolve_subroutine_dtype(
                    lt_tree,
                    Some(dt_node),
                    Some(*fdecl),
                    Some(lt_inst),
                    lt_prefix,
                )?),
                None => None,
            };
            if let Some(act_e) = mapped[pos] {
                let bound = match &formal_ty {
                    Some(fty) => self.bind_assigned(call_tree, act_e, fty)?,
                    None => self.bind_untyped_actual(call_tree, act_e)?,
                };
                substs.insert(fname.clone(), bound);
            } else if let Some(def_e) = *def_node {
                let mut def_binder =
                    ExprBinder::new(self.trees, self.design, lt_inst, lt_prefix, lt_consts, self.arena);
                def_binder.extra_tree = self.extra_tree;
                def_binder.let_depth = self.let_depth + 1;
                let bound_opt = match &formal_ty {
                    Some(fty) => def_binder.bind_assigned(lt_tree, def_e, fty),
                    None => def_binder.bind_untyped_actual(lt_tree, def_e),
                };
                self.diags.extend(def_binder.diags);
                substs.insert(fname.clone(), bound_opt?);
            } else {
                self.err(loc, format!("missing actual argument for 'let' formal '{fname}'"));
                return None;
            }
        }

        let body_expr = Self::expr_kids(self.tree(lt_tree), lt_node)
            .into_iter()
            .rfind(|&c| !matches!(self.tree(lt_tree).kind(c), SyntaxKind::Name | SyntaxKind::PortList))?;
        let mut let_binder = ExprBinder::new(self.trees, self.design, lt_inst, lt_prefix, lt_consts, self.arena);
        let_binder.extra_tree = self.extra_tree;
        let_binder.expr_substs = substs;
        let_binder.let_depth = self.let_depth + 1;
        let res = let_binder.bind_node(lt_tree, body_expr, ctx);
        self.diags.extend(let_binder.diags);
        let id = res?;
        Some(self.apply_ctx(id, ctx, ctx.is_some(), loc))
    }

    /// Resolves the [`RtlType`] of a subroutine formal port or local variable declaration,
    /// combining an optional `DataType` node with any unpacked `Dim` nodes on `decl`.
    ///
    /// Returns `None` and records an [`ExprDiag`] in `self.diags` when the type is unknown,
    /// non-synthesizable, or has invalid or out-of-bounds dimensions.
    pub fn resolve_subroutine_dtype(
        &mut self,
        tree_idx: usize,
        dtype: Option<NodeId>,
        decl: Option<NodeId>,
        owner_inst: Option<InstanceId>,
        owner_prefix: &str,
    ) -> Option<RtlType> {
        let base = match dtype {
            None => RtlType::Integral { width: 1, signed: false, four_state: true },
            Some(dt) => self.eval_simple_dtype(tree_idx, dt, owner_inst, owner_prefix)?,
        };
        if let Some(d) = decl {
            let t = self.tree(tree_idx);
            let udims: Vec<NodeId> = t.children(d).filter(|&c| t.kind(c) == SyntaxKind::Dim).collect();
            if !udims.is_empty() {
                let mut bounds = Vec::with_capacity(udims.len());
                let mut total_elems: u32 = 1;
                for dim in udims {
                    let dloc = self.loc_of(tree_idx, dim);
                    let Some((l, r)) = self.eval_dim_bounds(tree_idx, dim) else {
                        self.err(dloc, "unpacked array dimension requires positive constant bounds");
                        return None;
                    };
                    let Some(len) = range_len_u32(l, r, MAX_ARRAY_ELEMS) else {
                        self.err(dloc, "unpacked array dimension exceeds maximum element count");
                        return None;
                    };
                    let Some(next_elems) = total_elems.checked_mul(len).filter(|&n| n <= MAX_ARRAY_ELEMS) else {
                        self.err(dloc, "unpacked array exceeds maximum element count");
                        return None;
                    };
                    total_elems = next_elems;
                    bounds.push((l, r));
                }
                return Some(RtlType::UnpackedArray { elem: Box::new(base), dims: bounds });
            }
        }
        Some(base)
    }

    /// Resolves the return [`RtlType`] of a `FunctionDecl` CST node.
    ///
    /// Returns `None` and records an [`ExprDiag`] in `self.diags` when the return type is unknown,
    /// non-synthesizable, or has invalid dimensions.
    pub fn resolve_function_return_type(
        &mut self,
        tree_idx: usize,
        fn_node: NodeId,
        owner_inst: Option<InstanceId>,
        owner_prefix: &str,
    ) -> Option<RtlType> {
        let t = self.tree(tree_idx);
        let Some(rt) = t.child(fn_node, SyntaxKind::ReturnType) else {
            return Some(RtlType::Integral { width: 1, signed: false, four_state: true });
        };
        if t.bare_text(rt).trim() == "void" {
            return Some(RtlType::Void);
        }
        match t.child(rt, SyntaxKind::DataType) {
            Some(dt) => self.eval_simple_dtype(tree_idx, dt, owner_inst, owner_prefix),
            None => {
                let signed = Self::own_toks(t, rt).contains(&"signed");
                let dims: Vec<NodeId> = t.children(rt).filter(|&c| t.kind(c) == SyntaxKind::Dim).collect();
                let mut w = 1u32;
                for d in dims {
                    let dloc = self.loc_of(tree_idx, d);
                    let Some((l, r)) = self.eval_dim_bounds(tree_idx, d) else {
                        self.err(dloc, "packed return dimension requires constant bounds");
                        return None;
                    };
                    let Some(cnt) = range_len_u32(l, r, MAX_BIT_WIDTH) else {
                        self.err(dloc, "packed return dimension exceeds maximum bit width");
                        return None;
                    };
                    let Some(next_w) = w.checked_mul(cnt).filter(|&n| n <= MAX_BIT_WIDTH) else {
                        self.err(dloc, "packed return type exceeds maximum bit width");
                        return None;
                    };
                    w = next_w;
                }
                Some(RtlType::Integral { width: w, signed, four_state: true })
            }
        }
    }

    /// Evaluates a CST `DataType` node inside `(owner_inst, owner_prefix)` into an [`RtlType`],
    /// supporting built-in integer/vector keywords, inline packed and unpacked `struct`/`union`
    /// types, and scoped or lexical `typedef`s.
    ///
    /// Returns `None` and records an [`ExprDiag`] in `self.diags` on unknown, invalid, or
    /// non-synthesizable types instead of falling back to 1-bit `logic`.
    pub fn eval_simple_dtype(
        &mut self,
        tree_idx: usize,
        dt: NodeId,
        owner_inst: Option<InstanceId>,
        owner_prefix: &str,
    ) -> Option<RtlType> {
        let loc = self.loc_of(tree_idx, dt);
        let t = self.tree(tree_idx);
        let own = Self::own_toks(t, dt);
        if own.first() == Some(&"void") {
            return Some(RtlType::Void);
        }
        let pdims: Vec<NodeId> = t.children(dt).filter(|&c| t.kind(c) == SyntaxKind::Dim).collect();

        // Inline `struct` / `union` (packed or unpacked).
        if own.contains(&"struct") || own.contains(&"union") {
            let is_union = own.contains(&"union");
            let is_packed = own.contains(&"packed");
            let signed = own.contains(&"signed") && !own.contains(&"unsigned");
            let members: Vec<NodeId> = t.children(dt).filter(|&c| t.kind(c) == SyntaxKind::DataDecl).collect();
            if is_packed {
                let mut raw_members: Vec<(String, RtlType, u32)> = Vec::new();
                for m in members {
                    let mdt = self.tree(tree_idx).child(m, SyntaxKind::DataType);
                    let decls: Vec<NodeId> = self
                        .tree(tree_idx)
                        .children(m)
                        .filter(|&c| self.tree(tree_idx).kind(c) == SyntaxKind::Declarator)
                        .collect();
                    for d in decls {
                        let Some(nm_n) = self.tree(tree_idx).child(d, SyntaxKind::Name) else { continue };
                        let Some(&last) = self.tree(tree_idx).token_range(nm_n).last() else { continue };
                        let raw = &*self.tree(tree_idx).tokens[last as usize].text;
                        let fname = raw.strip_prefix('\\').unwrap_or(raw).to_string();
                        let fty = self.resolve_subroutine_dtype(tree_idx, mdt, Some(d), owner_inst, owner_prefix)?;
                        let Some(fw) = fty.bit_width() else {
                            self.err(loc, format!("member '{fname}' inside packed struct/union must be packed"));
                            return None;
                        };
                        raw_members.push((fname, fty, fw));
                    }
                }
                let total = if is_union {
                    raw_members.iter().map(|(_, _, w)| *w).max().unwrap_or(1).max(1)
                } else {
                    let mut sum = 0u32;
                    for (_, _, w) in &raw_members {
                        let Some(next) = sum.checked_add(*w).filter(|&n| n <= MAX_BIT_WIDTH) else {
                            self.err(loc, "packed struct exceeds maximum bit width");
                            return None;
                        };
                        sum = next;
                    }
                    sum.max(1)
                };
                let four_state = raw_members.iter().any(|(_, ft, _)| ft.is_four_state());
                let mut cursor = total;
                let fields: Vec<PackedField> = raw_members
                    .into_iter()
                    .map(|(fname, fty, w)| {
                        let lo = if is_union {
                            0
                        } else {
                            cursor = cursor.saturating_sub(w);
                            cursor
                        };
                        PackedField { name: fname, lo, width: w, ty: fty }
                    })
                    .collect();
                let sty = if is_union {
                    RtlType::PackedUnion { name: None, width: total, signed, four_state, fields }
                } else {
                    RtlType::PackedStruct { name: None, width: total, signed, four_state, fields }
                };
                return self.wrap_dtype_packed_dims(tree_idx, sty, &pdims);
            }
            if !pdims.is_empty() {
                self.err(loc, "packed dimensions are illegal on an unpacked struct or union");
                return None;
            }
            let mut fields = Vec::new();
            for m in members {
                let mdt = self.tree(tree_idx).child(m, SyntaxKind::DataType);
                let decls: Vec<NodeId> = self
                    .tree(tree_idx)
                    .children(m)
                    .filter(|&c| self.tree(tree_idx).kind(c) == SyntaxKind::Declarator)
                    .collect();
                for d in decls {
                    let Some(nm_n) = self.tree(tree_idx).child(d, SyntaxKind::Name) else { continue };
                    let Some(&last) = self.tree(tree_idx).token_range(nm_n).last() else { continue };
                    let raw = &*self.tree(tree_idx).tokens[last as usize].text;
                    let fname = raw.strip_prefix('\\').unwrap_or(raw).to_string();
                    let fty = self.resolve_subroutine_dtype(tree_idx, mdt, Some(d), owner_inst, owner_prefix)?;
                    fields.push((fname, fty));
                }
            }
            return Some(RtlType::UnpackedStruct { name: None, is_union, fields });
        }

        if let Some(&bad) =
            own.iter().find(|tok| matches!(**tok, "real" | "shortreal" | "realtime" | "string" | "chandle" | "event"))
        {
            self.err(loc, format!("non-synthesizable type '{bad}'"));
            return None;
        }

        let kw = own
            .iter()
            .find(|tok| {
                matches!(**tok, "logic" | "reg" | "bit" | "int" | "integer" | "byte" | "shortint" | "longint" | "time")
            })
            .copied();
        let head: Vec<&str> =
            t.token_range(dt).into_iter().map(|ti| &*t.tokens[ti as usize].text).take_while(|x| *x != "[").collect();
        let implicit = kw.is_none()
            && head.iter().all(|x| matches!(*x, "signed" | "unsigned" | "var" | "automatic" | "static"))
            && (!head.is_empty() || !pdims.is_empty());
        if kw.is_some() || implicit {
            let four_state = !matches!(kw, Some("bit" | "int" | "byte" | "shortint" | "longint"));
            let def_signed = matches!(kw, Some("int" | "integer" | "byte" | "shortint" | "longint"));
            let signed = if own.contains(&"unsigned") {
                false
            } else if own.contains(&"signed") {
                true
            } else {
                def_signed
            };
            let w: u32 = match kw {
                Some("int" | "integer") => 32,
                Some("byte") => 8,
                Some("shortint") => 16,
                Some("longint" | "time") => 64,
                _ => 1,
            };
            if pdims.is_empty() {
                return Some(RtlType::Integral { width: w, signed, four_state });
            }
            if w == 1 && pdims.len() == 1 {
                let dloc = self.loc_of(tree_idx, pdims[0]);
                let Some((l, r)) = self.eval_dim_bounds(tree_idx, pdims[0]) else {
                    self.err(dloc, "packed dimension requires constant bounds");
                    return None;
                };
                let Some(width) = range_len_u32(l, r, MAX_BIT_WIDTH) else {
                    self.err(dloc, "packed vector exceeds maximum bit width");
                    return None;
                };
                return Some(RtlType::Integral { width, signed, four_state });
            }
            let (inner, outer_dims) = if w == 1 {
                let last = pdims[pdims.len() - 1];
                let dloc = self.loc_of(tree_idx, last);
                let Some((l, r)) = self.eval_dim_bounds(tree_idx, last) else {
                    self.err(dloc, "packed dimension requires constant bounds");
                    return None;
                };
                let Some(iw) = range_len_u32(l, r, MAX_BIT_WIDTH) else {
                    self.err(dloc, "packed vector exceeds maximum bit width");
                    return None;
                };
                (RtlType::Integral { width: iw, signed: false, four_state }, &pdims[..pdims.len() - 1])
            } else {
                (RtlType::Integral { width: w, signed: false, four_state }, pdims.as_slice())
            };
            let mut wrapped = self.wrap_dtype_packed_dims(tree_idx, inner, outer_dims)?;
            if signed {
                if let RtlType::PackedArray { signed: s, .. } = &mut wrapped {
                    *s = true;
                }
            }
            return Some(wrapped);
        }

        let inst = owner_inst.unwrap_or(self.inst_id);
        let base_opt = match head.as_slice() {
            [pkg, "::", nm] => self.design.resolve_named_type(inst, owner_prefix, Some(pkg), nm).cloned(),
            [nm] => self
                .local_typedefs
                .get(*nm)
                .cloned()
                .or_else(|| self.design.resolve_named_type(inst, owner_prefix, None, nm).cloned()),
            _ => None,
        };
        let Some(base_ty) = base_opt else {
            self.err(loc, format!("unknown or unsupported type '{}'", self.tree(tree_idx).bare_text(dt).trim()));
            return None;
        };
        if let RtlType::NonSynthesizable(reason) = &base_ty {
            self.err(loc, format!("non-synthesizable type '{reason}'"));
            return None;
        }
        self.wrap_dtype_packed_dims(tree_idx, base_ty, &pdims)
    }

    fn wrap_dtype_packed_dims(&mut self, tree_idx: usize, mut cur: RtlType, dims: &[NodeId]) -> Option<RtlType> {
        if dims.is_empty() {
            return Some(cur);
        }
        let loc = self.loc_of(tree_idx, dims[0]);
        if !cur.is_packed() {
            self.err(loc, "packed dimensions require a packed element type");
            return None;
        }
        for &d in dims.iter().rev() {
            let dloc = self.loc_of(tree_idx, d);
            let Some((left, right)) = self.eval_dim_bounds(tree_idx, d) else {
                self.err(dloc, "packed dimension requires constant bounds");
                return None;
            };
            let Some(cnt) = range_len_u32(left, right, MAX_ARRAY_ELEMS) else {
                self.err(dloc, "packed array dimension exceeds maximum element count");
                return None;
            };
            let elem_w = cur.bit_width().unwrap_or(1);
            let Some(width) = elem_w.checked_mul(cnt).filter(|w| (1..=MAX_BIT_WIDTH).contains(w)) else {
                self.err(dloc, "packed array exceeds maximum bit width");
                return None;
            };
            let four_state = cur.is_four_state();
            cur = RtlType::PackedArray { elem: Box::new(cur), left, right, width, signed: false, four_state };
        }
        Some(cur)
    }

    /// Evaluates a CST `Dim` node (`[msb:lsb]` or unpacked size `[N]`) to constant `(left, right)` bounds.
    ///
    /// Returns `None` when the dimension is unsized, non-constant, contains `x`/`z` bits, or has a
    /// non-positive single-number size.
    pub fn eval_dim_bounds(&mut self, tree_idx: usize, dim: NodeId) -> Option<(i128, i128)> {
        let kids = Self::expr_kids(self.tree(tree_idx), dim);
        match kids.as_slice() {
            [a, b] => {
                let ea = self.bind_self(tree_idx, *a)?;
                let eb = self.bind_self(tree_idx, *b)?;
                let ca = self.arena.get(ea).cval.as_ref().filter(|c| c.known())?;
                let cb = self.arena.get(eb).cval.as_ref().filter(|c| c.known())?;
                Some((ca.v.to_i128()?, cb.v.to_i128()?))
            }
            [n] => {
                let en = self.bind_self(tree_idx, *n)?;
                let cn = self.arena.get(en).cval.as_ref().filter(|c| c.known())?;
                let sz = cn.v.to_i128()?;
                (sz > 0).then_some((0, sz - 1))
            }
            _ => None,
        }
    }

    fn bind_syscall(
        &mut self,
        tree_idx: usize,
        name: &str,
        slots: &[CallArgSlot],
        ctx: Option<(u32, bool, bool)>,
        loc: Loc,
    ) -> Option<ExprId> {
        for slot in slots {
            match slot {
                CallArgSlot::Named { .. } | CallArgSlot::Wildcard => {
                    self.err(loc, format!("named or wildcard arguments are illegal in call to {name}"));
                    return None;
                }
                CallArgSlot::EmptyPositional if name != "$past" => {
                    self.err(loc, format!("empty argument is illegal in call to {name}"));
                    return None;
                }
                _ => {}
            }
        }
        let pos_arg = |i: usize| -> Option<NodeId> {
            match slots.get(i)? {
                CallArgSlot::Positional(n) => Some(*n),
                _ => None,
            }
        };
        match name {
            "$signed" | "$unsigned" => {
                if slots.len() != 1 {
                    self.err(loc, format!("{name} expects 1 argument, got {}", slots.len()));
                    return None;
                }
                let a0 = pos_arg(0)?;
                let inner = self.bind_self(tree_idx, a0)?;
                let Some((w, _, four)) = self.arena.get(inner).ty.integral_shape() else {
                    self.err(loc, format!("argument of {name} must be integral"));
                    return None;
                };
                let signed = name == "$signed";
                let coerced = self.arena.coerce(inner, w, signed, four, loc);
                Some(self.apply_ctx(coerced, ctx, ctx.is_some(), loc))
            }
            "$clog2" => {
                if slots.len() != 1 {
                    self.err(loc, format!("$clog2 expects 1 argument, got {}", slots.len()));
                    return None;
                }
                let a0 = pos_arg(0)?;
                let inner = self.bind_self(tree_idx, a0)?;
                if let Some(cv) = self.arena.get(inner).cval.clone() {
                    let val = eval_clog2(&cv);
                    let id = self.arena.alloc_const(val, true, loc);
                    return Some(self.apply_ctx(id, ctx, ctx.is_some(), loc));
                }
                let id = self.arena.alloc(
                    ExprKind::SysCall { func: SysFunc::Clog2, args: vec![inner] },
                    RtlType::Integral { width: 32, signed: true, four_state: true },
                    None,
                    loc,
                );
                Some(self.apply_ctx(id, ctx, ctx.is_some(), loc))
            }
            "$bits" => {
                if slots.len() != 1 {
                    self.err(loc, format!("$bits expects 1 argument, got {}", slots.len()));
                    return None;
                }
                let a0 = pos_arg(0)?;
                let bits = if let Some(ty) = self.resolve_cast_data_type(tree_idx, a0) {
                    total_type_bits(&ty)
                } else {
                    let inner = self.bind_self(tree_idx, a0)?;
                    total_type_bits(&self.arena.get(inner).ty)
                };
                let Some(bits) = bits else {
                    self.err(loc, "cannot compute $bits of unsized/non-synthesizable argument");
                    return None;
                };
                let id = self.arena.alloc_const(CVal::new(32, true, bits as i128), true, loc);
                Some(self.apply_ctx(id, ctx, ctx.is_some(), loc))
            }
            "$size" | "$high" | "$low" | "$left" | "$right" | "$increment" => {
                if slots.is_empty() || slots.len() > 2 {
                    self.err(loc, format!("{name} expects 1 or 2 arguments, got {}", slots.len()));
                    return None;
                }
                let a0 = pos_arg(0)?;
                let dim_num: usize = if let Some(dnode) = pos_arg(1) {
                    let de = self.bind_self(tree_idx, dnode)?;
                    let Some(dc) = self.arena.get(de).cval.clone() else {
                        self.err(loc, format!("{name} dimension argument must be a constant expression"));
                        return None;
                    };
                    let Some(d) = cval_pos_u32(&dc, MAX_ARRAY_ELEMS) else {
                        self.err(loc, format!("{name} dimension argument must be a positive integer"));
                        return None;
                    };
                    d as usize
                } else {
                    1
                };
                let bounds_opt = if let Some(ty) = self.resolve_cast_data_type(tree_idx, a0) {
                    query_type_bounds(&ty, None, dim_num)
                } else {
                    let inner = self.bind_self(tree_idx, a0)?;
                    let pb = Some(self.base_packed_bounds(inner));
                    query_type_bounds(&self.arena.get(inner).ty, pb, dim_num)
                };
                let Some((left, right)) = bounds_opt else {
                    self.err(loc, format!("dimension {dim_num} is out of range for {name}"));
                    return None;
                };
                let v = match name {
                    "$left" => left,
                    "$right" => right,
                    "$low" => left.min(right),
                    "$high" => left.max(right),
                    "$increment" => {
                        if left >= right {
                            1
                        } else {
                            -1
                        }
                    }
                    _ => {
                        let Some(sz) = left.abs_diff(right).checked_add(1).and_then(|u| i128::try_from(u).ok()) else {
                            self.err(loc, format!("{name} dimension size overflows integer range"));
                            return None;
                        };
                        sz
                    }
                };
                let id = self.arena.alloc_const(CVal::new(32, true, v), true, loc);
                Some(self.apply_ctx(id, ctx, ctx.is_some(), loc))
            }
            "$countones" | "$onehot" | "$onehot0" | "$isunknown" => {
                if slots.len() != 1 {
                    self.err(loc, format!("{name} expects 1 argument, got {}", slots.len()));
                    return None;
                }
                let a0 = pos_arg(0)?;
                let inner = self.bind_self(tree_idx, a0)?;
                if self.arena.get(inner).ty.integral_shape().is_none() {
                    self.err(loc, format!("argument of {name} must be integral/packed"));
                    return None;
                };
                let (func, ret_ty) = match name {
                    "$countones" => {
                        (SysFunc::CountOnes, RtlType::Integral { width: 32, signed: true, four_state: false })
                    }
                    "$onehot" => (SysFunc::OneHot, RtlType::Integral { width: 1, signed: false, four_state: false }),
                    "$onehot0" => (SysFunc::OneHot0, RtlType::Integral { width: 1, signed: false, four_state: false }),
                    _ => (SysFunc::IsUnknown, RtlType::Integral { width: 1, signed: false, four_state: false }),
                };
                let cval = self.arena.get(inner).cval.as_ref().map(|cv| eval_bit_sysfunc(func, cv));
                let id = self.arena.alloc(ExprKind::SysCall { func, args: vec![inner] }, ret_ty, cval, loc);
                Some(self.apply_ctx(id, ctx, ctx.is_some(), loc))
            }
            "$past" | "$rose" | "$fell" | "$stable" | "$changed" | "$sampled" => {
                let max_args = match name {
                    "$sampled" => 1,
                    "$past" => 4,
                    _ => 2,
                };
                if slots.is_empty() || slots.len() > max_args {
                    self.err(loc, format!("{name} expects at most {max_args} arguments, got {}", slots.len()));
                    return None;
                }
                let Some(a0) = pos_arg(0) else {
                    self.err(loc, format!("missing primary expression argument in {name}"));
                    return None;
                };
                let expr = self.bind_self(tree_idx, a0)?;
                let Some((w, s, four)) = self.arena.get(expr).ty.integral_shape() else {
                    self.err(loc, format!("argument of {name} must be integral/packed"));
                    return None;
                };
                let mut depth = if name == "$sampled" { 0u32 } else { 1u32 };
                let mut gate = None;
                let mut clock = None;
                let mut clock_posedge = None;
                if name == "$past" {
                    if let Some(n_node) = pos_arg(1) {
                        let ne = self.bind_self(tree_idx, n_node)?;
                        let Some(nc) = self.arena.get(ne).cval.clone() else {
                            self.err(loc, "$past cycle count must be a constant integer");
                            return None;
                        };
                        let Some(d) = cval_pos_u32(&nc, MAX_SVA_CYCLES) else {
                            self.err(loc, "$past cycle count must be a positive integer within maximum cycle limit");
                            return None;
                        };
                        depth = d;
                    }
                    if let Some(g_node) = pos_arg(2) {
                        gate = Some(self.bind_condition(tree_idx, g_node)?);
                    }
                    if let Some(c_node) = pos_arg(3) {
                        let (ce, cp) = self.bind_clock_arg(tree_idx, c_node)?;
                        clock = Some(ce);
                        clock_posedge = cp;
                    }
                } else if let Some(c_node) = pos_arg(1) {
                    let (ce, cp) = self.bind_clock_arg(tree_idx, c_node)?;
                    clock = Some(ce);
                    clock_posedge = cp;
                }
                let kind = match name {
                    "$sampled" => SampledKind::Sampled,
                    "$past" => SampledKind::Past,
                    "$rose" => SampledKind::Rose,
                    "$fell" => SampledKind::Fell,
                    "$stable" => SampledKind::Stable,
                    _ => SampledKind::Changed,
                };
                let ret_ty = match kind {
                    SampledKind::Past | SampledKind::Sampled => {
                        RtlType::Integral { width: w, signed: s, four_state: four }
                    }
                    _ => RtlType::Integral { width: 1, signed: false, four_state: false },
                };
                let id = self.arena.alloc(
                    ExprKind::Sampled { kind, expr, depth, gate, clock, clock_posedge },
                    ret_ty,
                    None,
                    loc,
                );
                Some(self.apply_ctx(id, ctx, ctx.is_some(), loc))
            }
            "$anyseq" | "$anyconst" => {
                let w = ctx.map_or(1, |c| c.0);
                let s = ctx.is_some_and(|c| c.1);
                self.unsup(loc, format!("formal intrinsic '{name}' requires consumer harness or free input"));
                let _ = (w, s);
                None
            }
            _ => {
                self.unsup(loc, format!("unsupported system function '{name}' in RTL expression"));
                None
            }
        }
    }

    fn bind_assign_pattern(&mut self, tree_idx: usize, node: NodeId, target_ty: Option<&RtlType>) -> Option<ExprId> {
        let loc = self.loc_of(tree_idx, node);
        let Some(target_ty) = target_ty else {
            self.err(loc, "untyped assignment pattern requires an explicit type prefix or assignment context");
            return None;
        };
        let raw_entries = parse_assign_pattern_entries(self.tree(tree_idx), node);
        if raw_entries.is_empty() {
            self.err(loc, "empty assignment pattern '{}' is illegal");
            return None;
        }
        match target_ty {
            RtlType::PackedStruct { width, signed, fields, .. } => {
                self.validate_struct_pattern_keys(
                    &raw_entries,
                    fields.len(),
                    |n| fields.iter().any(|f| f.name == n),
                    loc,
                )?;
                let mut bound_fields: Vec<ExprId> = Vec::with_capacity(fields.len());
                for (idx, pf) in fields.iter().enumerate() {
                    let val_node = find_pattern_entry(&raw_entries, idx, &pf.name);
                    let Some(vn) = val_node else {
                        self.err(loc, format!("missing member '{}' in struct assignment pattern", pf.name));
                        return None;
                    };
                    bound_fields.push(self.bind_assigned(tree_idx, vn, &pf.ty)?);
                }
                let all_const = bound_fields.iter().all(|&e| self.arena.get(e).cval.is_some());
                let cval = if all_const && !bound_fields.is_empty() {
                    let mut it = bound_fields.iter().filter_map(|&e| self.arena.get(e).cval.clone());
                    let first = it.next()?;
                    Some(it.fold(first, |acc, x| CVal::concat(&acc, &x)).cast(*width, *signed))
                } else {
                    None
                };
                Some(self.arena.alloc(ExprKind::Concat(bound_fields), target_ty.clone(), cval, loc))
            }
            RtlType::PackedUnion { width, signed, four_state, fields, .. } => {
                if raw_entries.len() == 1 && raw_entries[0].0.as_deref() == Some("default") {
                    let bit_ty = RtlType::Integral { width: 1, signed: false, four_state: *four_state };
                    let b = self.bind_assigned(tree_idx, raw_entries[0].1, &bit_ty)?;
                    let cval = self
                        .arena
                        .get(b)
                        .cval
                        .as_ref()
                        .map(|bc| fill_cval(bc.bit_state(0), *width, *signed, *four_state));
                    return Some(self.arena.alloc(
                        ExprKind::Replicate { count: *width, expr: b },
                        target_ty.clone(),
                        cval,
                        loc,
                    ));
                }
                if raw_entries.len() != 1 {
                    self.err(loc, "packed union assignment pattern requires a single member or 'default'");
                    return None;
                }
                let pf = match &raw_entries[0].0 {
                    Some(k) => fields.iter().find(|f| &f.name == k),
                    None if fields.len() == 1 => fields.first(),
                    None => None,
                };
                let Some(pf) = pf else {
                    self.err(loc, "invalid or unknown member in packed union assignment pattern");
                    return None;
                };
                let bound = self.bind_assigned(tree_idx, raw_entries[0].1, &pf.ty)?;
                let coerced = self.arena.coerce(bound, *width, *signed, *four_state, loc);
                let cval = self.arena.get(coerced).cval.clone();
                Some(self.arena.alloc(ExprKind::Concat(vec![coerced]), target_ty.clone(), cval, loc))
            }
            RtlType::UnpackedStruct { fields, .. } => {
                self.validate_struct_pattern_keys(
                    &raw_entries,
                    fields.len(),
                    |n| fields.iter().any(|(fnm, _)| fnm == n),
                    loc,
                )?;
                let mut out = Vec::with_capacity(fields.len());
                for (idx, (fname, fty)) in fields.iter().enumerate() {
                    let Some(vn) = find_pattern_entry(&raw_entries, idx, fname) else {
                        self.err(loc, format!("missing member '{fname}' in unpacked struct assignment pattern"));
                        return None;
                    };
                    let be = self.bind_assigned(tree_idx, vn, fty)?;
                    out.push((Some(fname.clone()), be));
                }
                Some(self.arena.alloc(ExprKind::AssignPattern(out), target_ty.clone(), None, loc))
            }
            RtlType::PackedArray { elem, left, right, width, signed, .. } => {
                let Some(cnt_u32) = range_len_u32(*left, *right, MAX_ARRAY_ELEMS) else {
                    self.err(loc, "packed array assignment pattern exceeds maximum element count");
                    return None;
                };
                let cnt = cnt_u32 as usize;
                self.validate_array_pattern_keys(&raw_entries, cnt, loc)?;
                let mut parts = Vec::with_capacity(cnt);
                for idx in 0..cnt {
                    let Some(vn) = find_pattern_entry(&raw_entries, idx, "") else {
                        self.err(loc, "insufficient elements in packed array assignment pattern");
                        return None;
                    };
                    parts.push(self.bind_assigned(tree_idx, vn, elem)?);
                }
                let all_const = parts.iter().all(|&e| self.arena.get(e).cval.is_some());
                let cval = if all_const && !parts.is_empty() {
                    let mut it = parts.iter().filter_map(|&e| self.arena.get(e).cval.clone());
                    let first = it.next()?;
                    Some(it.fold(first, |acc, x| CVal::concat(&acc, &x)).cast(*width, *signed))
                } else {
                    None
                };
                Some(self.arena.alloc(ExprKind::Concat(parts), target_ty.clone(), cval, loc))
            }
            RtlType::UnpackedArray { elem, dims } if !dims.is_empty() => {
                let (left, right) = dims[0];
                let Some(cnt_u32) = range_len_u32(left, right, MAX_ARRAY_ELEMS) else {
                    self.err(loc, "unpacked array assignment pattern exceeds maximum element count");
                    return None;
                };
                let cnt = cnt_u32 as usize;
                self.validate_array_pattern_keys(&raw_entries, cnt, loc)?;
                let sub_ty = if dims.len() > 1 {
                    RtlType::UnpackedArray { elem: elem.clone(), dims: dims[1..].to_vec() }
                } else {
                    (**elem).clone()
                };
                let mut out = Vec::with_capacity(cnt);
                for idx in 0..cnt {
                    let Some(vn) = find_pattern_entry(&raw_entries, idx, "") else {
                        self.err(loc, "insufficient elements in unpacked array assignment pattern");
                        return None;
                    };
                    out.push((None, self.bind_assigned(tree_idx, vn, &sub_ty)?));
                }
                Some(self.arena.alloc(ExprKind::AssignPattern(out), target_ty.clone(), None, loc))
            }
            ty if ty.is_packed() => {
                let w = ty.bit_width().unwrap_or(1);
                let bit_ty = RtlType::Integral { width: 1, signed: false, four_state: ty.is_four_state() };
                if raw_entries.len() == 1 && raw_entries[0].0.as_deref() == Some("default") {
                    let b = self.bind_assigned(tree_idx, raw_entries[0].1, &bit_ty)?;
                    let cval = self
                        .arena
                        .get(b)
                        .cval
                        .as_ref()
                        .map(|bc| fill_cval(bc.bit_state(0), w, ty.is_signed(), ty.is_four_state()));
                    return Some(self.arena.alloc(ExprKind::Replicate { count: w, expr: b }, ty.clone(), cval, loc));
                }
                if w > MAX_ARRAY_ELEMS {
                    self.err(loc, "integral assignment pattern exceeds maximum element count");
                    return None;
                }
                if raw_entries.iter().any(|(k, _)| k.is_some()) {
                    self.err(loc, "named keys are illegal in integral assignment pattern");
                    return None;
                }
                if raw_entries.len() != w as usize {
                    self.err(
                        loc,
                        format!(
                            "assignment pattern for {w}-bit integral target requires {w} elements, got {}",
                            raw_entries.len()
                        ),
                    );
                    return None;
                }
                let mut parts = Vec::with_capacity(raw_entries.len());
                for (_, vn) in &raw_entries {
                    parts.push(self.bind_assigned(tree_idx, *vn, &bit_ty)?);
                }
                let all_const = parts.iter().all(|&e| self.arena.get(e).cval.is_some());
                let cval = if all_const && !parts.is_empty() {
                    let mut it = parts.iter().filter_map(|&e| self.arena.get(e).cval.clone());
                    let first = it.next()?;
                    Some(it.fold(first, |acc, x| CVal::concat(&acc, &x)).cast(w, ty.is_signed()))
                } else {
                    None
                };
                Some(self.arena.alloc(ExprKind::Concat(parts), ty.clone(), cval, loc))
            }
            _ => {
                self.err(loc, "assignment pattern is illegal for target type");
                None
            }
        }
    }

    fn validate_struct_pattern_keys(
        &mut self,
        entries: &[(Option<String>, NodeId)],
        field_count: usize,
        mut has_field: impl FnMut(&str) -> bool,
        loc: Loc,
    ) -> Option<()> {
        let any_pos = entries.iter().any(|(k, _)| k.is_none());
        let any_named = entries.iter().any(|(k, _)| k.as_deref().is_some_and(|s| s != "default"));
        if any_pos && any_named {
            self.err(loc, "cannot mix positional and named elements in struct assignment pattern");
            return None;
        }
        if any_pos && entries.iter().all(|(k, _)| k.is_none()) && entries.len() > field_count {
            self.err(
                loc,
                format!("excess elements in struct assignment pattern: expected {field_count}, got {}", entries.len()),
            );
            return None;
        }
        for (k_opt, _) in entries {
            if let Some(k) = k_opt {
                if k != "default" && !has_field(k) {
                    self.err(loc, format!("unknown member '{k}' in struct assignment pattern"));
                    return None;
                }
            }
        }
        Some(())
    }

    fn validate_array_pattern_keys(
        &mut self,
        entries: &[(Option<String>, NodeId)],
        elem_count: usize,
        loc: Loc,
    ) -> Option<()> {
        for (k_opt, _) in entries {
            if let Some(k) = k_opt {
                if k != "default" {
                    self.err(loc, format!("unsupported key '{k}' in array assignment pattern"));
                    return None;
                }
            }
        }
        if entries.iter().all(|(k, _)| k.is_none()) && entries.len() > elem_count {
            self.err(
                loc,
                format!("excess elements in array assignment pattern: expected {elem_count}, got {}", entries.len()),
            );
            return None;
        }
        Some(())
    }
}

/// Formal port descriptor extracted from a CST `FunctionDecl` or `TaskDecl`.
#[derive(Clone, Debug)]
pub struct SubroutineFormal {
    /// Formal parameter name.
    pub name: String,
    /// Port direction (`input` by default).
    pub dir: PortDir,
    /// Optional `DataType` CST node.
    pub dtype: Option<NodeId>,
    /// Declarator CST node.
    pub decl: Option<NodeId>,
    /// Optional default value expression CST node.
    pub default_expr: Option<NodeId>,
}

/// Returns the unqualified name of a `FunctionDecl` or `TaskDecl`.
pub fn subroutine_decl_name(t: &Tree, decl: NodeId) -> Option<String> {
    let n = t.child(decl, SyntaxKind::Name)?;
    let last = *t.token_range(n).last()?;
    let raw = &*t.tokens[last as usize].text;
    Some(raw.strip_prefix('\\').unwrap_or(raw).to_string())
}

/// Extracts all formal arguments (header `PortList` or body port declarations) of a `FunctionDecl` or `TaskDecl`.
pub fn extract_subroutine_formals(t: &Tree, decl: NodeId) -> Vec<SubroutineFormal> {
    let mut out = Vec::new();
    if let Some(pl) = t.child(decl, SyntaxKind::PortList) {
        let mut cur_dir = PortDir::Input;
        let mut cur_dt: Option<NodeId> = None;
        for p in t.children(pl).filter(|&p| t.kind(p) == SyntaxKind::Port) {
            let own: Vec<&str> = t
                .node(p)
                .children
                .iter()
                .filter_map(|c| match *c {
                    sv_syntax::Child::Token(i) if !t.tokens[i as usize].kind.is_trivia() => {
                        Some(&*t.tokens[i as usize].text)
                    }
                    _ => None,
                })
                .collect();
            let dir_tok = own.iter().find_map(|&tok| match tok {
                "input" => Some(PortDir::Input),
                "output" => Some(PortDir::Output),
                "inout" => Some(PortDir::Inout),
                "ref" => Some(PortDir::Ref),
                _ => None,
            });
            let dt = t.child(p, SyntaxKind::DataType);
            if dir_tok.is_some() || dt.is_some() {
                if let Some(d) = dir_tok {
                    cur_dir = d;
                }
                cur_dt = dt;
            }
            for d in t.children(p).filter(|&c| t.kind(c) == SyntaxKind::Declarator) {
                let Some(nm_node) = t.child(d, SyntaxKind::Name) else { continue };
                let Some(&tok) = t.token_range(nm_node).last() else { continue };
                let raw = &*t.tokens[tok as usize].text;
                let name = raw.strip_prefix('\\').unwrap_or(raw).to_string();
                let default_expr = t
                    .children(d)
                    .find(|&c| !matches!(t.kind(c), SyntaxKind::Name | SyntaxKind::Dim | SyntaxKind::Attribute));
                out.push(SubroutineFormal { name, dir: cur_dir, dtype: cur_dt, decl: Some(d), default_expr });
            }
        }
        return out;
    }
    // Non-header body port declarations (`input [7:0] a;`).
    if let Some(body) = t.child(decl, SyntaxKind::Body) {
        for item in t.children(body).filter(|&c| t.kind(c) == SyntaxKind::DataDecl) {
            let own: Vec<&str> = t
                .node(item)
                .children
                .iter()
                .filter_map(|c| match *c {
                    sv_syntax::Child::Token(i) if !t.tokens[i as usize].kind.is_trivia() => {
                        Some(&*t.tokens[i as usize].text)
                    }
                    _ => None,
                })
                .collect();
            let Some(dir) = own.first().and_then(|&tok| match tok {
                "input" => Some(PortDir::Input),
                "output" => Some(PortDir::Output),
                "inout" => Some(PortDir::Inout),
                "ref" => Some(PortDir::Ref),
                _ => None,
            }) else {
                continue;
            };
            let dt = t.child(item, SyntaxKind::DataType);
            for d in t.children(item).filter(|&c| t.kind(c) == SyntaxKind::Declarator) {
                let Some(nm_node) = t.child(d, SyntaxKind::Name) else { continue };
                let Some(&tok) = t.token_range(nm_node).last() else { continue };
                let raw = &*t.tokens[tok as usize].text;
                let name = raw.strip_prefix('\\').unwrap_or(raw).to_string();
                let default_expr = t
                    .children(d)
                    .find(|&c| !matches!(t.kind(c), SyntaxKind::Name | SyntaxKind::Dim | SyntaxKind::Attribute));
                out.push(SubroutineFormal { name, dir, dtype: dt, decl: Some(d), default_expr });
            }
        }
    }
    out
}

/// Resolves an interface port [`SymbolId`] inside `inst_id` up through parent `PortConn`s to its
/// concrete [`InstanceId`] (interface instance) and optional modport name.
pub fn resolve_interface_port(
    trees: &[Tree],
    design: &ElaboratedDesign,
    inst_id: InstanceId,
    port_sym: SymbolId,
) -> Option<(InstanceId, Option<String>)> {
    let inst = design.instance(inst_id);
    let parent_id = inst.parent?;
    let sym = design.symbol(port_sym);
    let declared_mp = match &sym.ty {
        RtlType::Interface { modport, .. } => modport.clone(),
        _ => None,
    };
    let port_pos = inst.ports.iter().position(|&p| p == port_sym)?;
    let conn = inst
        .port_conns
        .iter()
        .find(|c| c.port_name.as_deref() == Some(sym.name.as_str()))
        .or_else(|| inst.port_conns.iter().find(|c| c.port_name.is_none() && !c.wildcard && c.arg_index == port_pos))
        .or_else(|| inst.port_conns.iter().find(|c| c.wildcard));

    let (actual_text, scope_prefix) = match conn {
        Some(c) if c.implicit_named || c.wildcard => (sym.name.clone(), c.scope_prefix.clone()),
        Some(c) => {
            let (tr, nd) = c.expr?;
            let t = trees.get(tr)?;
            (t.bare_text(nd).trim().to_string(), c.scope_prefix.clone())
        }
        None => return None,
    };

    // Resolve `bus_i`, `bus_i.master`, or parent interface port `parent_bus`.
    if let Some(child_iface) = design.resolve_instance(parent_id, &scope_prefix, &actual_text) {
        return Some((child_iface, declared_mp));
    }
    if let Some((base_name, mp_name)) = actual_text.rsplit_once('.') {
        if let Some(child_iface) = design.resolve_instance(parent_id, &scope_prefix, base_name) {
            return Some((child_iface, declared_mp.or_else(|| Some(mp_name.to_string()))));
        }
    }
    let parent_inst = design.instance(parent_id);
    if let Some(psym) = parent_inst.resolve_lexical(&scope_prefix, &actual_text) {
        let (target, pmp) = resolve_interface_port(trees, design, parent_id, psym)?;
        return Some((target, declared_mp.or(pmp)));
    }
    None
}

/// Maps logical index `idx` on a vector/packed-array with `bounds = (left, right)` and element
/// width `elem_w` to physical LSB bit offset `0..total_w`, or `None` when out of bounds.
pub fn map_bit_offset(idx: i128, bounds: (i128, i128), elem_w: u32, total_w: u32) -> Option<u32> {
    let (left, right) = bounds;
    let rel = if left >= right {
        if idx < right || idx > left {
            return None;
        }
        u32::try_from(idx.abs_diff(right)).ok()?
    } else {
        // Ascending `[0:7]` puts MSB at `left = 0` and LSB (`0` bit offset) at `right = 7`.
        if idx < left || idx > right {
            return None;
        }
        u32::try_from(right.abs_diff(idx)).ok()?
    };
    let bit_lo = rel.checked_mul(elem_w)?;
    (bit_lo.checked_add(elem_w)? <= total_w).then_some(bit_lo)
}

fn slice_cval_bounds(
    bc: &CVal,
    msb: i128,
    lsb: i128,
    bounds: (i128, i128),
    elem_w: u32,
    total_w: u32,
    four_state: bool,
) -> CVal {
    let count = range_len_u32(msb, lsb, MAX_BIT_WIDTH).unwrap_or(1) as usize;
    let ascending_step = bounds.0 >= bounds.1;
    // Build from LSB index to MSB index.
    let low_idx = if ascending_step { msb.min(lsb) } else { msb.max(lsb) };
    let mut acc: Option<CVal> = None;
    for k in 0..count {
        let cur_idx_opt = if ascending_step { low_idx.checked_add(k as i128) } else { low_idx.checked_sub(k as i128) };
        let part = match cur_idx_opt.and_then(|cur_idx| map_bit_offset(cur_idx, bounds, elem_w, total_w)) {
            Some(off) => bc.slice(off, elem_w, false),
            None => {
                if four_state {
                    CVal::all_x(elem_w, false)
                } else {
                    CVal::new(elem_w, false, 0)
                }
            }
        };
        acc = Some(match acc {
            None => part,
            Some(prev) => CVal::concat(&part, &prev),
        });
    }
    acc.unwrap_or_else(|| CVal::new(1, false, 0))
}

fn eval_clog2(cv: &CVal) -> CVal {
    if !cv.known() {
        return CVal::all_x(32, true);
    }
    let mut v = cv.bits_v();
    if cv.s && cv.v.is_neg() {
        // IEEE 1800 §20.8.1: negative signed values are treated as their 2's-complement bit width unsigned value.
        v = cv.v.trunc(cv.w.max(32), false);
    }
    if v.le(&crate::bigint::Int::from(1)) {
        return CVal::new(32, true, 0);
    }
    let minus_one = v.sub(&crate::bigint::Int::from(1));
    let mut bits = 0i128;
    for b in 0..=(cv.w.max(64) + 1) {
        if minus_one.shr(b).is_zero() {
            bits = b as i128;
            break;
        }
    }
    CVal::new(32, true, bits)
}

fn total_type_bits(ty: &RtlType) -> Option<u64> {
    if let Some(w) = ty.bit_width() {
        return Some(w as u64);
    }
    match ty {
        RtlType::UnpackedArray { elem, dims } => {
            let mut acc = total_type_bits(elem)?;
            for &(l, r) in dims {
                let len = u64::try_from(l.abs_diff(r).checked_add(1)?).ok()?;
                acc = acc.checked_mul(len)?;
            }
            Some(acc)
        }
        RtlType::UnpackedStruct { is_union, fields, .. } => {
            if *is_union {
                let mut mx = 0u64;
                for (_, ft) in fields {
                    mx = mx.max(total_type_bits(ft)?);
                }
                Some(mx)
            } else {
                let mut sum = 0u64;
                for (_, ft) in fields {
                    sum = sum.checked_add(total_type_bits(ft)?)?;
                }
                Some(sum)
            }
        }
        _ => None,
    }
}

pub(crate) fn cval_nonneg_u32(cv: &CVal, max_val: u32) -> Option<u32> {
    if !cv.known() {
        return None;
    }
    let v = cv.v.to_i128()?;
    let u = u32::try_from(v).ok()?;
    (u <= max_val).then_some(u)
}

pub(crate) fn cval_pos_u32(cv: &CVal, max_val: u32) -> Option<u32> {
    cval_nonneg_u32(cv, max_val).filter(|&u| u > 0)
}

fn types_assignment_compatible(lhs: &RtlType, rhs: &RtlType) -> bool {
    if lhs.is_packed() && rhs.is_packed() {
        return true;
    }
    match (lhs, rhs) {
        (RtlType::UnpackedArray { elem: le, dims: ld }, RtlType::UnpackedArray { elem: re, dims: rd }) => {
            ld.len() == rd.len()
                && ld.iter().zip(rd.iter()).all(|(&(l1, r1), &(l2, r2))| l1.abs_diff(r1) == l2.abs_diff(r2))
                && (!le.is_packed() || le.bit_width() == re.bit_width())
                && types_assignment_compatible(le, re)
        }
        (
            RtlType::UnpackedStruct { name: ln, is_union: lu, fields: lf },
            RtlType::UnpackedStruct { name: rn, is_union: ru, fields: rf },
        ) => {
            lu == ru
                && lf.len() == rf.len()
                && (ln == rn || (ln.is_none() && rn.is_none()))
                && lf.iter().zip(rf.iter()).all(|((na, ta), (nb, tb))| na == nb && types_assignment_compatible(ta, tb))
        }
        _ => false,
    }
}

#[derive(Clone, Debug)]
pub(crate) enum CallArgSlot {
    Positional(NodeId),
    EmptyPositional,
    Named { name: String, expr: Option<NodeId> },
    Wildcard,
}

pub(crate) fn parse_arg_list_slots(t: &Tree, arg_list: Option<NodeId>) -> Vec<CallArgSlot> {
    let Some(al) = arg_list else { return Vec::new() };
    let mut out = Vec::new();
    let mut saw_any_content = false;
    let mut slot_has_arg = false;
    for ch in &t.node(al).children {
        match *ch {
            sv_syntax::Child::Token(ti) => {
                let tok = &t.tokens[ti as usize];
                if tok.kind.is_trivia() {
                    continue;
                }
                match &*tok.text {
                    "," => {
                        if !slot_has_arg {
                            out.push(CallArgSlot::EmptyPositional);
                        }
                        saw_any_content = true;
                        slot_has_arg = false;
                    }
                    ")" if saw_any_content && !slot_has_arg => {
                        out.push(CallArgSlot::EmptyPositional);
                    }
                    _ => {}
                }
            }
            sv_syntax::Child::Node(c) => {
                if t.kind(c) == SyntaxKind::Attribute {
                    continue;
                }
                saw_any_content = true;
                slot_has_arg = true;
                if t.kind(c) == SyntaxKind::Arg {
                    let own: Vec<&str> = t
                        .node(c)
                        .children
                        .iter()
                        .filter_map(|sub| match *sub {
                            sv_syntax::Child::Token(i) if !t.tokens[i as usize].kind.is_trivia() => {
                                Some(&*t.tokens[i as usize].text)
                            }
                            _ => None,
                        })
                        .collect();
                    let val_node =
                        t.children(c).find(|&k| !matches!(t.kind(k), SyntaxKind::Name | SyntaxKind::Attribute));
                    if own.first() == Some(&".*") || (own.first() == Some(&".") && own.get(1) == Some(&"*")) {
                        out.push(CallArgSlot::Wildcard);
                    } else if own.first() == Some(&".") {
                        let raw_nm = t
                            .child(c, SyntaxKind::Name)
                            .and_then(|n| t.token_range(n).last().map(|&ti| &*t.tokens[ti as usize].text))
                            .or_else(|| own.get(1).copied().filter(|&s| s != "(" && s != ")"))
                            .unwrap_or("");
                        let name = raw_nm.strip_prefix('\\').unwrap_or(raw_nm).to_string();
                        out.push(CallArgSlot::Named { name, expr: val_node });
                    } else if let Some(vn) = val_node {
                        out.push(CallArgSlot::Positional(vn));
                    } else {
                        out.push(CallArgSlot::EmptyPositional);
                    }
                } else {
                    out.push(CallArgSlot::Positional(c));
                }
            }
        }
    }
    out
}

pub(crate) fn resolve_call_arg_map(
    slots: &[CallArgSlot],
    formal_names: &[&str],
    callee_desc: &str,
    loc: Loc,
    diags: &mut Vec<ExprDiag>,
) -> Option<Vec<Option<NodeId>>> {
    let mut mapped: Vec<Option<NodeId>> = vec![None; formal_names.len()];
    let mut assigned = vec![false; formal_names.len()];
    let mut seen_named = false;
    for (idx, slot) in slots.iter().enumerate() {
        match slot {
            CallArgSlot::Wildcard => {
                diags.push(ExprDiag {
                    loc,
                    message: format!("wildcard '.*' argument is illegal in call to {callee_desc}"),
                    unsupported: false,
                });
                return None;
            }
            CallArgSlot::EmptyPositional => {
                diags.push(ExprDiag {
                    loc,
                    message: format!("empty positional argument is illegal in call to {callee_desc}"),
                    unsupported: false,
                });
                return None;
            }
            CallArgSlot::Positional(e) => {
                if seen_named {
                    diags.push(ExprDiag {
                        loc,
                        message: format!("positional argument cannot follow named argument in call to {callee_desc}"),
                        unsupported: false,
                    });
                    return None;
                }
                if idx >= formal_names.len() {
                    diags.push(ExprDiag {
                        loc,
                        message: format!(
                            "too many arguments in call to {callee_desc}: expected at most {}, got {}",
                            formal_names.len(),
                            slots.len()
                        ),
                        unsupported: false,
                    });
                    return None;
                }
                assigned[idx] = true;
                mapped[idx] = Some(*e);
            }
            CallArgSlot::Named { name, expr } => {
                seen_named = true;
                let Some(pos) = formal_names.iter().position(|&fnm| fnm == name) else {
                    diags.push(ExprDiag {
                        loc,
                        message: format!("unknown formal argument '.{name}' in call to {callee_desc}"),
                        unsupported: false,
                    });
                    return None;
                };
                if assigned[pos] {
                    diags.push(ExprDiag {
                        loc,
                        message: format!("duplicate argument for formal '{name}' in call to {callee_desc}"),
                        unsupported: false,
                    });
                    return None;
                }
                assigned[pos] = true;
                mapped[pos] = *expr;
            }
        }
    }
    Some(mapped)
}

fn query_type_bounds(ty: &RtlType, packed_1d: Option<(i128, i128)>, dim_num: usize) -> Option<(i128, i128)> {
    let idx = dim_num.saturating_sub(1);
    match ty {
        RtlType::UnpackedArray { elem, dims } => {
            if idx < dims.len() {
                Some(dims[idx])
            } else {
                query_type_bounds(elem, None, dim_num - dims.len())
            }
        }
        RtlType::PackedArray { elem, left, right, .. } => {
            if idx == 0 {
                Some((*left, *right))
            } else {
                query_type_bounds(elem, None, dim_num - 1)
            }
        }
        _ if ty.is_packed() && idx == 0 => Some(packed_1d.unwrap_or(((ty.bit_width()? as i128) - 1, 0))),
        _ => None,
    }
}

fn eval_bit_sysfunc(func: SysFunc, cv: &CVal) -> CVal {
    let w = cv.w.max(1);
    match func {
        SysFunc::IsUnknown => CVal::bool(!cv.known()),
        SysFunc::CountOnes => {
            let mut ones = 0i128;
            for i in 0..w {
                if cv.bit_state(i) == BitState::One {
                    ones += 1;
                }
            }
            CVal::new(32, true, ones)
        }
        SysFunc::OneHot => {
            let mut ones = 0u32;
            for i in 0..w {
                if cv.bit_state(i) == BitState::One {
                    ones += 1;
                }
            }
            CVal::bool(ones == 1)
        }
        SysFunc::OneHot0 => {
            let mut ones = 0u32;
            for i in 0..w {
                if cv.bit_state(i) == BitState::One {
                    ones += 1;
                }
            }
            CVal::bool(ones <= 1)
        }
        _ => CVal::new(32, true, 0),
    }
}

fn parse_assign_pattern_entries(t: &Tree, pat: NodeId) -> Vec<(Option<String>, NodeId)> {
    let mut out = Vec::new();
    let mut pending_nodes: Vec<NodeId> = Vec::new();
    let mut has_colon = false;
    let mut key_tok: Option<String> = None;
    for ch in &t.node(pat).children {
        match *ch {
            sv_syntax::Child::Token(ti) => {
                let tok = &t.tokens[ti as usize];
                if tok.kind.is_trivia() {
                    continue;
                }
                match &*tok.text {
                    "'{" | "{" => {}
                    ":" => {
                        has_colon = true;
                        if let Some(kn) = pending_nodes.pop() {
                            key_tok = Some(t.bare_text(kn).trim().to_string());
                        }
                    }
                    "," | "}" => {
                        if let Some(vn) = pending_nodes.pop() {
                            let k = if has_colon { key_tok.take() } else { None };
                            out.push((k, vn));
                        }
                        pending_nodes.clear();
                        has_colon = false;
                        key_tok = None;
                    }
                    "default" => {
                        key_tok = Some("default".into());
                    }
                    _ => {}
                }
            }
            sv_syntax::Child::Node(n) => {
                if t.kind(n) != SyntaxKind::Attribute {
                    pending_nodes.push(n);
                }
            }
        }
    }
    if let Some(vn) = pending_nodes.pop() {
        let k = if has_colon { key_tok.take() } else { None };
        out.push((k, vn));
    }
    out
}

fn find_pattern_entry(entries: &[(Option<String>, NodeId)], pos: usize, field_name: &str) -> Option<NodeId> {
    if !field_name.is_empty() {
        if let Some((_, n)) = entries.iter().find(|(k, _)| k.as_deref() == Some(field_name)) {
            return Some(*n);
        }
    }
    if entries.iter().all(|(k, _)| k.is_none()) {
        if let Some((_, n)) = entries.get(pos) {
            return Some(*n);
        }
    }
    entries.iter().find(|(k, _)| k.as_deref() == Some("default")).map(|(_, n)| *n)
}
