//! Whether a method overrides one of a base class.

use dv_model::Design;

/// `getOverride()`: a base class (user or library) has a virtual method of
/// the same name.
pub(crate) fn overrides_base(d: &Design, cls: &dv_model::Class, name: &str) -> bool {
    d.ancestors(cls.id).any(|a| a.methods.iter().any(|m| m.name == name && m.is_virtual))
}
