//! Whether a class is a UVM component or a UVM object.


use dv_model::{Class, Design};

use crate::derives_from;

pub(crate) fn is_component(d: &Design, c: &Class) -> bool {
    derives_from(d, Some(c.id), "uvm_component")
}

pub(crate) fn is_object(d: &Design, c: &Class) -> bool {
    derives_from(d, Some(c.id), "uvm_object")
}
