//! Raw bindings to `include/uvmcheck/model/CApi.h`. Keep this file in step
//! with that header; `dv-model` is the safe API tools should use.
#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_int};

/// The ABI version this binding was written for; `dv-model` refuses a library
/// that reports another one (`uvmcheck_model_abi_version`).
pub const ABI_VERSION: u32 = 7;
/// `UVMCHECK_NONE`: a class id meaning "no class".
pub const NONE: u32 = u32::MAX;

/// Opaque handle to a loaded design; free it with [`uvmcheck_design_free`].
#[repr(C)]
pub struct uvmcheck_design {
    _private: [u8; 0],
}

/// A source position (`uvmcheck_loc`). Lines and columns start at 1.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct uvmcheck_loc {
    /// Index into the design's file table; 0 is "no file".
    pub file: u32,
    /// 1-based line; 0 when unknown.
    pub line: u32,
    /// 1-based byte column.
    pub column: u32,
    /// For code from a macro: file of the outermost expansion.
    pub exp_file: u32,
    /// For code from a macro: line of the outermost expansion; 0 otherwise.
    pub exp_line: u32,
}

/// A class (`uvmcheck_class`); see `dv_model::Class`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_class {
    /// Name, NUL-terminated; valid until the design is freed.
    pub name: *const c_char,
    /// Enclosing package or class; empty at `$unit`.
    pub scope: *const c_char,
    /// Base class as written; empty when none.
    pub base_name: *const c_char,
    /// Base class id, or [`NONE`].
    pub base: u32,
    /// `UvmKind` as a number, in `dv_model::UvmKind` order.
    pub uvm: u8,
    /// Nonzero for a `virtual class`.
    pub is_virtual: u8,
    /// Nonzero for a parameterized class.
    pub is_parameterized: u8,
    /// Nonzero for a class from a library path.
    pub is_library: u8,
    /// Where it is declared or written.
    pub loc: uvmcheck_loc,
    /// Number of properties ([`uvmcheck_get_property`]).
    pub property_count: u32,
    /// Number of methods ([`uvmcheck_get_method`]).
    pub method_count: u32,
    /// Number of constraint blocks ([`uvmcheck_get_constraint`]).
    pub constraint_count: u32,
    /// Number of macro uses ([`uvmcheck_get_macro`]).
    pub macro_count: u32,
    /// Number of typedefs ([`uvmcheck_get_typedef`]).
    pub typedef_count: u32,
    /// Nonzero for an `interface class`.
    pub is_interface: u8,
}

/// A class property (`uvmcheck_property`); see `dv_model::Property`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_property {
    /// Name, NUL-terminated; valid until the design is freed.
    pub name: *const c_char,
    /// Type as printed by slang.
    pub type_: *const c_char,
    /// The type's class id, or [`NONE`].
    pub type_class: u32,
    /// 0 public, 1 protected, 2 local.
    pub visibility: u8,
    /// Nonzero when `static`.
    pub is_static: u8,
    /// Nonzero when `const`.
    pub is_const: u8,
    /// Nonzero when `rand` or `randc`.
    pub is_rand: u8,
    /// Nonzero for a virtual interface or an array of them.
    pub is_virtual_interface: u8,
    /// Where it is declared or written.
    pub loc: uvmcheck_loc,
}

/// A method (`uvmcheck_method`); see `dv_model::Method`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_method {
    /// Name, NUL-terminated; valid until the design is freed.
    pub name: *const c_char,
    /// 0 public, 1 protected, 2 local.
    pub visibility: u8,
    /// Nonzero for a task.
    pub is_task: u8,
    /// Nonzero for a virtual method (declared or by override).
    pub is_virtual: u8,
    /// Nonzero for `pure virtual`.
    pub is_pure: u8,
    /// Nonzero when `static`.
    pub is_static: u8,
    /// 0 not a phase method, 1 function phase, 2 task phase.
    pub phase: u8,
    /// Nonzero when the `virtual` keyword is written.
    pub declared_virtual: u8,
    /// Nonzero for a compiler-made method.
    pub is_builtin: u8,
    /// Nonzero when declared by a prototype in the class.
    pub is_extern: u8,
    /// Nonzero for tasks and void functions.
    pub returns_void: u8,
    /// Return type as written.
    pub return_type: *const c_char,
    /// Where it is declared or written.
    pub loc: uvmcheck_loc,
    /// Number of arguments ([`uvmcheck_get_argument`]).
    pub argument_count: u32,
    /// Number of locals ([`uvmcheck_get_local`]).
    pub local_count: u32,
    /// Number of calls ([`uvmcheck_get_call`]).
    pub call_count: u32,
    /// Number of assignments ([`uvmcheck_get_assignment`]).
    pub assignment_count: u32,
    /// Number of timing controls ([`uvmcheck_get_timing`]).
    pub timing_count: u32,
    /// Top-level statement index of `super.new(...);`, or -1.
    pub super_new_index: i32,
    /// Where that `super.new(...);` is.
    pub super_new_loc: uvmcheck_loc,
    /// Nonzero when `super.new` is called anywhere in the body.
    pub calls_super_new: u8,
}

