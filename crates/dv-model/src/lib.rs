//! The design model every DV tool reads: classes, their UVM role, properties
//! and methods, with source locations.
//!
//! A [`Design`] is an owned snapshot; nothing borrows from the front end
//! afterwards. Two front ends fill it:
//!
//! - the native one, [`load`] (what every tool uses by default), which builds
//!   the pure IEEE 1800 model via `sv-semantic` and classifies UVM roles and
//!   phases via [`Design::from_sv`];
//! - slang, through `Design::load` (feature `slang`, off by default; oracle
//!   and parity use only), which copies the C++ `uvmcheck::model` across the
//!   C ABI in `sv-slang`.
//!
//! Both take the same compile options as `uvmcheck`. Tools read only this
//! crate's types and do not see which front end built the design;
//! [`Design::dump`] prints the canonical form the parity gates compare.

#[cfg(feature = "slang")]
use std::ffi::{CStr, CString};
#[cfg(feature = "slang")]
use std::os::raw::c_char;
#[cfg(feature = "slang")]
use std::time::Instant;

#[cfg(feature = "slang")]
use sv_slang as ffi;

pub use svlang::semantic::model::{
    Argument, Assignment, Call, ClassId, Constraint, Direction, Error, Loc, Local, MacroUse, Property, RootKind,
    Timings, TypeShape, Typedef, Visibility,
};

/// Loads and models a UVM-classified [`Design`] from compile options.
pub fn load<S: AsRef<str>>(args: &[S]) -> Result<Design, Error> {
    svlang::semantic::load(args).map(Design::from_sv)
}

/// [`load`], also returning the parsed syntax trees (for syntax-level rules).
pub fn load_with_trees<S: AsRef<str>>(args: &[S]) -> Result<(Design, Vec<svlang::syntax::Tree>), Error> {
    svlang::semantic::load_with_trees(args).map(|(d, trees)| (Design::from_sv(d), trees))
}

/// Builds a [`Design`] from already-parsed syntax trees with timing metadata.
pub fn design_of(trees: &[svlang::syntax::Tree], parse_ms: f64) -> Design {
    Design::from_sv(svlang::semantic::design_of(trees, parse_ms))
}

/// Builds a [`Design`] directly from syntax trees.
pub fn build(trees: &[svlang::syntax::Tree]) -> Design {
    Design::from_sv(svlang::semantic::build(trees))
}

/// The UVM role of a class, from the nearest UVM base class it derives from.
/// Component roles come last, so [`UvmKind::is_component`] is one comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UvmKind {
    /// Not a UVM class.
    None,
    /// A `uvm_object` that is none of the roles below.
    Object,
    /// A `uvm_sequence_item`.
    SequenceItem,
    /// A `uvm_sequence` (or `uvm_sequence_base`).
    Sequence,
    /// A `uvm_component` that is none of the roles below.
    Component,
    /// A `uvm_test`.
    Test,
    /// A `uvm_env`.
    Env,
    /// A `uvm_agent`.
    Agent,
    /// A `uvm_driver`.
    Driver,
    /// A `uvm_monitor`.
    Monitor,
    /// A `uvm_sequencer` (or `uvm_sequencer_base`).
    Sequencer,
    /// A `uvm_scoreboard`.
    Scoreboard,
    /// A `uvm_subscriber`.
    Subscriber,
}

impl UvmKind {
    #[cfg(feature = "slang")]
    fn from_c(v: u8) -> Self {
        use UvmKind::*;
        [
            None,
            Object,
            SequenceItem,
            Sequence,
            Component,
            Test,
            Env,
            Agent,
            Driver,
            Monitor,
            Sequencer,
            Scoreboard,
            Subscriber,
        ]
        .get(v as usize)
        .copied()
        .unwrap_or(None)
    }

    /// Any `uvm_component`, whatever its role.
    pub fn is_component(self) -> bool {
        self as u8 >= UvmKind::Component as u8
    }

    /// The role's name as dumps and JSON print it (`"sequence_item"`).
    pub fn as_str(self) -> &'static str {
        use UvmKind::*;
        match self {
            None => "none",
            Object => "object",
            SequenceItem => "sequence_item",
            Sequence => "sequence",
            Component => "component",
            Test => "test",
            Env => "env",
            Agent => "agent",
            Driver => "driver",
            Monitor => "monitor",
            Sequencer => "sequencer",
            Scoreboard => "scoreboard",
            Subscriber => "subscriber",
        }
    }
}

