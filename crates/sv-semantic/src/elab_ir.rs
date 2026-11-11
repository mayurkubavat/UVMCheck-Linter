//! Arena-backed elaborated RTL design object model (IEEE 1800-2023 Clause 23).
//!
//! Produced alongside the flat hierarchy text lines during [`crate::elaborate_full`] /
//! [`crate::elaborate_with_params`]. Every elaborated module or interface instance gets
//! its own [`Instance`] and its own [`Symbol`]s (no instance sharing across separate
//! instantiations), and taken generate blocks (`if`/`case`/`for`) appear as lexical
//! scope prefixes (`g[0].x`) on [`Symbol::rel_path`] and [`Instance::rel_path`].

use std::collections::BTreeMap;

use sv_syntax::{NodeId, SyntaxKind};

use crate::fx::FxMap;
use crate::model::Loc;
use crate::types::CVal;
use crate::{Builder, Env, Found, NodeRef};

/// Dense arena identifier for an elaborated [`Instance`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstanceId(pub u32);

/// Dense arena identifier for an elaborated [`Symbol`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolId(pub u32);

/// Port direction on a module, interface, modport, or subroutine port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PortDir {
    /// `input`
    Input,
    /// `output`
    Output,
    /// `inout`
    Inout,
    /// `ref`
    Ref,
}

impl PortDir {
    /// Keyword spelling (`"input"`, `"output"`, `"inout"`, `"ref"`).
    pub fn as_str(self) -> &'static str {
        match self {
            PortDir::Input => "input",
            PortDir::Output => "output",
            PortDir::Inout => "inout",
            PortDir::Ref => "ref",
        }
    }
}

/// Kind of per-instance [`Symbol`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    /// Module or interface port (`is_net` distinguishes `input wire` vs `output logic`).
    Port {
        /// Port direction.
        dir: PortDir,
        /// `true` when the internal port object is a net rather than a variable.
        is_net: bool,
    },
    /// Variable (`logic`, `reg`, `bit`, `int`, struct, unpacked array, ...).
    Var,
    /// Net (`wire`, `tri`, `uwire`, `supply0`, `supply1`, ...).
    Net {
        /// Net keyword (`"wire"`, `"tri"`, ...).
        net_type: String,
    },
    /// Value parameter or `localparam`.
    Param {
        /// `true` for `localparam` or body parameter when header `#(...)` exists.
        local: bool,
    },
    /// Elaborated `genvar` constant inside a generate loop iteration.
    Genvar,
}

/// One member of a packed `struct` or `union`, with its bit offset from bit 0 (LSB).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedField {
    /// Member identifier.
    pub name: String,
    /// Lowest bit index in the enclosing packed aggregate (`0` = LSB).
    pub lo: u32,
    /// Bit width of this member.
    pub width: u32,
    /// Elaborated type of this member.
    pub ty: RtlType,
}

/// Elaborated SystemVerilog data type for RTL binding, typing, and netlist lowering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RtlType {
    /// Scalar or packed-vector integer (`logic [7:0]`, `int`, `byte`, `bit`).
    Integral {
        /// Total bit width (`>= 1`).
        width: u32,
        /// Two's-complement signedness.
        signed: bool,
        /// `true` when 4-state (`logic`, `reg`, `integer`), `false` when 2-state (`bit`, `int`).
        four_state: bool,
    },
    /// `struct packed [signed] { ... }`.
    PackedStruct {
        /// Optional typedef name.
        name: Option<String>,
        /// Total packed bit width.
        width: u32,
        /// Whether the packed struct is `signed`.
        signed: bool,
        /// `true` if any member is 4-state (or default packed 4-state).
        four_state: bool,
        /// Fields in declaration order (MSB first, with exact `lo` bit offsets).
        fields: Vec<PackedField>,
    },
    /// `union packed [signed] { ... }`.
    PackedUnion {
        /// Optional typedef name.
        name: Option<String>,
        /// Total packed bit width (maximum of member widths).
        width: u32,
        /// Whether the packed union is `signed`.
        signed: bool,
        /// `true` if any member is 4-state.
        four_state: bool,
        /// Union members (each starting at `lo = 0`).
        fields: Vec<PackedField>,
    },
    /// Multi-dimensional packed array (`T [hi:lo]`) where element is packed.
    PackedArray {
        /// Element type after indexing one packed dimension.
        elem: Box<RtlType>,
        /// Declared left bound (`[left:right]`).
        left: i128,
        /// Declared right bound (`[left:right]`).
        right: i128,
        /// Total bit width (`elem_width * count`).
        width: u32,
        /// Signedness of the packed array as a whole (`false` per IEEE 1800 §7.4.1).
        signed: bool,
        /// Whether elements carry 4-state bits.
        four_state: bool,
    },
    /// `enum [base_type] { ... }`.
    Enum {
        /// Optional typedef name.
        name: Option<String>,
        /// Underlying integral width.
        width: u32,
        /// Underlying signedness.
        signed: bool,
        /// Underlying 4-state flag.
        four_state: bool,
        /// Enumerator names and constant values.
        variants: Vec<(String, CVal)>,
    },
    /// Fixed-size unpacked array (`T mem [left:right]...`).
    UnpackedArray {
        /// Element type after indexing all `dims` (or slice of `dims`).
        elem: Box<RtlType>,
        /// Unpacked dimensions `(left, right)` in outer-to-inner order.
        dims: Vec<(i128, i128)>,
    },
    /// Unpacked `struct` or `union`.
    UnpackedStruct {
        /// Optional typedef name.
        name: Option<String>,
        /// `true` for unpacked `union`, `false` for unpacked `struct`.
        is_union: bool,
        /// Members in declaration order.
        fields: Vec<(String, RtlType)>,
    },
    /// Interface port or handle (`bus` or `bus.master`).
    Interface {
        /// Interface definition name.
        def: String,
        /// Optional modport name.
        modport: Option<String>,
    },
    /// `event`.
    Event,
    /// `void` (e.g. `void` function return type).
    Void,
    /// Non-synthesizable / non-RTL type (`real`, `string`, `class`, `queue`, `dynamic array`, ...).
    NonSynthesizable(String),
}

impl RtlType {
    /// Total bit width when `self` is a packed/integral/enum value (`None` for unpacked or non-RTL).
    pub fn bit_width(&self) -> Option<u32> {
        match self {
            RtlType::Integral { width, .. }
            | RtlType::PackedStruct { width, .. }
            | RtlType::PackedUnion { width, .. }
            | RtlType::PackedArray { width, .. }
            | RtlType::Enum { width, .. } => Some(*width),
            _ => None,
        }
    }

    /// Whether `self` is signed as an integral value.
    pub fn is_signed(&self) -> bool {
        match self {
            RtlType::Integral { signed, .. }
            | RtlType::PackedStruct { signed, .. }
            | RtlType::PackedUnion { signed, .. }
            | RtlType::PackedArray { signed, .. }
            | RtlType::Enum { signed, .. } => *signed,
            _ => false,
        }
    }

