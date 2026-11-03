//! Loads the linter's own rule examples through the C ABI.
use dv_model::{Design, PhaseKind, UvmKind};
use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(example: &str) -> Design {
    let r = repo();
    Design::load([
        format!("+incdir+{}", r.join("tests/common").display()),
        r.join(example).display().to_string(),
    ])
    .expect("design loads")
}

#[test]
fn reads_a_component_with_a_static_member() {
    let d = load("tests/rules/oop/SV-OOP-007/bad.sv");
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
    let d = load("tests/rules/oop/UVM-SEQ-002/bad.sv");
    let seqs: Vec<_> = d.classes().iter().filter(|c| c.uvm == UvmKind::Sequence && !c.name.starts_with("uvm_")).collect();
    assert!(!seqs.is_empty());
    assert!(seqs.iter().any(|c| c.methods.iter().any(|m| m.name == "pre_body" || m.name == "post_body")));
    assert!(d.classes().iter().flat_map(|c| &c.methods).any(|m| m.phase == PhaseKind::Function));
}

#[test]
fn reports_unreadable_sources() {
    let err = Design::load(["/no/such/file.sv"]).unwrap_err();
    assert!(!err.0.is_empty());
}
