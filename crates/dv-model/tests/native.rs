use dv_model::{PhaseKind, UvmKind};

fn design(src: &str) -> dv_model::Design {
    let tree = svlang::syntax::parse_source(src, "t.sv", Default::default());
    dv_model::build(&[tree])
}

#[test]
fn uvm_roles_and_phases_from_pure_sv_model() {
    let d = design(
        "class uvm_object; endclass\n\
         class uvm_component extends uvm_object; endclass\n\
         class uvm_driver #(type T = int) extends uvm_component; endclass\n\
         class unused_seq #(type T = int) extends uvm_object; endclass\n\
         class my_drv extends uvm_driver #(int);\n\
           virtual function void build_phase(); endfunction\n\
           virtual task run_phase(); endtask\n\
           function void helper(); endfunction\n\
         endclass\n",
    );
    let drv = d.find_class("my_drv").unwrap();
    assert_eq!(drv.base_name, "uvm_driver");
    assert_eq!(drv.uvm, UvmKind::Driver);
    assert_eq!(d.find_class("unused_seq").unwrap().uvm, UvmKind::None);

    let phase_of = |n: &str| drv.methods.iter().find(|m| m.name == n).unwrap().phase;
    assert_eq!(phase_of("build_phase"), PhaseKind::Function);
    assert_eq!(phase_of("run_phase"), PhaseKind::Task);
    assert_eq!(phase_of("helper"), PhaseKind::None);
}