    /// Whether `self` carries 4-state (`0`/`1`/`x`/`z`) values.
    pub fn is_four_state(&self) -> bool {
        match self {
            RtlType::Integral { four_state, .. }
            | RtlType::PackedStruct { four_state, .. }
            | RtlType::PackedUnion { four_state, .. }
            | RtlType::PackedArray { four_state, .. }
            | RtlType::Enum { four_state, .. } => *four_state,
            RtlType::UnpackedArray { elem, .. } => elem.is_four_state(),
            RtlType::UnpackedStruct { fields, .. } => fields.iter().any(|(_, t)| t.is_four_state()),
            _ => false,
        }
    }

    /// Returns `(width, signed, four_state)` when `self` can participate in bit-vector expressions.
    pub fn integral_shape(&self) -> Option<(u32, bool, bool)> {
        Some((self.bit_width()?, self.is_signed(), self.is_four_state()))
    }

    /// `true` when `self` is a packed value (`Integral`, `PackedStruct`, `PackedUnion`, `PackedArray`, `Enum`).
    pub fn is_packed(&self) -> bool {
        self.bit_width().is_some()
    }

    /// Looks up a field in a `PackedStruct` or `PackedUnion`.
    pub fn packed_field(&self, name: &str) -> Option<&PackedField> {
        match self {
            RtlType::PackedStruct { fields, .. } | RtlType::PackedUnion { fields, .. } => {
                fields.iter().find(|f| f.name == name)
            }
            _ => None,
        }
    }

    /// Converts `self` to a flat `RtlType::Integral` with the same `(width, signed, four_state)`.
    pub fn to_integral(&self) -> Option<RtlType> {
        let (width, signed, four_state) = self.integral_shape()?;
        Some(RtlType::Integral { width, signed, four_state })
    }
}

/// Port connection on an elaborated child [`Instance`], evaluated in its parent's scope.
#[derive(Clone, Debug)]
pub struct PortConn {
    /// Formal port name (`Some("clk")` for `.clk(c)`, `None` for positional).
    pub port_name: Option<String>,
    /// Positional index in the instantiation's argument list.
    pub arg_index: usize,
    /// Connected expression CST `(tree, node)` (`None` for explicit empty `.p()` or implicit `.p`).
    pub expr: Option<(usize, NodeId)>,
    /// `true` for IEEE 1800 §23.3.2.3 implicit named port connection (`.clk` without parentheses).
    pub implicit_named: bool,
    /// `true` for SystemVerilog `.*` wildcard port connection.
    pub wildcard: bool,
    /// Generate-block prefix in the parent instance where this instantiation sits.
    pub scope_prefix: String,
    /// Active genvars and generate-block constants in the parent at the instantiation site.
    pub gen_consts: Vec<(String, CVal)>,
    /// Source location of the port argument.
    pub loc: Loc,
}

/// Kind of active elaborated module/generate item recorded on an [`Instance`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ElabItemKind {
    /// `always_ff`, `always_comb`, `always_latch`, `always`, `initial`, or `final`.
    ProceduralBlock,
    /// `assign lhs = rhs;`
    ContinuousAssign,
    /// Module-level immediate `assert` / `assume` / `cover`.
    AssertStmt,
    /// Concurrent `assert property` / `assume property` / `cover property` / `cover sequence`.
    ConcurrentAssert,
    /// `clocking` / `default clocking` declaration.
    ClockingDecl,
    /// `default disable iff expr;`
    DefaultDisable,
    /// `function ... endfunction`
    FunctionDecl,
    /// `task ... endtask`
    TaskDecl,
    /// `sequence ... endsequence`
    SequenceDecl,
    /// `property ... endproperty`
    PropertyDecl,
    /// `let name(...) = expr;`
    LetDecl,
    /// `modport name (...);`
    ModportDecl,
    /// Built-in gate instantiation (`and`, `or`, `not`, `xor`, `buf`, ...).
    GateInst,
    /// `alias a = b;`
    NetAlias,
}

/// An active module or taken-generate-block item inside an elaborated [`Instance`].
#[derive(Clone, Debug)]
pub struct ElabItem {
    /// Item classification.
    pub kind: ElabItemKind,
    /// Index into `Compilation::trees()`.
    pub tree: usize,
    /// CST node ID in `trees[tree]`.
    pub node: NodeId,
    /// Lexical generate-block prefix (`""` at module root, `"g[0]"` inside a generate loop).
    pub scope_prefix: String,
    /// Active genvar and generate-scope parameter constants visible at this item.
    pub gen_consts: Vec<(String, CVal)>,
    /// Source location.
    pub loc: Loc,
}

/// One elaborated signal, port, parameter, or genvar belonging to a specific [`Instance`].
#[derive(Clone, Debug)]
pub struct Symbol {
    /// Arena ID in [`ElaboratedDesign::symbols`].
    pub id: SymbolId,
    /// Owning elaborated instance.
    pub instance: InstanceId,
    /// Unprefixed identifier (`"x"` even when inside generate block `"g[0]"`).
    pub name: String,
    /// Path relative to `instance` (`"x"` or `"g[0].x"`).
    pub rel_path: String,
    /// Full hierarchical path from the top module (`"top.u0.g[0].x"`).
    pub full_path: String,
    /// Port, variable, net, parameter, or genvar classification.
    pub kind: SymbolKind,
    /// Elaborated type in this instance's parameterization.
    pub ty: RtlType,
    /// Explicit 1-D packed dimension bounds `(left, right)` when declared (`[7:0]` or `[0:7]`).
    pub packed_bounds: Option<(i128, i128)>,
    /// Declaring CST `(tree, node)`.
    pub decl_node: (usize, NodeId),
    /// Optional declaration initializer expression `(tree, node)` (`logic [3:0] q = 4'd1`).
    pub init_node: Option<(usize, NodeId)>,
    /// Generate scope prefix where `init_node` is evaluated.
    pub init_scope_prefix: String,
    /// Constant value (always set when evaluable for `Param`/`Genvar`, and when constant for `init_node`).
    pub cval: Option<CVal>,
    /// Source location (`1`-based line/column, indexing [`ElaboratedDesign::files`]).
    pub loc: Loc,
}

/// Package or `$unit` compilation-unit scope exported for RTL expression binding and subroutine inlining.
#[derive(Clone, Debug, Default)]
pub struct PackageScope {
    /// Package name (`"$unit"` for the compilation-unit scope).
    pub name: String,
    /// Package imports `(pkg_name, item_or_none_for_wildcard)`.
    pub imports: Vec<(String, Option<String>)>,
    /// Value parameters and `localparam`s `(name, value, ty, loc)`.
    pub params: Vec<(String, CVal, RtlType, Loc)>,
    /// Enumerator constants `(name, value, enum_ty)`.
    pub enum_consts: Vec<(String, CVal, RtlType)>,
    /// Typedef aliases `(name, resolved_ty)`.
    pub typedefs: Vec<(String, RtlType)>,
    /// Package or unit `function`, `task`, `let`, `sequence`, and `property` declarations `(name, kind, tree, node, loc)`.
    pub subroutines: Vec<(String, ElabItemKind, usize, NodeId, Loc)>,
}

