//! Raw bindings to `include/uvmcheck/model/CApi.h`. Keep this file in step
//! with that header; `dv-model` is the safe API tools should use.
#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_int};

pub const ABI_VERSION: u32 = 5;
pub const NONE: u32 = u32::MAX;

#[repr(C)]
pub struct uvmcheck_design {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct uvmcheck_loc {
    pub file: u32,
    pub line: u32,
    pub column: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_class {
    pub name: *const c_char,
    pub scope: *const c_char,
    pub base_name: *const c_char,
    pub base: u32,
    pub uvm: u8,
    pub is_virtual: u8,
    pub is_parameterized: u8,
    pub is_library: u8,
    pub loc: uvmcheck_loc,
    pub property_count: u32,
    pub method_count: u32,
    pub constraint_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_property {
    pub name: *const c_char,
    pub type_: *const c_char,
    pub type_class: u32,
    pub visibility: u8,
    pub is_static: u8,
    pub is_const: u8,
    pub is_rand: u8,
    pub is_virtual_interface: u8,
    pub loc: uvmcheck_loc,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_method {
    pub name: *const c_char,
    pub visibility: u8,
    pub is_task: u8,
    pub is_virtual: u8,
    pub is_pure: u8,
    pub is_static: u8,
    pub phase: u8,
    pub declared_virtual: u8,
    pub is_builtin: u8,
    pub is_extern: u8,
    pub returns_void: u8,
    pub return_type: *const c_char,
    pub loc: uvmcheck_loc,
    pub argument_count: u32,
    pub local_count: u32,
    pub call_count: u32,
    pub assignment_count: u32,
    pub timing_count: u32,
    pub super_new_index: i32,
    pub super_new_loc: uvmcheck_loc,
    pub calls_super_new: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_assignment {
    pub lhs: *const c_char,
    pub rhs: *const c_char,
    pub root_name: *const c_char,
    pub loc: uvmcheck_loc,
    pub is_non_blocking: u8,
    pub root_kind: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_argument {
    pub name: *const c_char,
    pub type_: *const c_char,
    pub canonical_type: *const c_char,
    pub type_class: u32,
    pub direction: u8,
    pub shape: u8,
    pub loc: uvmcheck_loc,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_local {
    pub name: *const c_char,
    pub type_: *const c_char,
    pub is_virtual_interface: u8,
    pub loc: uvmcheck_loc,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_call {
    pub name: *const c_char,
    pub callee_scope: *const c_char,
    pub receiver: *const c_char,
    pub loc: uvmcheck_loc,
    pub first_arg_loc: uvmcheck_loc,
    pub is_system: u8,
    pub callee_declared_virtual: u8,
    pub callee_builtin: u8,
    pub first_arg_is_null: u8,
    pub via_super: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct uvmcheck_constraint {
    pub name: *const c_char,
    pub loc: uvmcheck_loc,
}

extern "C" {
    pub fn uvmcheck_model_abi_version() -> u32;
    pub fn uvmcheck_design_load(argc: c_int, argv: *const *const c_char) -> *mut uvmcheck_design;
    pub fn uvmcheck_design_free(d: *mut uvmcheck_design);
    pub fn uvmcheck_design_error(d: *const uvmcheck_design) -> *const c_char;
    pub fn uvmcheck_design_compile_errors(d: *const uvmcheck_design) -> u32;
    pub fn uvmcheck_design_time_ms(d: *const uvmcheck_design, stage: c_int) -> f64;
    pub fn uvmcheck_file_count(d: *const uvmcheck_design) -> u32;
    pub fn uvmcheck_file_name(d: *const uvmcheck_design, file: u32) -> *const c_char;
    pub fn uvmcheck_class_count(d: *const uvmcheck_design) -> u32;
    pub fn uvmcheck_get_class(d: *const uvmcheck_design, cls: u32, out: *mut uvmcheck_class) -> c_int;
    pub fn uvmcheck_get_property(
        d: *const uvmcheck_design,
        cls: u32,
        index: u32,
        out: *mut uvmcheck_property,
    ) -> c_int;
    pub fn uvmcheck_get_method(
        d: *const uvmcheck_design,
        cls: u32,
        index: u32,
        out: *mut uvmcheck_method,
    ) -> c_int;
    pub fn uvmcheck_get_argument(
        d: *const uvmcheck_design,
        cls: u32,
        method: u32,
        index: u32,
        out: *mut uvmcheck_argument,
    ) -> c_int;
    pub fn uvmcheck_get_local(
        d: *const uvmcheck_design,
        cls: u32,
        method: u32,
        index: u32,
        out: *mut uvmcheck_local,
    ) -> c_int;
    pub fn uvmcheck_get_call(
        d: *const uvmcheck_design,
        cls: u32,
        method: u32,
        index: u32,
        out: *mut uvmcheck_call,
    ) -> c_int;
    pub fn uvmcheck_get_assignment(
        d: *const uvmcheck_design,
        cls: u32,
        method: u32,
        index: u32,
        out: *mut uvmcheck_assignment,
    ) -> c_int;
    pub fn uvmcheck_get_timing(
        d: *const uvmcheck_design,
        cls: u32,
        method: u32,
        index: u32,
        out: *mut uvmcheck_loc,
    ) -> c_int;
    pub fn uvmcheck_get_constraint(
        d: *const uvmcheck_design,
        cls: u32,
        index: u32,
        out: *mut uvmcheck_constraint,
    ) -> c_int;
    pub fn uvmcheck_design_json(d: *mut uvmcheck_design) -> *const c_char;
}

#[cfg(test)]
mod tests {
    #[test]
    fn layout_matches_the_c_header() {
        use std::mem::size_of;
        // Pointer-sized fields first, then fixed-width ones, as in CApi.h.
        assert_eq!(size_of::<super::uvmcheck_loc>(), 12);
        assert_eq!(size_of::<super::uvmcheck_class>(), 3 * 8 + 4 + 4 + 12 + 12);
        // Sizes with the padding a C compiler adds (8-byte pointers).
        assert_eq!(size_of::<super::uvmcheck_property>(), 40);
        assert_eq!(size_of::<super::uvmcheck_method>(), 88);
        assert_eq!(size_of::<super::uvmcheck_assignment>(), 40);
        assert_eq!(size_of::<super::uvmcheck_argument>(), 48);
        assert_eq!(size_of::<super::uvmcheck_local>(), 32);
        assert_eq!(size_of::<super::uvmcheck_constraint>(), 24);
        assert_eq!(size_of::<super::uvmcheck_call>(), 56);
        assert_eq!(unsafe { super::uvmcheck_model_abi_version() }, super::ABI_VERSION);
    }
}