/// An assignment in a method body (`uvmcheck_assignment`); see `dv_model::Assignment`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_assignment {
    /// Left side as written.
    pub lhs: *const c_char,
    /// Right side as written.
    pub rhs: *const c_char,
    /// The argument, local or property assigned into.
    pub root_name: *const c_char,
    /// Where it is declared or written.
    pub loc: uvmcheck_loc,
    /// Nonzero for `<=`.
    pub is_non_blocking: u8,
    /// 0 other, 1 argument, 2 local, 3 property.
    pub root_kind: u8,
}

/// A method argument (`uvmcheck_argument`); see `dv_model::Argument`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_argument {
    /// Name, NUL-terminated; valid until the design is freed.
    pub name: *const c_char,
    /// Type as printed by slang.
    pub type_: *const c_char,
    /// Type with typedefs resolved.
    pub canonical_type: *const c_char,
    /// The type's class id, or [`NONE`].
    pub type_class: u32,
    /// 0 input, 1 output, 2 inout, 3 ref.
    pub direction: u8,
    /// `TypeShape` as a number, in `dv_model::TypeShape` order.
    pub shape: u8,
    /// Where it is declared or written.
    pub loc: uvmcheck_loc,
}

/// A local variable of a method (`uvmcheck_local`); see `dv_model::Local`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_local {
    /// Name, NUL-terminated; valid until the design is freed.
    pub name: *const c_char,
    /// Type as printed by slang.
    pub type_: *const c_char,
    /// Nonzero for a virtual interface or an array of them.
    pub is_virtual_interface: u8,
    /// Where it is declared or written.
    pub loc: uvmcheck_loc,
}

/// A call in a method body (`uvmcheck_call`); see `dv_model::Call`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_call {
    /// Name, NUL-terminated; valid until the design is freed.
    pub name: *const c_char,
    /// Class or package declaring the callee; empty if none.
    pub callee_scope: *const c_char,
    /// Object called on, as written; empty if none.
    pub receiver: *const c_char,
    /// Where it is declared or written.
    pub loc: uvmcheck_loc,
    /// Where the first argument starts.
    pub first_arg_loc: uvmcheck_loc,
    /// Nonzero for a system or built-in call.
    pub is_system: u8,
    /// Nonzero when the callee has the `virtual` keyword.
    pub callee_declared_virtual: u8,
    /// Nonzero when the callee is compiler-made.
    pub callee_builtin: u8,
    /// Nonzero when the first argument is `null`.
    pub first_arg_is_null: u8,
    /// Nonzero for `super.name(...)`.
    pub via_super: u8,
}

/// A constraint block (`uvmcheck_constraint`); see `dv_model::Constraint`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_constraint {
    /// Name, NUL-terminated; valid until the design is freed.
    pub name: *const c_char,
    /// Where it is declared or written.
    pub loc: uvmcheck_loc,
}

/// A user macro used in a class (`uvmcheck_macro`); see `dv_model::MacroUse`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_macro {
    /// Name, NUL-terminated; valid until the design is freed.
    pub name: *const c_char,
    /// Where it is declared or written.
    pub loc: uvmcheck_loc,
}

/// A typedef in a class (`uvmcheck_typedef`); see `dv_model::Typedef`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_typedef {
    /// Name, NUL-terminated; valid until the design is freed.
    pub name: *const c_char,
    /// Where it is declared or written.
    pub loc: uvmcheck_loc,
}

