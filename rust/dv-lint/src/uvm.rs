//! UVM rules: phases, objections, TLM, config_db, sequences and RAL.

use dv_model::{Design, PhaseKind, RootKind, UvmKind};

use crate::{derives_from, find_property, is_function_phase, is_task_phase, Rule, Severity, Sink};

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "UVM-PHS-009",
            name: "ObjectionInNonRunPhase",
            check: crate::Check::Model(objection_in_non_run_phase),
        },
        Rule {
            id: "UVM-OBS-001",
            name: "ObserverStateMutationInWrite",
            check: crate::Check::Model(observer_state_mutation_in_write),
        },
        Rule {
            id: "RAL-WIRE-001",
            name: "RalPredictorAdapterWiring",
            check: crate::Check::Model(ral_predictor_adapter_wiring),
        },
        Rule {
            id: "UVM-CFG-002",
            name: "UvmConfigDbNullContext",
            check: crate::Check::Model(uvm_config_db_null_context),
        },
        Rule {
            id: "UVM-SEQ-003",
            name: "UvmPSequencerAbuse",
            check: crate::Check::Model(uvm_p_sequencer_abuse),
        },
        Rule {
            id: "UVM-PHS-003",
            name: "UvmPhaseSignatureMismatch",
            check: crate::Check::Model(uvm_phase_signature_mismatch),
        },
        Rule {
            id: "UVM-SEQ-001",
            name: "UvmSequenceDirectVifAccess",
            check: crate::Check::Model(uvm_sequence_direct_vif_access),
        },
        Rule {
            id: "UVM-SEQ-002",
            name: "UvmSequencePrePostBodyMisuse",
            check: crate::Check::Model(uvm_sequence_pre_post_body_misuse),
        },
        Rule {
            id: "UVM-PHS-002",
            name: "UvmTimeInFunctionPhase",
            check: crate::Check::Model(uvm_time_in_function_phase),
        },
        Rule {
            id: "UVM-TLM-001",
            name: "UvmUnconnectedTlmPort",
            check: crate::Check::Model(uvm_unconnected_tlm_port),
        },
        Rule {
            id: "UVM-OBJ-001",
            name: "UvmUnpairedObjection",
            check: crate::Check::Model(uvm_unpaired_objection),
        },
        Rule {
            id: "UVM-VIF-DIR",
            name: "VirtualInterfaceModportDriveInversion",
            check: crate::Check::Model(virtual_interface_modport_drive_inversion),
        },
        Rule {
            id: "UVM-VSEQ-001",
            name: "VirtualSequencerWiringChecker",
            check: crate::Check::Model(virtual_sequencer_wiring_checker),
        },
    ]
}

fn objection_in_non_run_phase(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.phase != PhaseKind::Function {
                continue;
            }
            for c in &sub.calls {
                if c.name != "raise_objection" && c.name != "drop_objection" {
                    continue;
                }
                out.add(
                    "UVM-PHS-009",
                    "ObjectionInNonRunPhase",
                    Severity::Error,
                    format!(
                        "Objection method '.{}()' called inside function phase '{}'. \
In UVM methodology (IEEE 1800.2 § 9.2), phase objections control progression and completion strictly \
during runtime phases ('run_phase' and scheduled sub-phases). Function phases execute synchronously in \
zero simulation time; raising or dropping objections in function phases is prohibited and corrupts UVM objection counting.",
                        c.name, sub.name
                    ),
                    c.loc,
                );
            }
        }
    }
}

fn observer_state_mutation_in_write(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.name != "write" || sub.is_task {
                continue;
            }
            let Some(a0) = sub.arguments.first() else {
                continue;
            };
            if a0.type_class.is_none() {
                continue;
            }
            let arg = &a0.name;
            for a in &sub.assignments {
                if a.root_kind != RootKind::Argument || &a.root_name != arg {
                    continue;
                }
                out.add(
                    "UVM-OBS-001",
                    "ObserverStateMutationInWrite",
                    Severity::Error,
                    format!(
                        "In-place mutation of observed transaction argument '{arg}' in 'write()' method of class '{}'. \
In the UVM Observer pattern, the same transaction handle is broadcast to multiple subscribers \
(scoreboards, functional coverage, checkers). Mutating transaction fields directly corrupts data \
for all downstream observers. Clone the transaction ($cast(cloned_t, {arg}.clone())) before mutating.",
                        cls.name
                    ),
                    a.loc,
                );
            }
        }
    }
}

