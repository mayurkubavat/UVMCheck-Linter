use dv_lint::{lint, rules};

fn findings(src: &str) -> Vec<(String, u32, u32, &'static str)> {
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(&[tree]);
    lint(&d, |_| true)
        .iter()
        .map(|f| {
            let (file, line, col) = f.position(&d);
            (file.to_string(), line, col, f.rule)
        })
        .collect()
}

const MOCK: &str = "class uvm_object; function new(string name = \"\"); endfunction endclass\n\
    class uvm_phase extends uvm_object; function void raise_objection(uvm_object o); endfunction endclass\n\
    class uvm_component extends uvm_object; function new(string name, uvm_component parent); super.new(name); endfunction endclass\n\
    class uvm_monitor extends uvm_component; function new(string name, uvm_component parent); super.new(name, parent); endfunction endclass\n";

#[test]
fn rules_have_unique_ids() {
    let r = rules();
    assert_eq!(r.len(), 31);
    let mut ids: Vec<_> = r.iter().map(|r| r.id).collect();
    ids.dedup();
    assert_eq!(ids.len(), 31);
}

#[test]
fn oop_rules() {
    let src = "class a; int x; function void f(); endfunction endclass\n\
               class b extends a; int x; function void f(); endfunction function new(); endfunction endclass\n";
    let got = findings(src);
    assert!(
        got.contains(&("t.sv".into(), 2, 24, "SV-OOP-004")),
        "{got:?}"
    );
    assert!(
        got.contains(&("t.sv".into(), 2, 41, "SV-OOP-001")),
        "{got:?}"
    );
    assert!(
        got.contains(&("t.sv".into(), 2, 67, "SV-OOP-003")),
        "{got:?}"
    );
}

#[test]
fn component_rules() {
    let src = format!(
        "{MOCK}class my_mon extends uvm_monitor;\n  static int count;\n  int depth;\n\
         function new(string name, uvm_component parent); super.new(name, parent); endfunction\n\
         function void build_phase(uvm_phase phase); #1; endfunction\n\
         function void connect_phase(uvm_phase phase); phase.raise_objection(this); endfunction\n\
         endclass\n"
    );
    let got: Vec<_> = findings(&src).into_iter().map(|f| f.3).collect();
    for id in [
        "SV-OOP-007",
        "SV-STY-001",
        "UVM-PHS-002",
        "UVM-PHS-009",
        "UVM-OBJ-001",
        "SV-NAM-001",
    ] {
        assert!(got.contains(&id), "{id} missing in {got:?}");
    }
    // my_mon derives from uvm_monitor but lacks the _monitor suffix.
    assert!(got.contains(&"SV-NAM-001"), "{got:?}");
}

#[test]
fn macro_code_is_reported_at_the_expansion() {
    let src = "`define DECL static int n;\nclass uvm_component; endclass\n\
               class c_env extends uvm_component;\n  `DECL\nendclass\n";
    let got = findings(src);
    assert!(
        got.contains(&("t.sv".into(), 4, 0, "SV-OOP-007")),
        "{got:?}"
    );
}

#[test]
fn factory_and_phase_rules() {
    let src = "`define uvm_object_utils(T) typedef int type_id;\n\
               class uvm_object; virtual function void do_print(); endfunction endclass\n\
               class uvm_phase; endclass\n\
               class uvm_component extends uvm_object; virtual function void build_phase(uvm_phase phase); endfunction endclass\n\
               class a_env extends uvm_component;\n  `uvm_object_utils(a_env)\n\
               function void build_phase(uvm_phase phase); endfunction\nendclass\n\
               class b_obj extends uvm_object; function void f(); do_print(); endfunction endclass\n";
    let got: Vec<_> = findings(src).into_iter().map(|f| (f.1, f.3)).collect();
    assert!(got.contains(&(6, "UVM-FAC-002")), "{got:?}");
    assert!(got.contains(&(7, "UVM-PHS-001")), "{got:?}");
    assert!(got.contains(&(9, "UVM-FAC-003")), "{got:?}");
    assert!(got.contains(&(9, "UVM-BAN-003")), "{got:?}");
    assert!(!got.contains(&(5, "UVM-FAC-003")), "{got:?}");
}
