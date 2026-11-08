//! Group A rules that read only the class model: roles of classes and of
//! the classes their properties hold (MG-2, MG-3, MG-6).

use dv_model::{Design, Direction};

use crate::g_methodology::{class_methods, is_library, property_class, role, role_name, user_classes, Role};
use crate::Check::Model;
use crate::{Rule, Severity, Sink};

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "UVM-ROL-002", name: "CoverageOutsideObservers", check: Model(coverage_outside_observers) },
        Rule { id: "UVM-ROL-003", name: "DutCheckingInAgent", check: Model(dut_checking_in_agent) },
        Rule { id: "UVM-ROL-005", name: "MonitorDependsOnActivePath", check: Model(monitor_depends_on_active_path) },
        Rule { id: "UVM-CPL-002", name: "HandleToEnvOrTest", check: Model(handle_to_env_or_test) },
        Rule { id: "SV-CNS-008", name: "FunctionOutputArguments", check: Model(function_output_arguments) },
    ]
}

fn coverage_outside_observers(d: &Design, out: &mut Sink) {
    for cls in user_classes(d) {
        let r = role(d, cls);
        if r != Role::Driver && r != Role::Sequence {
            continue;
        }
        for p in &cls.properties {
            if !p.ty.contains("covergroup") {
                continue;
            }
            out.add(
                "UVM-ROL-002",
                "CoverageOutsideObservers",
                Severity::Warning,
                format!(
                    "{} '{}' holds covergroup '{}'. Coverage sampled here records stimulus that was requested, even if the DUT never saw it, and is \
lost when the agent runs passive. Sample it in a monitor or a coverage subscriber fed from the monitor's analysis port.",
                    role_name(r),
                    cls.name,
                    p.name
                ),
                p.loc,
            );
        }
    }
}

fn is_checker_class(d: &Design, c: &dv_model::Class) -> bool {
    if is_library(d, c) {
        return false;
    }
    if role(d, c) == Role::Scoreboard {
        return true;
    }
    let n = c.name.to_ascii_lowercase();
    ["scoreboard", "predictor", "ref_model", "refmodel", "reference_model"]
        .iter()
        .any(|w| n.contains(w))
}

fn dut_checking_in_agent(d: &Design, out: &mut Sink) {
    for cls in user_classes(d) {
        if role(d, cls) != Role::Agent {
            continue;
        }
        for p in &cls.properties {
            let Some(t) = property_class(d, cls, p) else { continue };
            if !is_checker_class(d, t) {
                continue;
            }
            out.add(
                "UVM-ROL-003",
                "DutCheckingInAgent",
                Severity::Warning,
                format!(
                    "Agent '{}' contains '{}' of type '{}'. Expected-value models and scoreboards are specific to one DUT, so the agent cannot be reused for another \
one. Build them in the env and connect them to the agent's analysis port.",
                    cls.name, p.name, t.name
                ),
                p.loc,
            );
        }
    }
}

fn monitor_depends_on_active_path(d: &Design, out: &mut Sink) {
    for cls in user_classes(d) {
        if role(d, cls) != Role::Monitor {
            continue;
        }
        for p in &cls.properties {
            let Some(t) = property_class(d, cls, p) else { continue };
            let r = role(d, t);
            let item_port = t.name.starts_with("uvm_seq_item_pull");
            if r != Role::Driver && r != Role::Sequencer && !item_port {
                continue;
            }
            out.add(
                "UVM-ROL-005",
                "MonitorDependsOnActivePath",
                Severity::Warning,
                format!(
                    "Monitor '{}' holds '{}' of type '{}'. Passive agents have no driver or sequencer, so the monitor fails there; observe the interface only \
and publish what you see on an analysis port.",
                    cls.name, p.name, t.name
                ),
                p.loc,
            );
        }
    }
}

fn handle_to_env_or_test(d: &Design, out: &mut Sink) {
    for cls in user_classes(d) {
        let r0 = role(d, cls);
        if r0 == Role::Env || r0 == Role::Test {
            continue;
        }
        for p in &cls.properties {
            let Some(t) = property_class(d, cls, p) else { continue };
            if is_library(d, t) {
                continue;
            }
            let r = role(d, t);
            if r != Role::Env && r != Role::Test {
                continue;
            }
            out.add(
                "UVM-CPL-002",
                "HandleToEnvOrTest",
                Severity::Warning,
                format!(
                    "{} '{}' stores '{}', a handle to {} '{}'. Everything below the env now depends on the env's structure. Pass in only what is used: a config object, a \
sequencer handle or a TLM port.",
                    role_name(r0),
                    cls.name,
                    p.name,
                    role_name(r),
                    t.name
                ),
                p.loc,
            );
        }
    }
}

/// `getOverride()`: a base class (user or library) has a virtual method of
/// the same name.
pub(crate) fn overrides_base(d: &Design, cls: &dv_model::Class, name: &str) -> bool {
    d.ancestors(cls.id)
        .any(|a| a.methods.iter().any(|m| m.name == name && m.is_virtual))
}

fn function_output_arguments(d: &Design, out: &mut Sink) {
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
