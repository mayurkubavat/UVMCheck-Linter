//! Pure IEEE 1800-2023 class and design model built from syntax trees.
//!
//! Every type in this module is owned and independent of the syntax trees that
//! produced it, so callers can share a [`Design`] across threads or layer
//! methodology-specific models on top of it.

use std::fmt;

/// Index of a class in [`Design::classes`]; also how classes refer to each
/// other ([`Class::base`], [`Class::lineage`], [`Property::type_class`]).
pub type ClassId = u32;

/// Access qualifier of a class member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    /// No qualifier.
    Public,
    /// `protected`.
    Protected,
    /// `local`.
    Local,
}

/// Direction of a subroutine argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// `input` (also the default when none is written).
    In,
    /// `output`.
    Out,
    /// `inout`.
    InOut,
    /// `ref` (and `const ref`).
    Ref,
}

/// Aggregate shape of a SystemVerilog data type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeShape {
    /// Anything not listed below.
    Other,
    /// `T x[]`.
    DynamicArray,
    /// `T x[$]`.
    Queue,
    /// `T x[K]` with a key type or `*`.
    AssociativeArray,
    /// `T x[N]` or `T x[A:B]`.
    FixedUnpackedArray,
    /// An unpacked `struct` (not a packed one).
    UnpackedStruct,
}

/// A source position. Lines and columns start at 1; a default `Loc` (file 0,
/// line 0) means "unknown".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Loc {
    /// Index into [`Design::files`]; 0 is "no file".
    pub file: u32,
    /// 1-based line.
    pub line: u32,
    /// 1-based byte column.
    pub column: u32,
    /// For code from a macro: the file and line of the outermost expansion
    /// (with column 0).
    pub expansion: Option<(u32, u32)>,
}

/// A class property (data member).
#[derive(Clone, Debug)]
pub struct Property {
    /// Declared name.
    pub name: String,
    /// Type string with parameter types resolved.
    pub ty: String,
    /// The property's type, when it is a class in this model.
    pub type_class: Option<ClassId>,
    /// Access qualifier.
    pub visibility: Visibility,
    /// Declared `static`.
    pub is_static: bool,
    /// Declared `const`.
    pub is_const: bool,
    /// Declared `rand` or `randc`.
    pub is_rand: bool,
    /// A virtual interface, or an array of them.
    pub is_virtual_interface: bool,
    /// Where the name is declared.
    pub loc: Loc,
}

/// A method (function or task) of a class, with extracted body facts.
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
    /// Has the `virtual` keyword itself (`is_virtual` also covers overrides).
    pub declared_virtual: bool,
    /// Built-in IEEE 1800 class method (`randomize`, `pre_randomize`, etc.).
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

/// What the left side of an assignment is rooted in: `x`, `x.f`, `x[i]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootKind {
    /// Anything else (a call result, a concatenation, a hierarchical name).
    Other,
    /// An argument of the method.
    Argument,
    /// A local variable of the method.
    Local,
    /// A property of the class.
    Property,
}

/// An assignment in a method body, in walk order.
#[derive(Clone, Debug)]
pub struct Assignment {
    /// Left side as written.
    pub lhs: String,
    /// Right side as written.
    pub rhs: String,
    /// `<=` rather than `=` or an operator assignment.
    pub is_non_blocking: bool,
    /// What the left side is rooted in.
    pub root_kind: RootKind,
    /// The argument, local or property assigned into.
    pub root_name: String,
    /// Where the assignment is.
    pub loc: Loc,
}

/// A call made in a method body, in the order the body is walked; calls nested
/// in arguments and in nested blocks are included.
#[derive(Clone, Debug)]
pub struct Call {
    /// As written: `"randomize"`, `"$urandom"`, `"get"`.
    pub name: String,
    /// A system task or function, or a built-in method such as `randomize()`.
    pub is_system: bool,
    /// Class (generic name) or package that declares the callee; empty if none.
    pub callee_scope: String,
    /// The callee has the `virtual` keyword itself.
    pub callee_declared_virtual: bool,
    /// The callee is built in by IEEE 1800 (`randomize` and the like).
    pub callee_builtin: bool,
    /// The first argument is the `null` literal.
    pub first_arg_is_null: bool,
    /// Where the first argument starts (line 0 when there is none).
    pub first_arg_loc: Loc,
    /// The object called on, as written (`"m_env.ap"`); empty if none.
    pub receiver: String,
    /// Written as `super.name(...)`.
    pub via_super: bool,
    /// Where the call is.
    pub loc: Loc,
}

/// A formal argument of a method.
#[derive(Clone, Debug)]
pub struct Argument {
    /// Declared name.
    pub name: String,
    /// The type as declared.
    pub ty: String,
    /// The type with typedefs resolved.
    pub canonical_type: String,
    /// The argument's type, when it is a class in this model.
    pub type_class: Option<ClassId>,
    /// Argument direction.
    pub direction: Direction,
    /// Aggregate shape of the type.
    pub shape: TypeShape,
    /// Where the argument is declared.
    pub loc: Loc,
}