extern "C" {
    /// The library's ABI version; must equal [`ABI_VERSION`].
    pub fn uvmcheck_model_abi_version() -> u32;
    /// Loads a design from compile options as given to uvmcheck (no program
    /// name). Never returns null; check [`uvmcheck_design_error`].
    pub fn uvmcheck_design_load(argc: c_int, argv: *const *const c_char) -> *mut uvmcheck_design;
    /// Frees a design and every string and record read from it.
    pub fn uvmcheck_design_free(d: *mut uvmcheck_design);
    /// Null when the design loaded (compile errors in the code do not count),
    /// else why it did not.
    pub fn uvmcheck_design_error(d: *const uvmcheck_design) -> *const c_char;
    /// Number of compile errors slang reported.
    pub fn uvmcheck_design_compile_errors(d: *const uvmcheck_design) -> u32;
    /// Milliseconds spent in a stage: 0 parse, 1 elaborate, 2 model build.
    pub fn uvmcheck_design_time_ms(d: *const uvmcheck_design, stage: c_int) -> f64;
    /// Size of the file table (entry 0 is "no file").
    pub fn uvmcheck_file_count(d: *const uvmcheck_design) -> u32;
    /// Name of file `file`.
    pub fn uvmcheck_file_name(d: *const uvmcheck_design, file: u32) -> *const c_char;
    /// Number of classes; ids run from 0.
    pub fn uvmcheck_class_count(d: *const uvmcheck_design) -> u32;
    /// Fills `*out` with class `cls` and returns 1,
    /// or returns 0 when an id is out of range.
    pub fn uvmcheck_get_class(d: *const uvmcheck_design, cls: u32, out: *mut uvmcheck_class) -> c_int;
    /// Fills `*out` with property `index` (of class `cls`) and returns 1,
    /// or returns 0 when an id is out of range.
    pub fn uvmcheck_get_property(
        d: *const uvmcheck_design,
        cls: u32,
        index: u32,
        out: *mut uvmcheck_property,
    ) -> c_int;
    /// Fills `*out` with method `index` (of class `cls`) and returns 1,
    /// or returns 0 when an id is out of range.
    pub fn uvmcheck_get_method(
        d: *const uvmcheck_design,
        cls: u32,
        index: u32,
        out: *mut uvmcheck_method,
    ) -> c_int;
    /// Fills `*out` with argument `index` (of class `cls`, method `method`) and returns 1,
    /// or returns 0 when an id is out of range.
    pub fn uvmcheck_get_argument(
        d: *const uvmcheck_design,
        cls: u32,
        method: u32,
        index: u32,
        out: *mut uvmcheck_argument,
    ) -> c_int;
    /// Fills `*out` with local `index` (of class `cls`, method `method`) and returns 1,
    /// or returns 0 when an id is out of range.
    pub fn uvmcheck_get_local(
        d: *const uvmcheck_design,
        cls: u32,
        method: u32,
        index: u32,
        out: *mut uvmcheck_local,
    ) -> c_int;
    /// Fills `*out` with call `index` (of class `cls`, method `method`) and returns 1,
    /// or returns 0 when an id is out of range.
    pub fn uvmcheck_get_call(
        d: *const uvmcheck_design,
        cls: u32,
        method: u32,
        index: u32,
        out: *mut uvmcheck_call,
    ) -> c_int;
    /// Fills `*out` with assignment `index` (of class `cls`, method `method`) and returns 1,
    /// or returns 0 when an id is out of range.
    pub fn uvmcheck_get_assignment(
        d: *const uvmcheck_design,
        cls: u32,
        method: u32,
        index: u32,
        out: *mut uvmcheck_assignment,
    ) -> c_int;
    /// Fills `*out` with the location of timing control `index` (of class
    /// `cls`, method `method`) and returns 1,
    /// or returns 0 when an id is out of range.
    pub fn uvmcheck_get_timing(
        d: *const uvmcheck_design,
        cls: u32,
        method: u32,
        index: u32,
        out: *mut uvmcheck_loc,
    ) -> c_int;
    /// Fills `*out` with constraint block `index` (of class `cls`) and returns 1,
    /// or returns 0 when an id is out of range.
    pub fn uvmcheck_get_constraint(
        d: *const uvmcheck_design,
        cls: u32,
        index: u32,
        out: *mut uvmcheck_constraint,
    ) -> c_int;
    /// Fills `*out` with macro use `index` (of class `cls`) and returns 1,
    /// or returns 0 when an id is out of range.
    pub fn uvmcheck_get_macro(d: *const uvmcheck_design, cls: u32, index: u32, out: *mut uvmcheck_macro) -> c_int;
    /// Fills `*out` with typedef `index` (of class `cls`) and returns 1,
    /// or returns 0 when an id is out of range.
    pub fn uvmcheck_get_typedef(
        d: *const uvmcheck_design,
        cls: u32,
        index: u32,
        out: *mut uvmcheck_typedef,
    ) -> c_int;
    /// The whole model as JSON (schema `uvmcheck.model/1`), made on first call.
    pub fn uvmcheck_design_json(d: *mut uvmcheck_design) -> *const c_char;
}

#[cfg(test)]
mod tests {
    #[test]
    fn layout_matches_the_c_header() {
        use std::mem::size_of;
        // Pointer-sized fields first, then fixed-width ones, as in CApi.h.
        assert_eq!(size_of::<super::uvmcheck_loc>(), 20);
        // 3 pointers, base, 4 flags, loc, 5 counts, is_interface, padded to 8.
        assert_eq!(size_of::<super::uvmcheck_class>(), 80);
        assert_eq!(size_of::<super::uvmcheck_macro>(), 32);
        assert_eq!(size_of::<super::uvmcheck_typedef>(), 32);
        // Sizes with the padding a C compiler adds (8-byte pointers).
        assert_eq!(size_of::<super::uvmcheck_property>(), 48);
        assert_eq!(size_of::<super::uvmcheck_method>(), 104);
        assert_eq!(size_of::<super::uvmcheck_assignment>(), 48);
        assert_eq!(size_of::<super::uvmcheck_argument>(), 56);
        assert_eq!(size_of::<super::uvmcheck_local>(), 40);
        assert_eq!(size_of::<super::uvmcheck_constraint>(), 32);
        assert_eq!(size_of::<super::uvmcheck_call>(), 72);
        assert_eq!(unsafe { super::uvmcheck_model_abi_version() }, super::ABI_VERSION);
    }
}