/// Whether a method is one of the standard UVM phase methods, and which kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaseKind {
    /// Not a phase method.
    None,
    /// A function phase (`build_phase`, `connect_phase`, `report_phase`, ...).
    Function,
    /// A task phase (`run_phase`, `main_phase`, ...).
    Task,
}

impl PhaseKind {
    /// Classifies a method name against standard IEEE 1800.2 UVM phases.
    pub fn from_method_name(name: &str) -> Self {
        if FUNCTION_PHASES.contains(&name) {
            PhaseKind::Function
        } else if TASK_PHASES.contains(&name) {
            PhaseKind::Task
        } else {
            PhaseKind::None
        }
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

const TASK_PHASES: &[&str] = &[
    "run_phase",
    "pre_reset_phase",
    "reset_phase",
    "post_reset_phase",
    "pre_configure_phase",
    "configure_phase",
    "post_configure_phase",
    "pre_main_phase",
    "main_phase",
    "post_main_phase",
    "pre_shutdown_phase",
    "shutdown_phase",
    "post_shutdown_phase",
];

/// Classifies the UVM role from the names along a class's resolved base chain.
fn classify_uvm(chain: &[&str]) -> UvmKind {
    let has = |n: &str| chain.contains(&n);
    if has("uvm_component") {
        for (base, kind) in [
            ("uvm_test", UvmKind::Test),
            ("uvm_env", UvmKind::Env),
            ("uvm_agent", UvmKind::Agent),
            ("uvm_driver", UvmKind::Driver),
            ("uvm_monitor", UvmKind::Monitor),
            ("uvm_sequencer_base", UvmKind::Sequencer),
            ("uvm_scoreboard", UvmKind::Scoreboard),
            ("uvm_subscriber", UvmKind::Subscriber),
        ] {
            if has(base) {
                return kind;
            }
        }
        return UvmKind::Component;
    }
    if has("uvm_sequence_base") || has("uvm_sequence") {
        return UvmKind::Sequence;
    }
    if has("uvm_sequence_item") {
        return UvmKind::SequenceItem;
    }
    if has("uvm_object") {
        UvmKind::Object
    } else {
        UvmKind::None
    }
}

/// A method (function or task) of a class, with the facts rules read from its
/// body.
#[derive(Clone, Debug)]
pub struct Method {
    /// Declared name (`new` for the constructor).
    pub name: String,
    /// Access qualifier.
    pub visibility: Visibility,
    /// A task rather than a function.
    pub is_task: bool,
    /// Virtual, declared so or by overriding a virtual method.
    pub is_virtual: bool,
    /// `pure virtual`.
    pub is_pure: bool,
    /// Declared `static`.
    pub is_static: bool,
    /// Which standard UVM phase method this is, if any.
    pub phase: PhaseKind,
    /// Has the `virtual` keyword itself (`is_virtual` also covers overrides).
    pub declared_virtual: bool,
    /// Made by the compiler (randomize, pre_randomize and the like).
    pub is_builtin: bool,
    /// Declared by a prototype in the class; `loc` is the body outside it.
    pub is_extern: bool,
    /// Return type as written; `"void"` for tasks.
    pub return_type: String,
    /// True for tasks and `void` functions.
    pub returns_void: bool,
    /// Where the method is declared (the out-of-block body for an `extern` one).
    pub loc: Loc,
    /// Formal arguments, in order.
    pub arguments: Vec<Argument>,
    /// Declared locals only (not `this` or a function's return variable).
    pub locals: Vec<Local>,
    /// Calls in the body, in walk order (nested calls included).
    pub calls: Vec<Call>,
    /// Assignments in the body (`=`, `<=`, `+=` and the like), in walk order.
    pub assignments: Vec<Assignment>,
    /// Delays, event controls and waits in the body, in walk order.
    pub timing_controls: Vec<Loc>,
    /// `super.new(...)` is called somewhere in the body.
    pub calls_super_new: bool,
    /// Position of the `super.new(...);` statement among the top-level
    /// statements, and where it is.
    pub super_new: Option<(u32, Loc)>,
}

/// A class: one entry per definition. A specialization of a parameterized
/// class (`uvm_driver #(pkt)`) maps to the entry of its generic class.
#[derive(Clone, Debug)]
pub struct Class {
    /// This class's index in [`Design::classes`].
    pub id: ClassId,
    /// Class name, without parameters.
    pub name: String,
    /// Enclosing package or class; empty at `$unit`.
    pub scope: String,
    /// The base class as written after `extends` (generic name); empty when none.
    pub base_name: String,
    /// The base class, when it is in this model.
    pub base: Option<ClassId>,
    /// UVM role from the base chain.
    pub uvm: UvmKind,
    /// A `virtual class`.
    pub is_virtual: bool,
    /// A parameterized class. Members are as declared, with types read from one
    /// of its specializations, so parameter types appear resolved.
    pub is_parameterized: bool,
    /// From a library path (uvm-core and the like) rather than user code.
    pub is_library: bool,
    /// An `interface class`.
    pub is_interface: bool,
    /// Where the class name is declared.
    pub loc: Loc,
    /// Properties declared in this class (not inherited), in order.
    pub properties: Vec<Property>,
    /// Methods declared in this class (not inherited), in order, compiler-made
    /// ones included (see [`Method::is_builtin`]).
    pub methods: Vec<Method>,
    /// Named constraint blocks, in order.
    pub constraints: Vec<Constraint>,
    /// User macros used inside the class's text, in order.
    pub macros: Vec<MacroUse>,
    /// Typedefs declared in the class, in order.
    pub typedefs: Vec<Typedef>,
}

/// An owned snapshot of a loaded design: its files and classes. Nothing in it
/// borrows from the front end that built it, so it can be shared between
/// threads and kept after the front end is gone.
#[derive(Clone, Debug)]
pub struct Design {
    files: Vec<String>,
    classes: Vec<Class>,
    compile_errors: u32,
    timings: Timings,
    /// How many specializations of each class slang elaborates (by id);
    /// empty when the front end does not say.
    specializations: Vec<u32>,
}

/// Owns a C design and frees it on every exit path.
#[cfg(feature = "slang")]
struct Handle(*mut ffi::uvmcheck_design);

#[cfg(feature = "slang")]
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { ffi::uvmcheck_design_free(self.0) }
    }
}

