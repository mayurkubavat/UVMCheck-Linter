//! Raw bindings to `include/uvmcheck/model/CApi.h`. Keep this file in step
//! with that header; `dv-model` is the safe API tools should use.
#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_int};

pub const ABI_VERSION: u32 = 2;
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
    pub fn uvmcheck_design_json(d: *mut uvmcheck_design) -> *const c_char;
}

#[cfg(test)]
mod tests {
    #[test]
    fn layout_matches_the_c_header() {
        use std::mem::size_of;
        // Pointer-sized fields first, then fixed-width ones, as in CApi.h.
        assert_eq!(size_of::<super::uvmcheck_loc>(), 12);
        assert_eq!(size_of::<super::uvmcheck_class>(), 3 * 8 + 4 + 4 + 12 + 8 + 4);
        assert_eq!(unsafe { super::uvmcheck_model_abi_version() }, super::ABI_VERSION);
    }
}