/// One elaborated module, interface, or program instance in [`ElaboratedDesign`].
#[derive(Clone, Debug)]
pub struct Instance {
    /// Arena ID in [`ElaboratedDesign::instances`].
    pub id: InstanceId,
    /// Enclosing parent instance (`None` for top-level roots).
    pub parent: Option<InstanceId>,
    /// Leaf instance name (`"top"`, `"u0"`, `"u_arr[1]"`).
    pub name: String,
    /// Path relative to `parent` (`"u0"` or `"g[0].u0"`; equals `name` for tops).
    pub rel_path: String,
    /// Full hierarchical path (`"top"`, `"top.g[0].u0"`).
    pub path: String,
    /// Module, interface, or program definition name.
    pub definition: String,
    /// `true` when `definition` is an `interface`.
    pub is_interface: bool,
    /// `true` when instantiated via a SystemVerilog `bind` directive.
    pub is_bound: bool,
    /// Definition CST `(tree, node)`.
    pub def_node: (usize, NodeId),
    /// Elaborated value parameters in declaration order.
    pub params: Vec<(String, Option<CVal>)>,
    /// Elaborated type-parameter names mapped to their resolved [`RtlType`].
    pub type_params: Vec<(String, RtlType)>,
    /// Elaborated `typedef`s visible in this instance: `(scope_prefix, alias_name, resolved_ty)`.
    pub typedefs: Vec<(String, String, RtlType)>,
    /// Package imports visible in this instance: `(scope_prefix, pkg_name, item_or_wildcard)`.
    pub imports: Vec<(String, String, Option<String>)>,
    /// Formal port symbols in header order.
    pub ports: Vec<SymbolId>,
    /// Port connections specified at the instantiation site in `parent`.
    pub port_conns: Vec<PortConn>,
    /// All symbols owned by this instance (ports, vars, nets, params, genvars).
    pub symbols: Vec<SymbolId>,
    /// Enumerator constants visible in this instance: `(scope_prefix, name, value, enum_type)`.
    pub enum_consts: Vec<(String, String, CVal, RtlType)>,
    /// Interface modport declarations: `(modport_name, Vec<(signal_name, dir)>)`.
    pub modports: Vec<(String, Vec<(String, PortDir)>)>,
    /// Active procedural, continuous-assign, SVA, subroutine, and clocking items in source/unroll order.
    pub items: Vec<ElabItem>,
    /// Per-instance `function` and `task` declarations `(scope_prefix, name, kind, tree, node)`.
    pub(crate) subroutines: Vec<(String, String, ElabItemKind, usize, NodeId)>,
    /// Direct child instances in elaboration order.
    pub children: Vec<InstanceId>,
    /// Source location of the instantiation (or module header for tops).
    pub loc: Loc,
    /// Fast lookup from `rel_path` (`"x"` or `"g[0].x"`) to [`SymbolId`].
    pub(crate) sym_by_rel: FxMap<String, SymbolId>,
}

impl Instance {
    /// Looks up a symbol by exact `rel_path` (`"q"` or `"g[0].q"`).
    pub fn symbol(&self, rel_path: &str) -> Option<SymbolId> {
        self.sym_by_rel.get(rel_path).copied()
    }

    /// Resolves an unqualified identifier `ident` from lexical generate scope `scope_prefix`
    /// (`"g[0].inner"` -> `"g[0].inner.ident"`, `"g[0].ident"`, `"ident"`).
    pub fn resolve_lexical(&self, scope_prefix: &str, ident: &str) -> Option<SymbolId> {
        let mut prefix = scope_prefix;
        loop {
            let cand = if prefix.is_empty() { ident.to_string() } else { format!("{prefix}.{ident}") };
            if let Some(&id) = self.sym_by_rel.get(&cand) {
                return Some(id);
            }
            if prefix.is_empty() {
                return None;
            }
            prefix = match prefix.rfind('.') {
                Some(i) => &prefix[..i],
                None => "",
            };
        }
    }

    /// Resolves an unqualified enum constant `ident` from lexical generate scope `scope_prefix`.
    pub fn resolve_enum(&self, scope_prefix: &str, ident: &str) -> Option<(&CVal, &RtlType)> {
        let mut prefix = scope_prefix;
        loop {
            if let Some((_, _, v, ty)) = self.enum_consts.iter().find(|(sp, n, _, _)| sp == prefix && n == ident) {
                return Some((v, ty));
            }
            if prefix.is_empty() {
                return None;
            }
            prefix = match prefix.rfind('.') {
                Some(i) => &prefix[..i],
                None => "",
            };
        }
    }

    /// Resolves a `typedef` or `type` parameter name `ident` from lexical generate scope `scope_prefix`.
    pub fn resolve_typedef(&self, scope_prefix: &str, ident: &str) -> Option<&RtlType> {
        let mut prefix = scope_prefix;
        loop {
            if let Some((_, _, ty)) = self.typedefs.iter().find(|(sp, n, _)| sp == prefix && n == ident) {
                return Some(ty);
            }
            if prefix.is_empty() {
                break;
            }
            prefix = match prefix.rfind('.') {
                Some(i) => &prefix[..i],
                None => "",
            };
        }
        self.type_params.iter().find(|(n, _)| n == ident).map(|(_, ty)| ty)
    }
}

/// Signal summary entry for hierarchical design browsers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeSignal {
    /// Local signal name (`"clk"`, `"q"`).
    pub name: String,
    /// Full hierarchical path (`"top.u0.q"`).
    pub path: String,
    /// Bit width (`0` for unpacked memories or non-bit-vector objects).
    pub width: u32,
    /// Signedness.
    pub signed: bool,
    /// Four-state (`logic`/`reg`) vs two-state (`bit`/`int`).
    pub four_state: bool,
    /// `"port"`, `"var"`, `"net"`, `"param"`, or `"genvar"`.
    pub kind: &'static str,
    /// Port direction when `kind == "port"`.
    pub dir: Option<PortDir>,
}

/// Hierarchical scope browser node representing a module instance, interface instance, or generate scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeNode {
    /// Full hierarchical path (`"top"`, `"top.g[0]"`, `"top.g[0].u0"`).
    pub path: String,
    /// Local scope name (`"top"`, `"g[0]"`, `"u0"`).
    pub name: String,
    /// Module/interface definition name, or `"$generate"` for a generate scope.
    pub module: String,
    /// Signals declared directly in this scope.
    pub signals: Vec<ScopeSignal>,
    /// Nested generate scopes and child module/interface instances.
    pub children: Vec<ScopeNode>,
}