#[cfg(feature = "slang")]
fn text(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

#[cfg(feature = "slang")]
fn loc(l: ffi::uvmcheck_loc) -> Loc {
    Loc {
        file: l.file,
        line: l.line,
        column: l.column,
        expansion: (l.exp_line != 0).then_some((l.exp_file, l.exp_line)),
    }
}

#[cfg(feature = "slang")]
fn id(v: u32) -> Option<ClassId> {
    (v != ffi::NONE).then_some(v)
}

#[cfg(feature = "slang")]
fn visibility_from_c(v: u8) -> Visibility {
    match v {
        1 => Visibility::Protected,
        2 => Visibility::Local,
        _ => Visibility::Public,
    }
}

#[cfg(feature = "slang")]
fn direction_from_c(v: u8) -> Direction {
    match v {
        1 => Direction::Out,
        2 => Direction::InOut,
        3 => Direction::Ref,
        _ => Direction::In,
    }
}

#[cfg(feature = "slang")]
fn shape_from_c(v: u8) -> TypeShape {
    match v {
        1 => TypeShape::DynamicArray,
        2 => TypeShape::Queue,
        3 => TypeShape::AssociativeArray,
        4 => TypeShape::FixedUnpackedArray,
        5 => TypeShape::UnpackedStruct,
        _ => TypeShape::Other,
    }
}

impl Design {
    /// Lifts a pure IEEE 1800 [`svlang::semantic::model::Design`] into [`Design`],
    /// classifying UVM component/object roles and standard UVM phase methods.
    pub fn from_sv(sv: svlang::semantic::model::Design) -> Design {
        let (files, sv_classes, compile_errors, timings, specializations) = sv.into_parts();
        let names: Vec<String> = sv_classes.iter().map(|c| c.name.clone()).collect();
        let classes = sv_classes
            .into_iter()
            .map(|c| {
                let uvm = if c.lineage.is_empty() {
                    UvmKind::None
                } else {
                    let chain: Vec<&str> = c.lineage.iter().map(|&id| names[id as usize].as_str()).collect();
                    classify_uvm(&chain)
                };
                let methods = c
                    .methods
                    .into_iter()
                    .map(|m| {
                        let phase = if m.is_builtin { PhaseKind::None } else { PhaseKind::from_method_name(&m.name) };
                        Method {
                            name: m.name,
                            visibility: m.visibility,
                            is_task: m.is_task,
                            is_virtual: m.is_virtual,
                            is_pure: m.is_pure,
                            is_static: m.is_static,
                            phase,
                            declared_virtual: m.declared_virtual,
                            is_builtin: m.is_builtin,
                            is_extern: m.is_extern,
                            return_type: m.return_type,
                            returns_void: m.returns_void,
                            loc: m.loc,
                            arguments: m.arguments,
                            locals: m.locals,
                            calls: m.calls,
                            assignments: m.assignments,
                            timing_controls: m.timing_controls,
                            calls_super_new: m.calls_super_new,
                            super_new: m.super_new,
                        }
                    })
                    .collect();
                Class {
                    id: c.id,
                    name: c.name,
                    scope: c.scope,
                    base_name: c.base_name,
                    base: c.base,
                    uvm,
                    is_virtual: c.is_virtual,
                    is_parameterized: c.is_parameterized,
                    is_library: c.is_library,
                    is_interface: c.is_interface,
                    loc: c.loc,
                    properties: c.properties,
                    methods,
                    constraints: c.constraints,
                    macros: c.macros,
                    typedefs: c.typedefs,
                }
            })
            .collect();
        Design { files, classes, compile_errors, timings, specializations }
    }

    /// A design built by another front end.
    pub fn from_parts(files: Vec<String>, classes: Vec<Class>, compile_errors: u32, timings: Timings) -> Design {
        Design { files, classes, compile_errors, timings, specializations: Vec::new() }
    }

    /// Sets [`Self::specializations`] for every class, by id.
    pub fn set_specializations(&mut self, counts: Vec<u32>) {
        self.specializations = counts;
    }

    /// How many times slang's AST walk visits the body of class `id`: once
    /// per specialization of a parameterized class (its default one and
    /// each distinct one the design names), once for any other class. 1
    /// when the front end does not count them.
    pub fn specializations(&self, id: ClassId) -> u32 {
        self.specializations.get(id as usize).copied().unwrap_or(1)
    }

    /// Every class's [`Self::specializations`], by id (empty when unknown).
    pub fn specialization_counts(&self) -> &[u32] {
        &self.specializations
    }

    /// Loads a design through slang (libuvmcheck_model) from compile options as given to `uvmcheck`, e.g.
    /// `["+incdir+src", "src/uvm_pkg.sv"]` or `["-f", "files.f"]`.
    #[cfg(feature = "slang")]
    pub fn load<I, S>(args: I) -> Result<Design, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let abi = unsafe { ffi::uvmcheck_model_abi_version() };
        if abi != ffi::ABI_VERSION {
            return Err(Error(format!("libuvmcheck_model ABI {abi}, expected {}", ffi::ABI_VERSION)));
        }
        let args: Vec<CString> = args
            .into_iter()
            .map(|a| CString::new(a.as_ref()).map_err(|_| Error("argument contains a NUL byte".into())))
            .collect::<Result<_, _>>()?;
        let argv: Vec<*const c_char> = args.iter().map(|a| a.as_ptr()).collect();
        let handle = Handle(unsafe { ffi::uvmcheck_design_load(argv.len() as i32, argv.as_ptr()) });
        let d = handle.0;
        let err = unsafe { ffi::uvmcheck_design_error(d) };
        if !err.is_null() {
            return Err(Error(text(err)));
        }

        let start = Instant::now();
        let files = (0..unsafe { ffi::uvmcheck_file_count(d) })
            .map(|i| text(unsafe { ffi::uvmcheck_file_name(d, i) }))
            .collect();
        let count = unsafe { ffi::uvmcheck_class_count(d) };
        let mut classes = Vec::with_capacity(count as usize);
        for cid in 0..count {
            let mut c = std::mem::MaybeUninit::<ffi::uvmcheck_class>::uninit();
            if unsafe { ffi::uvmcheck_get_class(d, cid, c.as_mut_ptr()) } == 0 {
                return Err(Error(format!("class {cid} missing")));
            }
            let c = unsafe { c.assume_init() };
            let mut properties = Vec::with_capacity(c.property_count as usize);
            for i in 0..c.property_count {
                let mut p = std::mem::MaybeUninit::<ffi::uvmcheck_property>::uninit();
                if unsafe { ffi::uvmcheck_get_property(d, cid, i, p.as_mut_ptr()) } == 0 {
                    return Err(Error(format!("property {i} of class {cid} missing")));
                }
                let p = unsafe { p.assume_init() };
                properties.push(Property {
                    name: text(p.name),
                    ty: text(p.type_),
                    type_class: id(p.type_class),
                    visibility: visibility_from_c(p.visibility),
                    is_static: p.is_static != 0,
                    is_const: p.is_const != 0,
                    is_rand: p.is_rand != 0,
                    is_virtual_interface: p.is_virtual_interface != 0,
                    loc: loc(p.loc),
                });
            }
            let mut methods = Vec::with_capacity(c.method_count as usize);
            for i in 0..c.method_count {
                let mut m = std::mem::MaybeUninit::<ffi::uvmcheck_method>::uninit();
                if unsafe { ffi::uvmcheck_get_method(d, cid, i, m.as_mut_ptr()) } == 0 {
                    return Err(Error(format!("method {i} of class {cid} missing")));
                }
                let m = unsafe { m.assume_init() };
                let mut arguments = Vec::with_capacity(m.argument_count as usize);
                for a in 0..m.argument_count {
                    let mut x = std::mem::MaybeUninit::<ffi::uvmcheck_argument>::uninit();
                    if unsafe { ffi::uvmcheck_get_argument(d, cid, i, a, x.as_mut_ptr()) } == 0 {
                        return Err(Error(format!("argument {a} of method {i} of class {cid} missing")));
                    }
                    let x = unsafe { x.assume_init() };
                    arguments.push(Argument {
                        name: text(x.name),
                        ty: text(x.type_),
                        canonical_type: text(x.canonical_type),
                        type_class: id(x.type_class),
                        direction: direction_from_c(x.direction),
                        shape: shape_from_c(x.shape),
                        loc: loc(x.loc),
                    });
                }
                let mut locals = Vec::with_capacity(m.local_count as usize);
                for l in 0..m.local_count {
                    let mut x = std::mem::MaybeUninit::<ffi::uvmcheck_local>::uninit();
                    if unsafe { ffi::uvmcheck_get_local(d, cid, i, l, x.as_mut_ptr()) } == 0 {
                        return Err(Error(format!("local {l} of method {i} of class {cid} missing")));
                    }
                    let x = unsafe { x.assume_init() };
                    locals.push(Local {
                        name: text(x.name),
                        ty: text(x.type_),
                        is_virtual_interface: x.is_virtual_interface != 0,
                        loc: loc(x.loc),
                    });
                }
                let mut calls = Vec::with_capacity(m.call_count as usize);
                for k in 0..m.call_count {
                    let mut x = std::mem::MaybeUninit::<ffi::uvmcheck_call>::uninit();
                    if unsafe { ffi::uvmcheck_get_call(d, cid, i, k, x.as_mut_ptr()) } == 0 {
                        return Err(Error(format!("call {k} of method {i} of class {cid} missing")));
                    }
                    let x = unsafe { x.assume_init() };
                    calls.push(Call {
                        name: text(x.name),
                        is_system: x.is_system != 0,
                        callee_scope: text(x.callee_scope),
                        callee_declared_virtual: x.callee_declared_virtual != 0,
                        callee_builtin: x.callee_builtin != 0,
                        first_arg_is_null: x.first_arg_is_null != 0,
                        first_arg_loc: loc(x.first_arg_loc),
                        receiver: text(x.receiver),
                        via_super: x.via_super != 0,
                        loc: loc(x.loc),
                    });
                }
                let mut assignments = Vec::with_capacity(m.assignment_count as usize);
                for k in 0..m.assignment_count {
                    let mut x = std::mem::MaybeUninit::<ffi::uvmcheck_assignment>::uninit();
                    if unsafe { ffi::uvmcheck_get_assignment(d, cid, i, k, x.as_mut_ptr()) } == 0 {
                        return Err(Error(format!("assignment {k} of method {i} of class {cid} missing")));
                    }
                    let x = unsafe { x.assume_init() };
                    assignments.push(Assignment {
                        lhs: text(x.lhs),
                        rhs: text(x.rhs),
                        is_non_blocking: x.is_non_blocking != 0,
                        root_kind: match x.root_kind {
                            1 => RootKind::Argument,
                            2 => RootKind::Local,
                            3 => RootKind::Property,
                            _ => RootKind::Other,
                        },
                        root_name: text(x.root_name),
                        loc: loc(x.loc),
                    });
                }
                let mut timing_controls = Vec::with_capacity(m.timing_count as usize);
                for k in 0..m.timing_count {
                    let mut x = ffi::uvmcheck_loc::default();
                    if unsafe { ffi::uvmcheck_get_timing(d, cid, i, k, &mut x) } == 0 {
                        return Err(Error(format!("timing control {k} of method {i} of class {cid} missing")));
                    }
                    timing_controls.push(loc(x));
                }
                methods.push(Method {
                    name: text(m.name),
                    visibility: visibility_from_c(m.visibility),
                    is_task: m.is_task != 0,
                    is_virtual: m.is_virtual != 0,
                    is_pure: m.is_pure != 0,
                    is_static: m.is_static != 0,
                    phase: match m.phase {
                        1 => PhaseKind::Function,
                        2 => PhaseKind::Task,
                        _ => PhaseKind::None,
                    },
                    declared_virtual: m.declared_virtual != 0,
                    is_builtin: m.is_builtin != 0,
                    is_extern: m.is_extern != 0,
                    return_type: text(m.return_type),
                    returns_void: m.returns_void != 0,
                    loc: loc(m.loc),
                    arguments,
                    locals,
                    calls,
                    assignments,
                    timing_controls,
                    calls_super_new: m.calls_super_new != 0,
                    super_new: (m.super_new_index >= 0).then(|| (m.super_new_index as u32, loc(m.super_new_loc))),
                });
            }
            let mut constraints = Vec::with_capacity(c.constraint_count as usize);
            for i in 0..c.constraint_count {
                let mut x = std::mem::MaybeUninit::<ffi::uvmcheck_constraint>::uninit();
                if unsafe { ffi::uvmcheck_get_constraint(d, cid, i, x.as_mut_ptr()) } == 0 {
                    return Err(Error(format!("constraint {i} of class {cid} missing")));
                }
                let x = unsafe { x.assume_init() };
                constraints.push(Constraint { name: text(x.name), loc: loc(x.loc) });
            }
            let mut macros = Vec::with_capacity(c.macro_count as usize);
            for i in 0..c.macro_count {
                let mut x = std::mem::MaybeUninit::<ffi::uvmcheck_macro>::uninit();
                if unsafe { ffi::uvmcheck_get_macro(d, cid, i, x.as_mut_ptr()) } == 0 {
                    return Err(Error(format!("macro use {i} of class {cid} missing")));
                }
                let x = unsafe { x.assume_init() };
                macros.push(MacroUse { name: text(x.name), loc: loc(x.loc) });
            }
            let mut typedefs = Vec::with_capacity(c.typedef_count as usize);
            for i in 0..c.typedef_count {
                let mut x = std::mem::MaybeUninit::<ffi::uvmcheck_typedef>::uninit();
                if unsafe { ffi::uvmcheck_get_typedef(d, cid, i, x.as_mut_ptr()) } == 0 {
                    return Err(Error(format!("typedef {i} of class {cid} missing")));
                }
                let x = unsafe { x.assume_init() };
                typedefs.push(Typedef { name: text(x.name), loc: loc(x.loc) });
            }
            classes.push(Class {
                id: cid,
                name: text(c.name),
                scope: text(c.scope),
                base_name: text(c.base_name),
                base: id(c.base),
                uvm: UvmKind::from_c(c.uvm),
                is_virtual: c.is_virtual != 0,
                is_parameterized: c.is_parameterized != 0,
                is_library: c.is_library != 0,
                is_interface: c.is_interface != 0,
                loc: loc(c.loc),
                properties,
                methods,
                constraints,
                macros,
                typedefs,
            });
        }
        let copy_ms = start.elapsed().as_secs_f64() * 1e3;
        let timings = unsafe {
            Timings {
                parse_ms: ffi::uvmcheck_design_time_ms(d, 0),
                elaborate_ms: ffi::uvmcheck_design_time_ms(d, 1),
                model_ms: ffi::uvmcheck_design_time_ms(d, 2),
                copy_ms,
            }
        };
        let compile_errors = unsafe { ffi::uvmcheck_design_compile_errors(d) };
        Ok(Design { files, classes, compile_errors, timings, specializations: Vec::new() })
    }

    /// Every class, indexed by [`ClassId`].
    pub fn classes(&self) -> &[Class] {
        &self.classes
    }

    /// The class with id `id`. Panics when the id is out of range.
    pub fn class(&self, id: ClassId) -> &Class {
        &self.classes[id as usize]
    }

    /// The first class named `name` (any scope).
    pub fn find_class(&self, name: &str) -> Option<&Class> {
        self.classes.iter().find(|c| c.name == name)
    }

    /// File names, indexed by [`Loc::file`]; entry 0 is the empty "no file".
    pub fn files(&self) -> &[String] {
        &self.files
    }

    /// The file name of `loc`, or `""` when it has none.
    pub fn file_name(&self, loc: Loc) -> &str {
        self.files.get(loc.file as usize).map(String::as_str).unwrap_or("")
    }

    /// Errors the front end reported while compiling (parse, preprocessor and,
    /// through slang, semantic errors). The model is still built when this is not 0.
    pub fn compile_errors(&self) -> u32 {
        self.compile_errors
    }

    /// What each stage of the load cost.
    pub fn timings(&self) -> Timings {
        self.timings
    }

    /// The class's base chain, nearest first, ending at the first base that is
    /// not in the model. Stops on a cycle.
    pub fn ancestors(&self, id: ClassId) -> impl Iterator<Item = &Class> {
        let mut next = self.class(id).base;
        let mut left = self.classes.len();
        std::iter::from_fn(move || {
            let cur = next.filter(|_| left > 0)?;
            left -= 1;
            let c = self.class(cur);
            next = c.base;
            Some(c)
        })
    }

    /// True when the class is `name` or derives from it.
    pub fn derives_from(&self, id: ClassId, name: &str) -> bool {
        let c = self.class(id);
        c.name == name || c.base_name == name || self.ancestors(id).any(|a| a.name == name || a.base_name == name)
    }

    /// Component-typed properties of a component: its likely children.
    pub fn child_components<'a>(&'a self, c: &'a Class) -> impl Iterator<Item = (&'a Property, &'a Class)> + 'a {
        c.properties.iter().filter_map(move |p| {
            let t = self.class(p.type_class?);
            t.uvm.is_component().then_some((p, t))
        })
    }
}

