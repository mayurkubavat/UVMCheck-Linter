//! Loads the linter's own rule examples through the C ABI.
use dv_model::{Design, Direction, PhaseKind, RootKind, TypeShape, UvmKind};
use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(example: &str) -> Design {
    let r = repo();
    Design::load([format!("+incdir+{}", r.join("testdata/common").display()), r.join(example).display().to_string()])
        .expect("design loads")
}

#[test]
fn reads_a_component_with_a_static_member() {
    let d = load("testdata/rules/oop/SV-OOP-007/bad.sv");
    assert_eq!(d.compile_errors(), 0);
    let c = d.find_class("MultiChannelMonitor").expect("class in model");
    assert_eq!(c.uvm, UvmKind::Component);
    assert!(c.uvm.is_component());
    assert!(d.derives_from(c.id, "uvm_object"));
    assert!(d.file_name(c.loc).ends_with("bad.sv"));
    let p = c.properties.iter().find(|p| p.name == "m_global_packet_counter").unwrap();
    assert!(p.is_static && !p.is_const);
    assert_eq!(p.loc.line, 10);
}

#[test]
fn reads_sequence_methods() {
    let d = load("testdata/rules/oop/UVM-SEQ-002/bad.sv");
    let seqs: Vec<_> =
        d.classes().iter().filter(|c| c.uvm == UvmKind::Sequence && !c.name.starts_with("uvm_")).collect();
    assert!(!seqs.is_empty());
    assert!(seqs.iter().any(|c| c.methods.iter().any(|m| m.name == "pre_body" || m.name == "post_body")));
    assert!(d.classes().iter().flat_map(|c| &c.methods).any(|m| m.phase == PhaseKind::Function));
}

#[test]
fn reports_unreadable_sources() {
    let err = Design::load(["/no/such/file.sv"]).unwrap_err();
    assert!(!err.0.is_empty());
}

#[test]
fn parameterized_bases_are_in_the_model() {
    let d = load("testdata/map/map_tb.sv");
    let drv = d.find_class("my_drv").expect("driver");
    assert_eq!(drv.uvm, UvmKind::Driver);
    let base = d.class(drv.base.expect("base resolved to the parameterized definition"));
    assert_eq!(base.name, "uvm_driver");
    assert!(base.is_parameterized);
    assert!(d.derives_from(drv.id, "uvm_component"));
    // Specializations are not separate entries.
    assert_eq!(d.classes().iter().filter(|c| c.name == "uvm_driver").count(), 1);
}

#[test]
fn reads_arguments_locals_constraints_and_extern_methods() {
    let d = load("testdata/model/facts.sv");
    assert_eq!(d.compile_errors(), 0);
    let c = d.find_class("facts_item").expect("class in model");
    assert_eq!(c.uvm, UvmKind::SequenceItem);
    assert_eq!(c.constraints.len(), 1);
    assert_eq!(c.constraints[0].name, "addr_c");

    let sum = c.methods.iter().find(|m| m.name == "sum").expect("extern method in model");
    assert!(sum.is_extern && sum.declared_virtual && !sum.returns_void);
    assert_eq!(sum.return_type, "int");
    assert_eq!(sum.loc.line, 22); // the body, after the class
    let q = &sum.arguments[0];
    assert_eq!((q.name.as_str(), q.direction, q.shape), ("q", Direction::Ref, TypeShape::Queue));
    assert_eq!(sum.arguments[1].direction, Direction::In);
    let locals: Vec<_> = sum.locals.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(locals, ["total"]);

    let probe = c.methods.iter().find(|m| m.name == "probe").unwrap();
    assert!(probe.returns_void && probe.locals[0].is_virtual_interface);
    let calls: Vec<_> = probe.calls.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(calls, ["randomize", "set", "$urandom"]);
    let set = &probe.calls[1];
    assert!(set.first_arg_is_null && set.callee_scope == "uvm_config_db" && !set.is_system);

    let sub = d.find_class("facts_sub").expect("derived class in model");
    let ctor = sub.methods.iter().find(|m| m.name == "new").unwrap();
    assert!(ctor.calls_super_new);
    assert_eq!(ctor.super_new.map(|(i, l)| (i, l.line)), Some((0, 37)));
    let write = sub.methods.iter().find(|m| m.name == "write").unwrap();
    let a = &write.assignments[0];
    assert_eq!((a.lhs.as_str(), a.rhs.as_str(), a.root_kind), ("t.addr", "0", RootKind::Argument));
    assert!(a.is_non_blocking);
    assert_eq!(write.assignments[1].root_kind, RootKind::Property);
    let wait = sub.methods.iter().find(|m| m.name == "wait_a_bit").unwrap();
    assert_eq!(wait.timing_controls.iter().map(|l| l.line).collect::<Vec<_>>(), [45]);
    assert!(c.methods.iter().any(|m| m.name == "randomize" && m.is_builtin));
}