/// Complete arena-backed elaborated design across all top modules.
#[derive(Clone, Debug, Default)]
pub struct ElaboratedDesign {
    /// File path table indexed by [`Loc::file`] (`files[0]` is `""`).
    pub files: Vec<String>,
    /// Top-level elaborated instances in sorted order.
    pub tops: Vec<InstanceId>,
    /// Arena of all elaborated instances (`InstanceId(i)` indexes `instances[i]`).
    pub instances: Vec<Instance>,
    /// Arena of all elaborated per-instance symbols (`SymbolId(i)` indexes `symbols[i]`).
    pub symbols: Vec<Symbol>,
    /// Elaborated `package` scopes keyed by package name.
    pub packages: BTreeMap<String, PackageScope>,
    /// Compilation-unit `$unit` scope (constants, typedefs, subroutines, imports).
    pub unit_scope: PackageScope,
    /// Fast lookup from full instance path (`"top.u0"`) to [`InstanceId`].
    pub(crate) inst_by_path: FxMap<String, InstanceId>,
    /// Fast lookup from full symbol path (`"top.u0.sig"`) to [`SymbolId`].
    pub(crate) sym_by_path: FxMap<String, SymbolId>,
}

impl ElaboratedDesign {
    /// Borrows an [`Instance`] by [`InstanceId`].
    ///
    /// # Panics
    ///
    /// Panics if `id` is out of bounds for [`Self::instances`].
    pub fn instance(&self, id: InstanceId) -> &Instance {
        &self.instances[id.0 as usize]
    }

    /// Borrows a [`Symbol`] by [`SymbolId`].
    ///
    /// # Panics
    ///
    /// Panics if `id` is out of bounds for [`Self::symbols`].
    pub fn symbol(&self, id: SymbolId) -> &Symbol {
        &self.symbols[id.0 as usize]
    }

    /// Looks up an elaborated instance by full hierarchical path (`"top.u0"`).
    pub fn instance_by_path(&self, path: &str) -> Option<InstanceId> {
        self.inst_by_path.get(path).copied()
    }

    /// Looks up an elaborated symbol by full hierarchical path (`"top.u0.q"` or `"top.g[0].x"`).
    pub fn symbol_by_path(&self, path: &str) -> Option<SymbolId> {
        self.sym_by_path.get(path).copied()
    }

    /// Resolves `pkg::ident` (or `$unit::ident`) to a parameter or enum constant.
    pub fn resolve_scoped_const(&self, pkg_name: &str, ident: &str) -> Option<(&CVal, &RtlType)> {
        let p = if pkg_name == "$unit" { &self.unit_scope } else { self.packages.get(pkg_name)? };
        if let Some((_, cv, ty, _)) = p.params.iter().find(|(n, _, _, _)| n == ident) {
            return Some((cv, ty));
        }
        if let Some((_, cv, ty)) = p.enum_consts.iter().find(|(n, _, _)| n == ident) {
            return Some((cv, ty));
        }
        // Transitive explicit re-imports inside the package.
        for (imp_pkg, imp_item) in &p.imports {
            if imp_item.as_deref().is_none_or(|x| x == ident) && imp_pkg != pkg_name {
                if let Some(res) = self.resolve_scoped_const(imp_pkg, ident) {
                    return Some(res);
                }
            }
        }
        None
    }