/// What [`Design::dump`] prints beyond names, flags and locations.
#[derive(Clone, Copy, Debug, Default)]
pub struct DumpOptions {
    /// Type strings (property, argument, local and return types).
    pub types: bool,
    /// Calls, assignments, timing controls and `super.new` of each method.
    pub bodies: bool,
}

impl Design {
    /// A canonical text form, for comparing front ends: classes sorted by
    /// scope, name and location, class references by name.
    pub fn dump(&self, opts: DumpOptions) -> String {
        use std::fmt::Write;
        let qual = |id: Option<ClassId>| match id {
            Some(i) => {
                let c = self.class(i);
                if c.scope.is_empty() {
                    c.name.clone()
                } else {
                    format!("{}::{}", c.scope, c.name)
                }
            }
            None => "-".into(),
        };
        let at = |l: Loc| format!("@{}:{}:{}", self.file_name(l), l.line, l.column);
        let vis = |v: Visibility| match v {
            Visibility::Public => "public",
            Visibility::Protected => "protected",
            Visibility::Local => "local",
        };
        let flag = |b: bool, s: &'static str| if b { s } else { "" };
        let mut order: Vec<&Class> = self.classes.iter().collect();
        order.sort_by(|a, b| {
            (&a.scope, &a.name, self.file_name(a.loc), a.loc.line, a.loc.column).cmp(&(
                &b.scope,
                &b.name,
                self.file_name(b.loc),
                b.loc.line,
                b.loc.column,
            ))
        });
        let mut s = String::new();
        for c in order {
            let _ = writeln!(
                s,
                "class {} {} base={} base_id={} uvm={} {} {} {}{}",
                qual(Some(c.id)),
                at(c.loc),
                c.base_name,
                qual(c.base),
                c.uvm.as_str(),
                flag(c.is_virtual, "virtual"),
                flag(c.is_parameterized, "param"),
                flag(c.is_library, "library"),
                flag(c.is_interface, " interface"),
            );
            for m in &c.macros {
                let _ = writeln!(s, "  macro {} {}", m.name, at(m.loc));
            }
            for t in &c.typedefs {
                let _ = writeln!(s, "  typedef {} {}", t.name, at(t.loc));
            }
            for p in &c.properties {
                let _ = write!(
                    s,
                    "  prop {} {} {} {} {} {} class={} {}",
                    p.name,
                    vis(p.visibility),
                    flag(p.is_static, "static"),
                    flag(p.is_const, "const"),
                    flag(p.is_rand, "rand"),
                    flag(p.is_virtual_interface, "vif"),
                    qual(p.type_class),
                    at(p.loc)
                );
                if opts.types {
                    let _ = write!(s, " type={}", p.ty);
                }
                s.push('\n');
            }
            for m in &c.methods {
                let _ = write!(
                    s,
                    "  method {} {} {} {} {} {} {:?} {} {} {} {} {}",
                    m.name,
                    vis(m.visibility),
                    flag(m.is_task, "task"),
                    flag(m.is_virtual, "virtual"),
                    flag(m.is_pure, "pure"),
                    flag(m.is_static, "static"),
                    m.phase,
                    flag(m.declared_virtual, "declared_virtual"),
                    flag(m.is_builtin, "builtin"),
                    flag(m.is_extern, "extern"),
                    flag(m.returns_void, "void"),
                    at(m.loc)
                );
                if opts.types {
                    let _ = write!(s, " returns={}", m.return_type);
                }
                s.push('\n');
                for a in &m.arguments {
                    let _ = write!(
                        s,
                        "    arg {} {:?} {:?} class={} {}",
                        a.name,
                        a.direction,
                        a.shape,
                        qual(a.type_class),
                        at(a.loc)
                    );
                    if opts.types {
                        let _ = write!(s, " type={} canonical={}", a.ty, a.canonical_type);
                    }
                    s.push('\n');
                }
                for l in &m.locals {
                    let _ = write!(s, "    local {} {} {}", l.name, flag(l.is_virtual_interface, "vif"), at(l.loc));
                    if opts.types {
                        let _ = write!(s, " type={}", l.ty);
                    }
                    s.push('\n');
                }
                if opts.bodies {
                    for k in &m.calls {
                        let _ = writeln!(
                            s,
                            "    call {} {} scope={} {} {} {} receiver={} {} {}",
                            k.name,
                            flag(k.is_system, "system"),
                            k.callee_scope,
                            flag(k.callee_declared_virtual, "callee_virtual"),
                            flag(k.callee_builtin, "callee_builtin"),
                            flag(k.first_arg_is_null, "null_arg"),
                            k.receiver.replace('\n', "\\n"),
                            flag(k.via_super, "super"),
                            at(k.loc)
                        );
                        if k.first_arg_loc.line != 0 {
                            let n = s.len();
                            s.truncate(n - 1);
                            let _ = writeln!(s, " arg0{}", at(k.first_arg_loc));
                        }
                    }
                    for a in &m.assignments {
                        let _ = writeln!(
                            s,
                            "    assign {} = {} {} {:?} {} {}",
                            a.lhs.replace('\n', "\\n"),
                            a.rhs.replace('\n', "\\n"),
                            flag(a.is_non_blocking, "nonblocking"),
                            a.root_kind,
                            a.root_name,
                            at(a.loc)
                        );
                    }
                    for t in &m.timing_controls {
                        let _ = writeln!(s, "    timing {}", at(*t));
                    }
                    if let Some((i, l)) = m.super_new {
                        let _ = writeln!(s, "    super_new {} {}", i, at(l));
                    } else if m.calls_super_new {
                        s.push_str("    super_new nested\n");
                    }
                }
            }
            for k in &c.constraints {
                let _ = writeln!(s, "  constraint {} {}", k.name, at(k.loc));
            }
        }
        s
    }
}
