//! SV-CNS-008 (FunctionOutputArguments): Flags void class functions that hand
//! their results back through output arguments.

use dv_model::{Design, Direction};
use crate::support::class_roles::{class_methods, user_classes};
use crate::{Severity, Sink};
use crate::support::overrides::{overrides_base};

pub(crate) fn function_output_arguments(d: &Design, out: &mut Sink) {
    for cls in user_classes(d) {
        for m in class_methods(cls) {
            if m.is_task || !m.returns_void || overrides_base(d, cls, &m.name) {
                continue;
            }
            let outs: Vec<&str> = m
                .arguments
                .iter()
                .filter(|a| a.direction == Direction::Out)
                .map(|a| a.name.as_str())
                .collect();
            if outs.is_empty() {
                continue;
            }
            let list = outs.iter().map(|n| format!("'{n}'")).collect::<Vec<_>>().join(", ");
            out.add(
                "SV-CNS-008",
                "FunctionOutputArguments",
                Severity::Info,
                format!(
                    "void function '{}::{}' returns its result through output argument{}{}. {}",
                    cls.name,
                    m.name,
                    if outs.len() > 1 { "s " } else { " " },
                    list,
                    if outs.len() > 1 { "Return a struct that holds them instead." } else { "Return the value instead." }
                ),
                m.loc,
            );
        }
    }
}