fn ral_predictor_adapter_wiring(d: &Design, out: &mut Sink) {
    let mut predictors = Vec::new();
    for cls in d.classes() {
        if cls.is_parameterized || !cls.uvm.is_component() {
            continue;
        }
        for p in &cls.properties {
            if p.ty.contains("reg_predictor") {
                predictors.push((cls, p));
            }
        }
    }
    if predictors.is_empty() {
        return;
    }
    let assignments = connect_phase_assignments(d);
    for (owner, p) in predictors {
        let target = format!("{}.map", p.name);
        if assignments.iter().any(|s| s.contains(&target)) {
            continue;
        }
        out.add(
            "RAL-WIRE-001",
            "RalPredictorAdapterWiring",
            Severity::Warning,
            format!(
                "Register predictor '{0}' in component '{1}' is missing mandatory '.map' assignment in connect_phase (e.g. '{0}.map = reg_model.default_map;'). \
Without map assignment, register prediction will fail silently and mirror values will not update.",
                p.name, owner.name
            ),
            p.loc,
        );
    }
}

fn uvm_config_db_null_context(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || !cls.uvm.is_component() {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            for c in &sub.calls {
                if c.is_system || (c.name != "get" && c.name != "set") {
                    continue;
                }
                if !c.callee_scope.contains("uvm_config_db") || !c.first_arg_is_null {
                    continue;
                }
                out.add(
                    "UVM-CFG-002",
                    "UvmConfigDbNullContext",
                    Severity::Warning,
                    format!(
                        "Passing 'null' as context to uvm_config_db::{} inside uvm_component '{}'. Pass 'this' instead to preserve hierarchical scoping.",
                        c.name, cls.name
                    ),
                    c.first_arg_loc,
                );
            }
        }
    }
}

fn uvm_p_sequencer_abuse(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || cls.uvm != UvmKind::Sequence {
            continue;
        }
        let Some(p) = find_property(cls, "p_sequencer") else {
            continue;
        };
        out.add(
            "UVM-SEQ-003",
            "UvmPSequencerAbuse",
            Severity::Warning,
            format!(
                "Sequence '{}' declares 'p_sequencer' via `uvm_declare_p_sequencer. \
This violates the Law of Demeter and architectural layering by tightly binding sequence scenario generation \
to physical testbench agent/sequencer topology. If sequencer topology or naming changes, sequences break. \
Decouple sequences by passing configuration parameters down through sequence properties or uvm_config_db.",
                cls.name
            ),
            p.loc,
        );
    }
}

fn uvm_phase_signature_mismatch(d: &Design, out: &mut Sink) {
    const ID: &str = "UVM-PHS-003";
    const NAME: &str = "UvmPhaseSignatureMismatch";
    for cls in d.classes() {
        if cls.is_parameterized || !cls.uvm.is_component() {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            let n = &sub.name;
            let (fp, tp) = (is_function_phase(n), is_task_phase(n));
            if !fp && !tp {
                continue;
            }
            let at = sub.loc;
            if fp {
                if sub.is_task {
                    out.add(
                        ID,
                        NAME,
                        Severity::Error,
                        format!(
                            "UVM phase '{n}' in class '{}' is declared as a 'task'. Standard UVM function phases must be declared as 'function void'.",
                            cls.name
                        ),
                        at,
                    );
                } else if !sub.returns_void {
                    out.add(
                        ID,
                        NAME,
                        Severity::Error,
                        format!(
                            "UVM phase '{n}' in class '{}' has return type '{}'. Standard UVM phases must return 'void'.",
                            cls.name, sub.return_type
                        ),
                        at,
                    );
                }
            }
            if tp && !sub.is_task {
                out.add(
                    ID,
                    NAME,
                    Severity::Error,
                    format!(
                        "UVM phase '{n}' in class '{}' is declared as a 'function'. Time-consuming UVM phases must be declared as 'task'.",
                        cls.name
                    ),
                    at,
                );
            }
            if sub.arguments.len() != 1 {
                out.add(
                    ID,
                    NAME,
                    Severity::Error,
                    format!(
                        "UVM phase '{n}' in class '{}' declares {} argument(s), but standard UVM phases must declare exactly one argument of type 'uvm_phase'.",
                        cls.name,
                        sub.arguments.len()
                    ),
                    at,
                );
            } else {
                let a = &sub.arguments[0];
                let ok = (a.type_class.is_some() && derives_from(d, a.type_class, "uvm_phase"))
                    || a.ty.contains("uvm_phase");
                if !ok {
                    out.add(
                        ID,
                        NAME,
                        Severity::Error,
                        format!(
                            "Argument '{}' of UVM phase '{n}' in class '{}' has type '{}', but must be of type 'uvm_phase'.",
                            a.name, cls.name, a.ty
                        ),
                        a.loc,
                    );
                }
            }
        }
    }
}