/// A variable declared directly in a method's body.
#[derive(Clone, Debug)]
pub struct Local {
    /// Declared name.
    pub name: String,
    /// Resolved type string.
    pub ty: String,
    /// A virtual interface, or an array of them.
    pub is_virtual_interface: bool,
    /// Where the variable is declared.
    pub loc: Loc,
}

/// A named constraint block of a class.
#[derive(Clone, Debug)]
pub struct Constraint {
    /// Constraint block name.
    pub name: String,
    /// Where the block is declared.
    pub loc: Loc,
}

/// A use of a user macro written inside the class's text, by name without
/// the backtick.
#[derive(Clone, Debug)]
pub struct MacroUse {
    /// Macro name without the backtick.
    pub name: String,
    /// Where the macro is used.
    pub loc: Loc,
}

/// A typedef declared in the class.
#[derive(Clone, Debug)]
pub struct Typedef {
    /// Typedef name.
    pub name: String,
    /// Where the typedef is declared.
    pub loc: Loc,
}

/// A SystemVerilog class: one entry per definition. A specialization of a
/// parameterized class maps to the entry of its generic class.
#[derive(Clone, Debug)]
pub struct Class {
    /// This class's index in [`Design::classes`].
    pub id: ClassId,
    /// Class name, without parameters.
    pub name: String,
    /// Enclosing package, module or class; empty at `$unit`.
    pub scope: String,
    /// The base class as written after `extends` (generic name); empty when none.
    pub base_name: String,
    /// The base class, when it is in this model.
    pub base: Option<ClassId>,
    /// Resolved inheritance chain starting with `self.id` (propagating type
    /// parameter bindings along `extends`), or empty for an unspecialized
    /// parameterized class definition.
    pub lineage: Vec<ClassId>,
    /// A `virtual class`.
    pub is_virtual: bool,
    /// A parameterized class. Members are as declared, with types read from one
    /// of its specializations, so parameter types appear resolved.
    pub is_parameterized: bool,
    /// From a library path rather than user code.
    pub is_library: bool,
    /// An `interface class`.
    pub is_interface: bool,
    /// Where the class name is declared.
    pub loc: Loc,
    /// Properties declared in this class (not inherited), in order.
    pub properties: Vec<Property>,
    /// Methods declared in this class (not inherited), in order, built-in ones
    /// included (see [`Method::is_builtin`]).
    pub methods: Vec<Method>,
    /// Named constraint blocks, in order.
    pub constraints: Vec<Constraint>,
    /// User macros used inside the class's text, in order.
    pub macros: Vec<MacroUse>,
    /// Typedefs declared in the class, in order.
    pub typedefs: Vec<Typedef>,
}

/// Wall-clock cost of a load, by stage, in milliseconds.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timings {
    /// Preprocessing and parsing.
    pub parse_ms: f64,
    /// Elaboration (0 when building straight from syntax trees).
    pub elaborate_ms: f64,
    /// Building the semantic model.
    pub model_ms: f64,
    /// Copying across an external boundary (0 for native builds).
    pub copy_ms: f64,
}

/// A design that could not be loaded (bad options or unreadable sources).
/// Compile errors in SystemVerilog code are reported in [`Design::compile_errors`],
/// not as an `Error`.
#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Owned SystemVerilog class and file model of a compilation.
#[derive(Clone, Debug)]
pub struct Design {
    files: Vec<String>,
    classes: Vec<Class>,
    compile_errors: u32,
    timings: Timings,
    specializations: Vec<u32>,
}

impl Design {
    /// Constructs a [`Design`] from its owned parts.
    pub fn from_parts(files: Vec<String>, classes: Vec<Class>, compile_errors: u32, timings: Timings) -> Design {
        Design { files, classes, compile_errors, timings, specializations: Vec::new() }
    }

    /// Sets [`Self::specializations`] for every class, by id.
    pub fn set_specializations(&mut self, counts: Vec<u32>) {
        self.specializations = counts;
    }

    /// Distinct specializations elaborated for class `id` (1 when non-generic
    /// or not tracked).
    pub fn specializations(&self, id: ClassId) -> u32 {
        self.specializations.get(id as usize).copied().unwrap_or(1)
    }

    /// Every class's [`Self::specializations`], by id (empty when unknown).
    pub fn specialization_counts(&self) -> &[u32] {
        &self.specializations
    }

    /// Consumes the design and returns `(files, classes, compile_errors, timings, specializations)`.
    pub fn into_parts(self) -> (Vec<String>, Vec<Class>, u32, Timings, Vec<u32>) {
        (self.files, self.classes, self.compile_errors, self.timings, self.specializations)
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

    /// File names, indexed by [`Loc::file`]; entry 0 is the empty `"no file"`.
    pub fn files(&self) -> &[String] {
        &self.files
    }

    /// The file name of `loc`, or `""` when it has none.
    pub fn file_name(&self, loc: Loc) -> &str {
        self.files.get(loc.file as usize).map(String::as_str).unwrap_or("")
    }

    /// Parse and preprocessor error count encountered while loading.
    pub fn compile_errors(&self) -> u32 {
        self.compile_errors
    }

    /// Wall-clock cost per stage.
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
}
