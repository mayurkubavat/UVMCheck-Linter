//! The design model every DV tool reads: classes, their UVM role, properties
//! and methods, with source locations.
//!
//! [`Design::load`] takes the same compile options as `uvmcheck` and returns
//! an owned snapshot; nothing borrows from the front end afterwards. Today the
//! front end is slang behind `sv-slang`; tools do not see that and will not
//! notice when layers move to native Rust.

use std::fmt;
#[cfg(feature = "slang")]
use std::ffi::{CStr, CString};
#[cfg(feature = "slang")]
use std::os::raw::c_char;
#[cfg(feature = "slang")]
use std::time::Instant;

#[cfg(feature = "slang")]
use sv_slang as ffi;

pub type ClassId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UvmKind {
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
}

impl UvmKind {
    #[cfg(feature = "slang")]
    fn from_c(v: u8) -> Self {
        use UvmKind::*;
        [None, Object, SequenceItem, Sequence, Component, Test, Env, Agent, Driver, Monitor, Sequencer, Scoreboard, Subscriber]
            .get(v as usize)
            .copied()
            .unwrap_or(None)
    }

    /// Any `uvm_component`, whatever its role.
    pub fn is_component(self) -> bool {
        self as u8 >= UvmKind::Component as u8
    }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Protected,
    Local,
}

impl Visibility {
    #[cfg(feature = "slang")]
    fn from_c(v: u8) -> Self {
        match v {
            1 => Visibility::Protected,
            2 => Visibility::Local,
            _ => Visibility::Public,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaseKind {
    None,
    Function,
    Task,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
    InOut,
    Ref,
}

impl Direction {
    #[cfg(feature = "slang")]
    fn from_c(v: u8) -> Self {
        match v {
            1 => Direction::Out,
            2 => Direction::InOut,
            3 => Direction::Ref,
            _ => Direction::In,
        }
    }
}

/// The shape of a type, as far as rules care: aggregates that are costly to
/// copy, and the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeShape {
    Other,
    DynamicArray,
    Queue,
    AssociativeArray,
    FixedUnpackedArray,
    UnpackedStruct,
}

impl TypeShape {
    #[cfg(feature = "slang")]
    fn from_c(v: u8) -> Self {
        match v {
            1 => TypeShape::DynamicArray,
            2 => TypeShape::Queue,
            3 => TypeShape::AssociativeArray,
            4 => TypeShape::FixedUnpackedArray,
            5 => TypeShape::UnpackedStruct,
            _ => TypeShape::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Loc {
    /// Index into [`Design::files`]; 0 is "no file".
    pub file: u32,
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Debug)]
pub struct Property {
    pub name: String,
    pub ty: String,
    pub type_class: Option<ClassId>,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_const: bool,
    pub is_rand: bool,
    pub is_virtual_interface: bool,
    pub loc: Loc,
}

#[derive(Clone, Debug)]
pub struct Method {
    pub name: String,
    pub visibility: Visibility,
    pub is_task: bool,
    pub is_virtual: bool,
    pub is_pure: bool,
    pub is_static: bool,
    pub phase: PhaseKind,
    /// Has the `virtual` keyword itself (`is_virtual` also covers overrides).
    pub declared_virtual: bool,
    /// Made by the compiler (randomize, pre_randomize and the like).
    pub is_builtin: bool,
    /// Declared by a prototype in the class; `loc` is the body outside it.
    pub is_extern: bool,
    pub return_type: String,
    pub returns_void: bool,
    pub loc: Loc,
    pub arguments: Vec<Argument>,
    /// Declared locals only (not `this` or a function's return variable).
    pub locals: Vec<Local>,
    /// Calls in the body, in walk order (nested calls included).
    pub calls: Vec<Call>,
    pub assignments: Vec<Assignment>,
    /// Delays, event controls and waits in the body, in walk order.
    pub timing_controls: Vec<Loc>,
    /// `super.new(...)` is called somewhere in the body.
    pub calls_super_new: bool,
    /// Position of the `super.new(...);` statement among the top-level
    /// statements, and where it is.
    pub super_new: Option<(u32, Loc)>,
}

/// What the left side of an assignment is rooted in: `x`, `x.f`, `x[i]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootKind {
    Other,
    Argument,
    Local,
    Property,
}

#[derive(Clone, Debug)]
pub struct Assignment {
    /// Left side as written.
    pub lhs: String,
    /// Right side as written.
    pub rhs: String,
    pub is_non_blocking: bool,
    pub root_kind: RootKind,
    /// The argument, local or property assigned into.
    pub root_name: String,
    pub loc: Loc,
}

#[derive(Clone, Debug)]
pub struct Call {
    /// As written: "randomize", "$urandom", "get".
    pub name: String,
    /// A system task or function, or a built-in method such as randomize().
    pub is_system: bool,
    /// Class (generic name) or package that declares the callee; empty if none.
    pub callee_scope: String,
    pub callee_declared_virtual: bool,
    pub callee_builtin: bool,
    pub first_arg_is_null: bool,
    pub first_arg_loc: Loc,
    /// The object called on, as written ("m_env.ap"); empty if none.
    pub receiver: String,
    /// Written as `super.name(...)`.
    pub via_super: bool,
    pub loc: Loc,
}

#[derive(Clone, Debug)]
pub struct Argument {
    pub name: String,
    pub ty: String,
    pub canonical_type: String,
    pub type_class: Option<ClassId>,
    pub direction: Direction,
    pub shape: TypeShape,
    pub loc: Loc,
}

#[derive(Clone, Debug)]
pub struct Local {
    pub name: String,
    pub ty: String,
    pub is_virtual_interface: bool,
    pub loc: Loc,
}

#[derive(Clone, Debug)]
pub struct Constraint {
    pub name: String,
    pub loc: Loc,
}

#[derive(Clone, Debug)]
pub struct Class {
    pub id: ClassId,
    pub name: String,
    pub scope: String,
    pub base_name: String,
    pub base: Option<ClassId>,
    pub uvm: UvmKind,
    pub is_virtual: bool,
    pub is_parameterized: bool,
    pub is_library: bool,
    pub loc: Loc,
    pub properties: Vec<Property>,
    pub methods: Vec<Method>,
    pub constraints: Vec<Constraint>,
}

/// Wall-clock cost of a load, by stage, in milliseconds.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timings {
    pub parse_ms: f64,
    pub elaborate_ms: f64,
    /// Building the C++ model from the elaborated AST.
    pub model_ms: f64,
    /// Copying the model across the C ABI into Rust.
    pub copy_ms: f64,
}

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug)]
pub struct Design {
    files: Vec<String>,
    classes: Vec<Class>,
    compile_errors: u32,
    timings: Timings,
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
    Loc { file: l.file, line: l.line, column: l.column }
}