    /// Resolves an unqualified constant `ident` via `import pkg::*` / `import pkg::ident` or `$unit`.
    pub fn resolve_imported_const(
        &self,
        inst_id: InstanceId,
        scope_prefix: &str,
        ident: &str,
    ) -> Option<(&CVal, &RtlType)> {
        let inst = self.instance(inst_id);
        let mut prefix = scope_prefix;
        loop {
            for (sp, pkg, item) in &inst.imports {
                if sp == prefix && item.as_deref().is_none_or(|x| x == ident) {
                    if let Some(res) = self.resolve_scoped_const(pkg, ident) {
                        return Some(res);
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
        if let Some(res) = self.resolve_scoped_const("$unit", ident) {
            return Some(res);
        }
        for (pkg, item) in &self.unit_scope.imports {
            if item.as_deref().is_none_or(|x| x == ident) {
                if let Some(res) = self.resolve_scoped_const(pkg, ident) {
                    return Some(res);
                }
            }
        }
        None
    }

    /// Resolves a named type (`typedef` or `type` parameter), optionally qualified as `pkg::T`.
    pub fn resolve_named_type(
        &self,
        inst_id: InstanceId,
        scope_prefix: &str,
        pkg: Option<&str>,
        name: &str,
    ) -> Option<&RtlType> {
        if let Some(pkg_name) = pkg {
            let p = if pkg_name == "$unit" { &self.unit_scope } else { self.packages.get(pkg_name)? };
            return p.typedefs.iter().find(|(n, _)| n == name).map(|(_, ty)| ty);
        }
        let inst = self.instance(inst_id);
        if let Some(ty) = inst.resolve_typedef(scope_prefix, name) {
            return Some(ty);
        }
        let mut prefix = scope_prefix;
        loop {
            for (sp, imp_pkg, item) in &inst.imports {
                if sp == prefix && item.as_deref().is_none_or(|x| x == name) {
                    if let Some(p) = self.packages.get(imp_pkg) {
                        if let Some((_, ty)) = p.typedefs.iter().find(|(n, _)| n == name) {
                            return Some(ty);
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
        if let Some((_, ty)) = self.unit_scope.typedefs.iter().find(|(n, _)| n == name) {
            return Some(ty);
        }
        for (imp_pkg, item) in &self.unit_scope.imports {
            if item.as_deref().is_none_or(|x| x == name) {
                if let Some(p) = self.packages.get(imp_pkg) {
                    if let Some((_, ty)) = p.typedefs.iter().find(|(n, _)| n == name) {
                        return Some(ty);
                    }
                }
            }
        }
        None
    }

    /// Resolves a `function` or `task` call target to `(kind, tree, node, owning_instance, scope_prefix)`.
    ///
    /// When `pkg` is `None`, searches local declarations and package imports together at each
    /// lexical `scope_prefix` level from the innermost generate block outward to the module root,
    /// followed by `$unit` declarations and `$unit` imports. `owning_instance` is `Some(inst_id)`
    /// for instance-local subroutines and `None` for package or `$unit` subroutines.
    pub fn resolve_subroutine(
        &self,
        inst_id: InstanceId,
        scope_prefix: &str,
        pkg: Option<&str>,
        name: &str,
    ) -> Option<(ElabItemKind, usize, NodeId, Option<InstanceId>, String)> {
        let is_sub = |k: ElabItemKind| matches!(k, ElabItemKind::FunctionDecl | ElabItemKind::TaskDecl);
        if let Some(pkg_name) = pkg {
            let p = if pkg_name == "$unit" { &self.unit_scope } else { self.packages.get(pkg_name)? };
            if let Some((_, k, t, n, _)) = p.subroutines.iter().find(|(nm, k, _, _, _)| is_sub(*k) && nm == name) {
                return Some((*k, *t, *n, None, String::new()));
            }
            return None;
        }
        let inst = self.instance(inst_id);
        let mut prefix = scope_prefix;
        loop {
            if let Some((sp, _, k, t, n)) = inst.subroutines.iter().find(|(sp, nm, _, _, _)| sp == prefix && nm == name)
            {
                return Some((*k, *t, *n, Some(inst_id), sp.clone()));
            }
            for (sp, imp_pkg, imp_item) in &inst.imports {
                if sp == prefix && imp_item.as_deref().is_none_or(|x| x == name) {
                    if let Some(p) = self.packages.get(imp_pkg) {
                        if let Some((_, k, t, n, _)) =
                            p.subroutines.iter().find(|(nm, k, _, _, _)| is_sub(*k) && nm == name)
                        {
                            return Some((*k, *t, *n, None, String::new()));
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
        if let Some((_, k, t, n, _)) =
            self.unit_scope.subroutines.iter().find(|(nm, k, _, _, _)| is_sub(*k) && nm == name)
        {
            return Some((*k, *t, *n, None, String::new()));
        }
        for (imp_pkg, imp_item) in &self.unit_scope.imports {
            if imp_item.as_deref().is_none_or(|x| x == name) {
                if let Some(p) = self.packages.get(imp_pkg) {
                    if let Some((_, k, t, n, _)) =
                        p.subroutines.iter().find(|(nm, k, _, _, _)| is_sub(*k) && nm == name)
                    {
                        return Some((*k, *t, *n, None, String::new()));
                    }
                }
            }
        }
        None
    }

    /// Resolves a dotted hierarchical symbol path `raw_path` (`a.b.sig`) written inside `from_inst`
    /// at generate scope `scope_prefix`: tries relative to `from_inst` (innermost generate prefix
    /// outward), then upward through ancestor instances, then from `$root` (`tops`).
    pub fn resolve_hierarchical(&self, from_inst: InstanceId, scope_prefix: &str, raw_path: &str) -> Option<SymbolId> {
        let clean = raw_path.strip_prefix("$root.").unwrap_or(raw_path);
        if raw_path.starts_with("$root.") {
            return self.symbol_by_path(clean);
        }
        // 1. Relative to current instance (walking generate prefixes outward).
        let inst = self.instance(from_inst);
        let mut prefix = scope_prefix;
        loop {
            let full = if prefix.is_empty() {
                format!("{}.{clean}", inst.path)
            } else {
                format!("{}.{prefix}.{clean}", inst.path)
            };
            if let Some(sym) = self.symbol_by_path(&full) {
                return Some(sym);
            }
            if prefix.is_empty() {
                break;
            }
            prefix = match prefix.rfind('.') {
                Some(i) => &prefix[..i],
                None => "",
            };
        }
        // 2. Relative to enclosing ancestor instances (including upward module-name match).
        let mut cur = Some(from_inst);
        while let Some(cid) = cur {
            let c = self.instance(cid);
            if let Some(rest) = clean
                .strip_prefix(&format!("{}.", c.name))
                .or_else(|| clean.strip_prefix(&format!("{}.", c.definition)))
            {
                if let Some(sym) = self.symbol_by_path(&format!("{}.{rest}", c.path)) {
                    return Some(sym);
                }
            }
            if let Some(sym) = self.symbol_by_path(&format!("{}.{clean}", c.path)) {
                return Some(sym);
            }
            cur = c.parent;
        }
        // 3. Rooted at a top module (`top.u0.sig`).
        self.symbol_by_path(clean)
    }

    /// Resolves a dotted hierarchical path to a child [`InstanceId`] (e.g. an interface instance
    /// `bus_i` or `g[0].bus_i` referenced inside `from_inst`).
    pub fn resolve_instance(&self, from_inst: InstanceId, scope_prefix: &str, raw_path: &str) -> Option<InstanceId> {
        let clean = raw_path.strip_prefix("$root.").unwrap_or(raw_path);
        if raw_path.starts_with("$root.") {
            return self.instance_by_path(clean);
        }
        let inst = self.instance(from_inst);
        let mut prefix = scope_prefix;
        loop {
            let full = if prefix.is_empty() {
                format!("{}.{clean}", inst.path)
            } else {
                format!("{}.{prefix}.{clean}", inst.path)
            };
            if let Some(id) = self.instance_by_path(&full) {
                return Some(id);
            }
            if prefix.is_empty() {
                break;
            }
            prefix = match prefix.rfind('.') {
                Some(i) => &prefix[..i],
                None => "",
            };
        }
        self.instance_by_path(clean)
    }

    /// Builds a nested [`ScopeNode`] browser tree for every top module, expanding generate
    /// block prefixes (`g[0]`) and child instances into hierarchical scopes.
    pub fn scope_tree(&self) -> Vec<ScopeNode> {
        self.tops.iter().map(|&t| self.build_scope_node(t)).collect()
    }

    fn build_scope_node(&self, inst_id: InstanceId) -> ScopeNode {
        let inst = self.instance(inst_id);
        let mut root = ScopeNode {
            path: inst.path.clone(),
            name: inst.name.clone(),
            module: inst.definition.clone(),
            signals: Vec::new(),
            children: Vec::new(),
        };
        let mut gen_map: BTreeMap<String, ScopeNode> = BTreeMap::new();
        for &sid in &inst.symbols {
            let sym = self.symbol(sid);
            let (kind, dir) = match &sym.kind {
                SymbolKind::Port { dir, .. } => ("port", Some(*dir)),
                SymbolKind::Var => ("var", None),
                SymbolKind::Net { .. } => ("net", None),
                SymbolKind::Param { .. } => ("param", None),
                SymbolKind::Genvar => ("genvar", None),
            };
            let sig = ScopeSignal {
                name: sym.name.clone(),
                path: sym.full_path.clone(),
                width: sym.ty.bit_width().unwrap_or(0),
                signed: sym.ty.is_signed(),
                four_state: sym.ty.is_four_state(),
                kind,
                dir,
            };
            match sym.rel_path.rsplit_once('.') {
                None => root.signals.push(sig),
                Some((gen_prefix, _)) => {
                    ensure_gen_scope(&inst.path, gen_prefix, &mut gen_map).signals.push(sig);
                }
            }
        }
        for &cid in &inst.children {
            let child_inst = self.instance(cid);
            let child_node = self.build_scope_node(cid);
            match child_inst.rel_path.rsplit_once('.') {
                None => root.children.push(child_node),
                Some((gen_prefix, _)) => {
                    ensure_gen_scope(&inst.path, gen_prefix, &mut gen_map).children.push(child_node);
                }
            }
        }
        for item in &inst.items {
            if !item.scope_prefix.is_empty() {
                ensure_gen_scope(&inst.path, &item.scope_prefix, &mut gen_map);
            }
        }
        let leaf_keys: Vec<String> = gen_map.keys().cloned().collect();
        for lk in &leaf_keys {
            let mut cur = lk.as_str();
            while let Some((parent_k, _)) = cur.rsplit_once('.') {
                ensure_gen_scope(&inst.path, parent_k, &mut gen_map);
                cur = parent_k;
            }
        }
        // Fold flat `gen_map` entries (`"g[0]"`, `"g[0].inner"`) into nested `ScopeNode`s,
        // attaching deepest scopes first while preserving ascending sibling order.
        let mut keys: Vec<String> = gen_map.keys().cloned().collect();
        keys.sort_by(|a, b| {
            let da = a.matches('.').count();
            let db = b.matches('.').count();
            db.cmp(&da).then_with(|| cmp_scope_key_natural(a, b))
        });
        for k in keys {
            let Some(node) = gen_map.remove(&k) else { continue };
            match k.rsplit_once('.') {
                Some((parent_k, _)) => {
                    ensure_gen_scope(&inst.path, parent_k, &mut gen_map).children.push(node);
                }
                None => root.children.push(node),
            }
        }
        root
    }
}

fn cmp_scope_seg_natural(a: &str, b: &str) -> std::cmp::Ordering {
    if let (Some((ab, ai)), Some((bb, bi))) = (split_indexed_seg(a), split_indexed_seg(b)) {
        return ab.cmp(bb).then_with(|| ai.cmp(&bi));
    }
    a.cmp(b)
}

fn split_indexed_seg(seg: &str) -> Option<(&str, i128)> {
    if let Some(base) = seg.strip_suffix(']') {
        let (name, idx_str) = base.rsplit_once('[')?;
        let idx = idx_str.parse::<i128>().ok()?;
        return Some((name, idx));
    }
    let digits_start = seg.rfind(|c: char| !c.is_ascii_digit()).map_or(0, |i| i + 1);
    if digits_start > 0 && digits_start < seg.len() {
        let idx = seg[digits_start..].parse::<i128>().ok()?;
        return Some((&seg[..digits_start], idx));
    }
    None
}

fn cmp_scope_key_natural(a: &str, b: &str) -> std::cmp::Ordering {
    let mut ia = a.split('.');
    let mut ib = b.split('.');
    loop {
        match (ia.next(), ib.next()) {
            (Some(sa), Some(sb)) => {
                let ord = cmp_scope_seg_natural(sa, sb);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (None, None) => return std::cmp::Ordering::Equal,
        }
    }
}

fn ensure_gen_scope<'a>(
    inst_path: &str,
    gen_prefix: &str,
    map: &'a mut BTreeMap<String, ScopeNode>,
) -> &'a mut ScopeNode {
    let mut acc = String::new();
    for seg in gen_prefix.split('.') {
        if !acc.is_empty() {
            acc.push('.');
        }
        acc.push_str(seg);
        let key = acc.clone();
        map.entry(key.clone()).or_insert_with(|| ScopeNode {
            path: format!("{inst_path}.{key}"),
            name: seg.to_string(),
            module: "$generate".into(),
            signals: Vec::new(),
            children: Vec::new(),
        });
    }
    map.get_mut(gen_prefix).expect("inserted above")
}

// ---- Builder -> RtlType resolution ------------------------------------------

impl Builder<'_> {
    /// Resolves a CST declaration `(dtype, declarator)` in `(scope, env)` (with `self.inst`
    /// already populated with the active `Vals`) into an [`RtlType`].
    pub(crate) fn rtl_type(
        &self,
        dt: Option<NodeRef>,
        item: Option<NodeRef>,
        scope: usize,
        env: &Env,
        depth: usize,
    ) -> RtlType {
        if depth > 24 {
            return RtlType::NonSynthesizable("recursive type".into());
        }
        // 1. Unpacked dimensions on the declarator (`logic [7:0] mem [0:15]`).
        if let Some(decl_item) = item {
            let udims: Vec<NodeRef> =
                self.children(decl_item).into_iter().filter(|c| self.kind(*c) == SyntaxKind::Dim).collect();
            if !udims.is_empty() {
                let elem = self.rtl_type(dt, None, scope, env, depth + 1);
                return self.wrap_unpacked_dims(elem, &udims, scope, env);
            }
        }
        let Some(dt) = dt else {
            // Implicit type (`wire w`, `input a`): 1-bit unsigned 4-state logic.
            return RtlType::Integral { width: 1, signed: false, four_state: true };
        };
        self.rtl_data_type(dt, scope, env, None, depth)
    }

    fn wrap_unpacked_dims(&self, elem: RtlType, udims: &[NodeRef], scope: usize, env: &Env) -> RtlType {
        if let RtlType::NonSynthesizable(_) = &elem {
            return elem;
        }
        let mut total_elems: u32 = match &elem {
            RtlType::UnpackedArray { dims, .. } => {
                let mut acc = 1u32;
                for &(l, r) in dims {
                    let Some(c) = crate::types::range_len_u32(l, r, crate::types::MAX_ARRAY_ELEMS) else {
                        return RtlType::NonSynthesizable("unpacked array exceeds maximum element limit".into());
                    };
                    let Some(next) = acc.checked_mul(c).filter(|&v| v <= crate::types::MAX_ARRAY_ELEMS) else {
                        return RtlType::NonSynthesizable("unpacked array exceeds maximum element limit".into());
                    };
                    acc = next;
                }
                acc
            }
            _ => 1,
        };
        let mut bounds = Vec::with_capacity(udims.len());
        for &d in udims {
            let toks = self.toks_of(d);
            let inner: Vec<&str> = toks.iter().copied().filter(|t| !matches!(*t, "[" | "]")).collect();
            if inner.is_empty() {
                return RtlType::NonSynthesizable("dynamic array".into());
            }
            if inner.contains(&"$") {
                return RtlType::NonSynthesizable("queue".into());
            }
            if inner == ["*"] || self.child(d, SyntaxKind::DataType).is_some() {
                return RtlType::NonSynthesizable("associative array".into());
            }
            if self.dim_is_error(d, scope, env, true) {
                return RtlType::NonSynthesizable("invalid unpacked array dimension".into());
            }
            let Some((l, r)) = self.dim_range(d, scope, env) else {
                return RtlType::NonSynthesizable("unpacked array with non-constant bounds".into());
            };
            let Some(cnt) = crate::types::range_len_u32(l, r, crate::types::MAX_ARRAY_ELEMS) else {
                return RtlType::NonSynthesizable("unpacked array exceeds maximum element limit".into());
            };
            let Some(next) = total_elems.checked_mul(cnt).filter(|&v| v <= crate::types::MAX_ARRAY_ELEMS) else {
                return RtlType::NonSynthesizable("unpacked array exceeds maximum element limit".into());
            };
            total_elems = next;
            bounds.push((l, r));
        }
        match elem {
            RtlType::UnpackedArray { elem: inner_elem, dims: mut inner_dims } => {
                bounds.append(&mut inner_dims);
                RtlType::UnpackedArray { elem: inner_elem, dims: bounds }
            }
            other => RtlType::UnpackedArray { elem: Box::new(other), dims: bounds },
        }
    }

    pub(crate) fn rtl_data_type(
        &self,
        dt: NodeRef,
        scope: usize,
        env: &Env,
        alias: Option<String>,
        depth: usize,
    ) -> RtlType {
        if depth > 24 {
            return RtlType::NonSynthesizable("recursive type".into());
        }
        let own = self.own_tokens(dt);
        let pdims: Vec<NodeRef> = self.children(dt).into_iter().filter(|c| self.kind(*c) == SyntaxKind::Dim).collect();

        // Non-synthesizable scalar keywords.
        if let Some(&kw) = own.first() {
            match kw {
                "real" | "shortreal" | "realtime" | "string" | "chandle" => {
                    return RtlType::NonSynthesizable(kw.into());
                }
                "virtual" => return RtlType::NonSynthesizable("virtual interface".into()),
                "event" => return RtlType::Event,
                "void" => return RtlType::Void,
                _ => {}
            }
        }

        // `enum [base_type] { ... } [pdims]`
        if own.first() == Some(&"enum") {
            let base = self.child(dt, SyntaxKind::DataType);
            let (width, signed, four_state) = match base {
                Some(b) => {
                    let bty = self.rtl_data_type(b, scope, env, None, depth + 1);
                    if let RtlType::NonSynthesizable(r) = bty {
                        return RtlType::NonSynthesizable(r);
                    }
                    let Some(shape) = bty.integral_shape() else {
                        return RtlType::NonSynthesizable("non-integral enum base type".into());
                    };
                    shape
                }
                None => (32, true, false),
            };
            let variants = self.enum_values(dt, scope, env, depth + 1);
            let ety = RtlType::Enum { name: alias, width, signed, four_state, variants };
            return self.wrap_packed_dims(ety, &pdims, scope, env);
        }

        // `struct` / `union` (packed or unpacked)
        if matches!(own.first(), Some(&"struct" | &"union")) {
            let is_union = own.first() == Some(&"union");
            let is_packed = own.contains(&"packed");
            let signed = own.contains(&"signed");
            if is_packed {
                let mut raw_members: Vec<(String, RtlType, u32)> = Vec::new();
                for m in self.children(dt) {
                    if self.kind(m) != SyntaxKind::DataDecl {
                        continue;
                    }
                    let mdt = self.child(m, SyntaxKind::DataType);
                    for d in self.children(m) {
                        if self.kind(d) == SyntaxKind::Declarator {
                            let Some((fname, _)) = self.name_of(d) else { continue };
                            let fty = self.rtl_type(mdt, Some(d), scope, env, depth + 1);
                            if let RtlType::NonSynthesizable(r) = fty {
                                return RtlType::NonSynthesizable(r);
                            }
                            let Some(w) = fty.bit_width() else {
                                return RtlType::NonSynthesizable(format!(
                                    "unpacked member '{fname}' inside packed struct or union"
                                ));
                            };
                            raw_members.push((fname, fty, w));
                        }
                    }
                }
                let total = if is_union {
                    raw_members.iter().map(|m| m.2).max().unwrap_or(1)
                } else {
                    let mut sum = 0u32;
                    for &(_, _, w) in &raw_members {
                        let Some(next) = sum.checked_add(w).filter(|&tw| tw <= crate::types::MAX_BIT_WIDTH) else {
                            return RtlType::NonSynthesizable("packed struct exceeds maximum bit width".into());
                        };
                        sum = next;
                    }
                    sum.max(1)
                };
                let four_state = raw_members.iter().any(|(_, t, _)| t.is_four_state());
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
                    RtlType::PackedUnion { name: alias, width: total, signed, four_state, fields }
                } else {
                    RtlType::PackedStruct { name: alias, width: total, signed, four_state, fields }
                };
                return self.wrap_packed_dims(sty, &pdims, scope, env);
            }
            // Unpacked struct / union.
            let mut fields = Vec::new();
            for m in self.children(dt) {
                if self.kind(m) != SyntaxKind::DataDecl {
                    continue;
                }
                let mdt = self.child(m, SyntaxKind::DataType);
                for d in self.children(m) {
                    if self.kind(d) == SyntaxKind::Declarator {
                        if let Some((fname, _)) = self.name_of(d) {
                            let fty = self.rtl_type(mdt, Some(d), scope, env, depth + 1);
                            fields.push((fname, fty));
                        }
                    }
                }
            }
            return RtlType::UnpackedStruct { name: alias, is_union, fields };
        }

        // Built-in integer / vector keywords (`logic`, `bit`, `reg`, `int`, `byte`, ...)
        // or implicit vector (`[7:0]`, `signed [7:0]`).
        let kw = own.iter().find(|t| crate::types::INT_TYPES.contains(t)).copied();
        let implicit = kw.is_none()
            && own.iter().all(|t| matches!(*t, "signed" | "unsigned" | "parameter" | "localparam"))
            && (!own.is_empty() || !pdims.is_empty());
        if kw.is_some() || implicit {
            let four_state = !matches!(kw, Some("bit" | "int" | "byte" | "shortint" | "longint"));
            let default_signed = matches!(kw, Some("int" | "integer" | "byte" | "shortint" | "longint"));
            let signed = if own.contains(&"unsigned") {
                false
            } else if own.contains(&"signed") {
                true
            } else {
                default_signed
            };
            let base_w: u32 = match kw {
                Some("int" | "integer") => 32,
                Some("byte") => 8,
                Some("shortint") => 16,
                Some("longint" | "time") => 64,
                _ => 1,
            };
            // Single-dimension vector (`logic [7:0]`) is a plain `Integral`; multi-dimensional
            // (`logic [3:0][7:0]`) wraps outer dimensions as `PackedArray` so `x[i]` yields `logic [7:0]`.
            if pdims.is_empty() {
                return RtlType::Integral { width: base_w, signed, four_state };
            }
            if base_w == 1 && pdims.len() == 1 {
                if self.dim_is_error(pdims[0], scope, env, false) {
                    return RtlType::NonSynthesizable("invalid packed dimension".into());
                }
                let Some((l, r)) = self.dim_range(pdims[0], scope, env) else {
                    return RtlType::NonSynthesizable("packed dimension with non-constant bounds".into());
                };
                let Some(width) = crate::types::range_len_u32(l, r, crate::types::MAX_BIT_WIDTH) else {
                    return RtlType::NonSynthesizable("packed vector exceeds maximum bit width".into());
                };
                return RtlType::Integral { width, signed, four_state };
            }
            let inner_start = if base_w == 1 {
                let last = pdims[pdims.len() - 1];
                if self.dim_is_error(last, scope, env, false) {
                    return RtlType::NonSynthesizable("invalid packed dimension".into());
                }
                let Some((l, r)) = self.dim_range(last, scope, env) else {
                    return RtlType::NonSynthesizable("packed dimension with non-constant bounds".into());
                };
                let Some(w) = crate::types::range_len_u32(l, r, crate::types::MAX_BIT_WIDTH) else {
                    return RtlType::NonSynthesizable("packed vector exceeds maximum bit width".into());
                };
                let inner = RtlType::Integral { width: w, signed: false, four_state };
                let mut wrapped = self.wrap_packed_dims(inner, &pdims[..pdims.len() - 1], scope, env);
                if signed {
                    if let RtlType::PackedArray { signed: s, .. } = &mut wrapped {
                        *s = true;
                    }
                }
                return wrapped;
            } else {
                let inner = RtlType::Integral { width: base_w, signed: false, four_state };
                self.wrap_packed_dims(inner, &pdims, scope, env)
            };
            return inner_start;
        }

        // Named type: type parameter, typedef, interface, or class.
        let toks = self.toks_of(dt);
        if toks.is_empty() {
            return RtlType::Integral { width: 1, signed: false, four_state: true };
        }
        let head: Vec<&str> = toks.iter().take_while(|t| **t != "[").copied().collect();
        if let [tname] = head.as_slice() {
            if let Some(bd) = env.get(*tname) {
                let base = self.rtl_data_type(bd.node, bd.scope, &bd.env, Some((*tname).to_string()), depth + 1);
                return self.wrap_packed_dims(base, &pdims, scope, env);
            }
            let mut cur = Some(scope);
            while let Some(c) = cur {
                if let Some(Some(def_dt)) = self.scopes[c].type_params.get(*tname) {
                    let base = self.rtl_data_type(*def_dt, c, env, Some((*tname).to_string()), depth + 1);
                    return self.wrap_packed_dims(base, &pdims, scope, env);
                }
                cur = self.scopes[c].parent;
            }
            if self.interfaces.contains_key(*tname) {
                return RtlType::Interface { def: (*tname).to_string(), modport: None };
            }
            if let Some(td) = self.block_typedef(tname, dt) {
                let udims: Vec<NodeRef> =
                    self.children(td).into_iter().filter(|c| self.kind(*c) == SyntaxKind::Dim).collect();
                let base = match self.child(td, SyntaxKind::DataType) {
                    Some(inner_dt) => self.rtl_data_type(inner_dt, scope, env, Some((*tname).to_string()), depth + 1),
                    None => RtlType::NonSynthesizable(format!("unresolved forward typedef '{tname}'")),
                };
                let with_udims =
                    if udims.is_empty() { base } else { self.wrap_unpacked_dims(base, &udims, scope, env) };
                return self.wrap_packed_dims(with_udims, &pdims, scope, env);
            }
        }
        if let [iface, ".", mp] = head.as_slice() {
            if self.interfaces.contains_key(*iface) {
                return RtlType::Interface { def: (*iface).to_string(), modport: Some((*mp).to_string()) };
            }
        }
        if head.len() == 1 || head.contains(&"::") {
            match self.lookup_type_pub(dt, scope) {
                Some(Found::Typedef(td, td_scope)) => {
                    let tname = self.name_of(td).map(|x| x.0);
                    let udims: Vec<NodeRef> =
                        self.children(td).into_iter().filter(|c| self.kind(*c) == SyntaxKind::Dim).collect();
                    let base = match self.child(td, SyntaxKind::DataType) {
                        Some(inner_dt) => self.rtl_data_type(inner_dt, td_scope, env, tname, depth + 1),
                        None => RtlType::NonSynthesizable(format!(
                            "unresolved forward typedef '{}'",
                            tname.unwrap_or_default()
                        )),
                    };
                    let with_udims =
                        if udims.is_empty() { base } else { self.wrap_unpacked_dims(base, &udims, td_scope, env) };
                    let mut wrapped = self.wrap_packed_dims(with_udims, &pdims, scope, env);
                    if own.contains(&"signed") || own.contains(&"unsigned") {
                        let sg = own.contains(&"signed");
                        match &mut wrapped {
                            RtlType::Integral { signed, .. }
                            | RtlType::PackedStruct { signed, .. }
                            | RtlType::PackedUnion { signed, .. }
                            | RtlType::PackedArray { signed, .. }
                            | RtlType::Enum { signed, .. } => *signed = sg,
                            _ => {}
                        }
                    }
                    return wrapped;
                }
                Some(Found::Class(cid)) => {
                    return RtlType::NonSynthesizable(format!("class {}", self.classes[cid].name));
                }
                Some(Found::Param(Some(pdt), psc)) => {
                    let base = self.rtl_data_type(pdt, psc, env, None, depth + 1);
                    return self.wrap_packed_dims(base, &pdims, scope, env);
                }
                _ => {}
            }
        }
        if matches!(head.as_slice(), ["mailbox"] | ["semaphore"] | ["process"]) {
            return RtlType::NonSynthesizable(format!("class {}", head[0]));
        }
        let raw_name = self.tree(dt).bare_text(dt.node).trim().to_string();
        RtlType::NonSynthesizable(format!("unknown or unsupported type '{raw_name}'"))
    }

    fn wrap_packed_dims(&self, mut elem: RtlType, pdims: &[NodeRef], scope: usize, env: &Env) -> RtlType {
        if let RtlType::NonSynthesizable(_) = &elem {
            return elem;
        }
        for &d in pdims.iter().rev() {
            if self.dim_is_error(d, scope, env, false) {
                return RtlType::NonSynthesizable("invalid packed dimension".into());
            }
            let Some((left, right)) = self.dim_range(d, scope, env) else {
                return RtlType::NonSynthesizable("packed dimension with non-constant bounds".into());
            };
            let Some(count) = crate::types::range_len_u32(left, right, crate::types::MAX_BIT_WIDTH) else {
                return RtlType::NonSynthesizable("packed array exceeds maximum bit width".into());
            };
            let Some(ew) = elem.bit_width() else {
                return RtlType::NonSynthesizable("packed dimension applied to unpacked type".into());
            };
            let Some(width) = ew.checked_mul(count).filter(|&tw| tw <= crate::types::MAX_BIT_WIDTH) else {
                return RtlType::NonSynthesizable("packed array exceeds maximum bit width".into());
            };
            let four_state = elem.is_four_state();
            elem = RtlType::PackedArray { elem: Box::new(elem), left, right, width, signed: false, four_state };
        }
        elem
    }

    /// Returns `(left, right)` bounds when `dt` resolves to a 1-D packed vector (`logic [7:0]` or `logic [0:7]`).
    pub(crate) fn packed_1d_bounds(
        &self,
        dt: Option<NodeRef>,
        scope: usize,
        env: &Env,
        depth: usize,
    ) -> Option<(i128, i128)> {
        if depth > 16 {
            return None;
        }
        let dt = dt?;
        let pdims: Vec<NodeRef> = self.children(dt).into_iter().filter(|c| self.kind(*c) == SyntaxKind::Dim).collect();
        if pdims.len() == 1 {
            return self.dim_range(pdims[0], scope, env);
        }
        if !pdims.is_empty() {
            return None;
        }
        let toks = self.toks_of(dt);
        let head: Vec<&str> = toks.iter().take_while(|t| **t != "[").copied().collect();
        if let [tname] = head.as_slice() {
            if let Some(bd) = env.get(*tname) {
                return self.packed_1d_bounds(Some(bd.node), bd.scope, &bd.env, depth + 1);
            }
            if let Some(td) = self.block_typedef(tname, dt) {
                return self.packed_1d_bounds(self.child(td, SyntaxKind::DataType), scope, env, depth + 1);
            }
        }
        if head.len() == 1 || head.contains(&"::") {
            if let Some(Found::Typedef(td, td_scope)) = self.lookup_type_pub(dt, scope) {
                return self.packed_1d_bounds(self.child(td, SyntaxKind::DataType), td_scope, env, depth + 1);
            }
        }
        None
    }
}