/// Every `lhs = rhs` in any class's connect_phase, parameterized ones included.
fn connect_phase_assignments(d: &Design) -> Vec<String> {
    d.classes()
        .iter()
        .flat_map(|c| c.methods.iter())
        .filter(|m| m.name == "connect_phase")
        .flat_map(|m| m.assignments.iter())
        .map(|a| format!("{} = {}", a.lhs, a.rhs))
        .collect()
}

fn uvm_sequence_direct_vif_access(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        let is_seq = cls.uvm == UvmKind::Sequence;
        if !is_seq && cls.uvm != UvmKind::SequenceItem {
            continue;
        }
        let kind = if is_seq { "Sequence" } else { "Sequence item" };
        for p in &cls.properties {
            if !p.is_virtual_interface {
                continue;
            }
            out.add(
                "UVM-SEQ-001",
                "UvmSequenceDirectVifAccess",
                Severity::Warning,
                format!(
                    "{kind} class '{}' directly declares virtual interface property '{}'. Sequences must remain protocol-abstract and transaction-level; \
pin-level driving is the responsibility of the driver component.",
                    cls.name, p.name
                ),
                p.loc,
            );
        }
        for sub in &cls.methods {
            if sub.is_extern {
                continue;
            }
            for v in &sub.locals {
                if !v.is_virtual_interface {
                    continue;
                }
                out.add(
                    "UVM-SEQ-001",
                    "UvmSequenceDirectVifAccess",
                    Severity::Warning,
                    format!(
                        "Method '{}' in sequence class '{}' declares virtual interface variable '{}'. Pin-level driving must be confined to drivers.",
                        sub.name, cls.name, v.name
                    ),
                    v.loc,
                );
            }
        }
    }
}

fn uvm_sequence_pre_post_body_misuse(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || cls.uvm != UvmKind::Sequence {
            continue;
        }
        for m in &cls.methods {
            if m.is_extern || (m.name != "pre_body" && m.name != "post_body") {
                continue;
            }
            out.add(
                "UVM-SEQ-002",
                "UvmSequencePrePostBodyMisuse",
                Severity::Warning,
                format!(
                    "Sequence '{}' overrides lifecycle callback '{}()'. \
In UVM sequence architecture, pre_body() and post_body() are explicitly skipped when a sequence is executed as a child sequence \
via `uvm_do or `uvm_do_with. Critical setup or cleanup logic will silently fail to execute when nested. \
Migrate logic to pre_start() / post_start() (which execute in all modes) or body().",
                    cls.name, m.name
                ),
                m.loc,
            );
        }
    }
}

fn uvm_time_in_function_phase(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || !cls.uvm.is_component() {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.phase != PhaseKind::Function {
                continue;
            }
            // The last one, as the C++ rule reports.
            let Some(&last) = sub.timing_controls.last() else {
                continue;
            };
            out.add(
                "UVM-PHS-002",
                "UvmTimeInFunctionPhase",
                Severity::Error,
                format!(
                    "Time-consuming statement detected inside function phase '{}' in class '{}'.",
                    sub.name, cls.name
                ),
                last,
            );
        }
    }
}