#[cfg(feature = "slang")]
fn id(v: u32) -> Option<ClassId> {
    (v != ffi::NONE).then_some(v)
}

impl Design {
    /// A design built by another front end (the native one in sv-sema).
    pub fn from_parts(files: Vec<String>, classes: Vec<Class>, compile_errors: u32, timings: Timings) -> Design {
        Design { files, classes, compile_errors, timings }
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
                    visibility: Visibility::from_c(p.visibility),
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
                        direction: Direction::from_c(x.direction),
                        shape: TypeShape::from_c(x.shape),
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
                    visibility: Visibility::from_c(m.visibility),
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
                loc: loc(c.loc),
                properties,
                methods,
                constraints,
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
        Ok(Design { files, classes, compile_errors, timings })
    }

    pub fn classes(&self) -> &[Class] {
        &self.classes
    }

    pub fn class(&self, id: ClassId) -> &Class {
        &self.classes[id as usize]
    }

    pub fn find_class(&self, name: &str) -> Option<&Class> {
        self.classes.iter().find(|c| c.name == name)
    }

    pub fn files(&self) -> &[String] {
        &self.files
    }

    pub fn file_name(&self, loc: Loc) -> &str {
        self.files.get(loc.file as usize).map(String::as_str).unwrap_or("")
    }

    pub fn compile_errors(&self) -> u32 {
        self.compile_errors
    }

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
                if c.scope.is_empty() { c.name.clone() } else { format!("{}::{}", c.scope, c.name) }
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
                "class {} {} base={} base_id={} uvm={} {} {} {}",
                qual(Some(c.id)),
                at(c.loc),
                c.base_name,
                qual(c.base),
                c.uvm.as_str(),
                flag(c.is_virtual, "virtual"),
                flag(c.is_parameterized, "param"),
                flag(c.is_library, "library"),
            );
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
                    let _ = write!(s, "    arg {} {:?} {:?} class={} {}", a.name, a.direction, a.shape, qual(a.type_class), at(a.loc));
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
