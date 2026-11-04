//! Wave 2: factory registration, phase super calls and do_* hooks, on the
//! model's macro uses and typedefs (ported from src/rules/uvm/).

use std::collections::HashMap;

use dv_model::{Class, Design};

use crate::{derives_from, find_method, find_property, is_function_phase, is_task_phase, Rule, Severity, Sink};

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "UVM-FAC-003", name: "UvmFactoryMissingUtils", run: uvm_factory_missing_utils },
        Rule { id: "UVM-FAC-002", name: "UvmFactoryMacroMismatch", run: uvm_factory_macro_mismatch },
        Rule { id: "UVM-PHS-001", name: "UvmMissingSuperPhase", run: uvm_missing_super_phase },
        Rule { id: "UVM-BAN-003", name: "DirectDoMethodCall", run: direct_do_method_call },
    ]
}

fn is_component(d: &Design, c: &Class) -> bool {
    derives_from(d, Some(c.id), "uvm_component")
}

fn is_object(d: &Design, c: &Class) -> bool {
    derives_from(d, Some(c.id), "uvm_object")
}

fn uvm_factory_missing_utils(d: &Design, out: &mut Sink) {
    const REGISTERS: [&str; 4] =
        ["uvm_component_utils", "uvm_object_utils", "uvm_component_param_utils", "uvm_object_param_utils"];
    for cls in d.classes() {
        if cls.is_parameterized || cls.is_virtual || cls.is_interface || cls.name.starts_with("uvm_") {
            continue;
        }
        let (comp, obj) = (is_component(d, cls), is_object(d, cls));
        if !comp && !obj {
            continue;
        }
        // A member of its own named type_id or get_type, or a registration macro.
        let declares = |n: &str| {
            cls.typedefs.iter().any(|t| t.name == n) || find_property(cls, n).is_some() || find_method(cls, n).is_some()
        };
        if declares("type_id") || declares("get_type") {
            continue;
        }
        if cls.macros.iter().any(|m| REGISTERS.iter().any(|r| m.name.starts_with(r))) {
            continue;
        }
        let (kind, mac) = if comp { ("uvm_component", "`uvm_component_utils") } else { ("uvm_object", "`uvm_object_utils") };
        out.add(
            "UVM-FAC-003",
            "UvmFactoryMissingUtils",
            Severity::Warning,
            format!(
                "Concrete class '{0}' derived from '{kind}' is not registered with the UVM factory. Add '{mac}({0})' to enable testbench factory overrides.",
                cls.name
            ),
            cls.loc,
        );
    }
}

fn uvm_factory_macro_mismatch(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        let (comp, obj) = (is_component(d, cls), is_object(d, cls));
        if !comp && !obj {
            continue;
        }
        for m in &cls.macros {
            if comp && m.name.starts_with("uvm_object_utils") {
                out.add(
                    "UVM-FAC-002",
                    "UvmFactoryMacroMismatch",
                    Severity::Error,
                    format!(
                        "Class '{}' derives from 'uvm_component' but declares '`{}'. Use '`uvm_component_utils' \
for components to support hierarchical factory registration.",
                        cls.name, m.name
                    ),
                    m.loc,
                );
            } else if !comp && obj && m.name.starts_with("uvm_component_utils") {
                out.add(
                    "UVM-FAC-002",
                    "UvmFactoryMacroMismatch",
                    Severity::Error,
                    format!(
                        "Class '{}' derives from 'uvm_object' (not a component) but declares '`{}'. Use '`uvm_object_utils' \
because non-component objects do not have parent hierarchy contexts.",
                        cls.name, m.name
                    ),
                    m.loc,
                );
            }
        }
        // The C++ rule reads the registry from how slang prints the type_id
        // alias, which is the alias's own path (pkg::cls::type_id).
        for t in cls.typedefs.iter().filter(|t| t.name == "type_id") {
            let printed = if cls.scope.is_empty() {
                format!("{}::{}", cls.name, t.name)
            } else {
                format!("{}::{}::{}", cls.scope, cls.name, t.name)
            };
            if comp && printed.contains("uvm_object_registry") {
                out.add(
                    "UVM-FAC-002",
                    "UvmFactoryMacroMismatch",
                    Severity::Error,
                    format!(
                        "Class '{}' derives from 'uvm_component' but its factory registry is 'uvm_object_registry'. Use 'uvm_component_registry'.",
                        cls.name
                    ),
                    t.loc,
                );
            } else if !comp && obj && printed.contains("uvm_component_registry") {
                out.add(
                    "UVM-FAC-002",
                    "UvmFactoryMacroMismatch",
                    Severity::Error,
                    format!(
                        "Class '{}' derives from 'uvm_object' but its factory registry is 'uvm_component_registry'. Use 'uvm_object_registry'.",
                        cls.name
                    ),
                    t.loc,
                );
            }
        }
    }
}

fn uvm_missing_super_phase(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || cls.name == "uvm_component" || !is_component(d, cls) {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || !(is_function_phase(&sub.name) || is_task_phase(&sub.name)) {
                continue;
            }
            if sub.calls.iter().any(|c| c.name == sub.name) {
                continue;
            }
            out.add(
                "UVM-PHS-001",
                "UvmMissingSuperPhase",
                Severity::Warning,
                format!("Method '{0}' in class '{1}' does not call super.{0}(phase).", sub.name, cls.name),
                sub.loc,
            );
        }
    }
}

fn direct_do_method_call(d: &Design, out: &mut Sink) {
    const PUBLIC: [(&str, &str); 6] = [
        ("do_copy", "copy()"),
        ("do_compare", "compare()"),
        ("do_print", "print() or sprint()"),
        ("do_pack", "pack()"),
        ("do_unpack", "unpack()"),
        ("do_record", "record()"),
    ];
    // Callee classes by name: the first class of each name, as findClass.
    let mut by_name: HashMap<&str, &Class> = HashMap::new();
    for c in d.classes() {
        by_name.entry(c.name.as_str()).or_insert(c);
    }
    for cls in d.classes() {
        for sub in &cls.methods {
            for call in &sub.calls {
                if call.is_system || call.via_super {
                    continue;
                }
                let Some(&(_, public)) = PUBLIC.iter().find(|(n, _)| *n == call.name) else { continue };
                let Some(owner) = by_name.get(call.callee_scope.as_str()) else { continue };
                if !is_object(d, owner) {
                    continue;
                }
                out.add(
                    "UVM-BAN-003",
                    "DirectDoMethodCall",
                    Severity::Warning,
                    format!(
                        "Direct call to {}(). The do_* methods are hooks that the UVM \
policy classes call from {public}; calling them directly skips the \
policy object (comparer, printer, packer, recorder) and the field automation. Call {public} instead.",
                        call.name
                    ),
                    call.loc,
                );
            }
        }
    }
}