fn uvm_unconnected_tlm_port(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || !cls.uvm.is_component() {
            continue;
        }
        let ports: Vec<_> = cls
            .properties
            .iter()
            .filter(|p| p.ty.contains("_port") || p.ty.contains("_export"))
            .collect();
        if ports.is_empty() {
            continue;
        }
        let connected: Vec<&str> = cls
            .methods
            .iter()
            .filter(|m| !m.is_extern && !m.is_builtin)
            .flat_map(|m| m.calls.iter())
            .filter(|c| c.name == "connect" && !c.receiver.is_empty())
            .map(|c| c.receiver.as_str())
            .collect();
        for p in ports {
            if connected.iter().any(|r| r.ends_with(p.name.as_str())) {
                continue;
            }
            out.add(
                "UVM-TLM-001",
                "UvmUnconnectedTlmPort",
                Severity::Warning,
                format!(
                    "TLM port/export '{}' in component '{}' is declared but never connected via .connect(...). \
Unconnected TLM ports can cause simulation fatal errors upon transaction transfer.",
                    p.name, cls.name
                ),
                p.loc,
            );
        }
    }
}

fn uvm_unpaired_objection(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || (!cls.uvm.is_component() && cls.uvm != UvmKind::Sequence) {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            let raise = sub.calls.iter().find(|c| c.name == "raise_objection");
            let drop = sub.calls.iter().find(|c| c.name == "drop_objection");
            match (raise, drop) {
                (Some(r), None) => {
                    out.add(
                        "UVM-OBJ-001",
                        "UvmUnpairedObjection",
                        Severity::Error,
                        format!(
                            "Method '{}' in class '{}' calls 'raise_objection' but has no corresponding 'drop_objection'. \
This un-dropped objection will cause the simulation to hang indefinitely.",
                            sub.name, cls.name
                        ),
                        r.loc,
                    );
                }
                (None, Some(k)) => {
                    out.add(
                        "UVM-OBJ-001",
                        "UvmUnpairedObjection",
                        Severity::Error,
                        format!(
                            "Method '{}' in class '{}' calls 'drop_objection' without any prior 'raise_objection' in the same scope. \
Dropping unraised objections leads to UVM runtime fatal errors.",
                            sub.name, cls.name
                        ),
                        k.loc,
                    );
                }
                _ => {}
            }
        }
    }
}

fn virtual_interface_modport_drive_inversion(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        if !cls.name.contains("monitor") && !derives_from(d, Some(cls.id), "uvm_monitor") {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern {
                continue;
            }
            for a in &sub.assignments {
                let lhs = &a.lhs;
                if !lhs.contains("vif.") && !lhs.contains("intf.") && !lhs.contains("_vif.") {
                    continue;
                }
                out.add(
                    "UVM-VIF-DIR",
                    "VirtualInterfaceModportDriveInversion",
                    Severity::Error,
                    format!(
                        "Passive uvm_monitor '{}' illegally drives virtual interface signal '{lhs}'. Monitors must be strictly read-only observers. Driving signals from a monitor \
violates UVM passive component semantics and causes multi-driver contention.",
                        cls.name
                    ),
                    a.loc,
                );
            }
        }
    }
}

fn virtual_sequencer_wiring_checker(d: &Design, out: &mut Sink) {
    let mut fields = Vec::new();
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        let vseqr = cls.name.contains("vseqr")
            || cls.name.contains("virtual_sequencer")
            || derives_from(d, Some(cls.id), "uvm_sequencer");
        if !vseqr {
            continue;
        }
        for p in &cls.properties {
            let sub = p.ty.contains("sequencer")
                || p.name.contains("_seqr")
                || p.name.contains("sequencer");
            if sub && !derives_from(d, p.type_class, "uvm_phase") {
                fields.push((cls, p));
            }
        }
    }
    if fields.is_empty() {
        return;
    }
    let assignments = connect_phase_assignments(d);
    for (owner, p) in fields {
        if assignments.iter().any(|s| s.contains(p.name.as_str())) {
            continue;
        }
        out.add(
            "UVM-VSEQ-001",
            "VirtualSequencerWiringChecker",
            Severity::Warning,
            format!(
                "Sub-sequencer handle '{0}' in virtual sequencer '{1}' is declared but never wired in any connect_phase. \
Virtual sequences attempting to access p_sequencer.{0} will crash with a null-pointer dereference at runtime.",
                p.name, owner.name
            ),
            p.loc,
        );
    }
}
