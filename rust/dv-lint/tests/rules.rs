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
    assert_eq!(r.len(), 68);
    let mut ids: Vec<_> = r.iter().map(|r| r.id).collect();
    ids.dedup();
    assert_eq!(ids.len(), 68);
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

#[test]
fn syntax_rules() {
    let src = "module m; defparam m.P = 2;\n\
               always @* casex (a) 1: ; endcase\n\
               initial begin fork join -> ev; x = #1 y; if (y == y) ; force y = 1; release y; end\n\
               endmodule\n\
               program p; endprogram\n\
               class c; mailbox mb = new; semaphore sem; task t(); sem.get(0); endtask endclass\n";
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (files, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    let got: Vec<_> = found
        .iter()
        .map(|f| {
            let (_, line, col) = f.position_in(&files);
            (line, col, f.rule)
        })
        .collect();
    for want in [
        (1, 11, "SV-DEP-002"),
        (2, 1, "SV-DEP-003"),
        (2, 11, "SV-COR-009"),
        (2, 11, "SV-COR-006"),
        (3, 15, "SV-DED-010"),
        (3, 25, "SV-RCE-001"),
        (3, 34, "SV-DLY-002"),
        (3, 48, "SV-COR-008"),
        (3, 56, "SV-DEP-005"),
        (5, 1, "SV-DEP-001"),
        (6, 23, "SV-CON-007"),
        (6, 61, "SV-CON-005"),
    ] {
        assert!(got.contains(&want), "{want:?} not in {got:?}");
    }
    assert_eq!(got.iter().filter(|g| g.2 == "SV-DEP-005").count(), 1, "{got:?}");
}

#[test]
fn syntax_rules_batch_two() {
    let src = "package foo; endpackage\n\
               interface bus; endinterface\n\
               module m(a); input a; wire w; reg r; endmodule\n\
               class c; covergroup cg @(posedge k); cp: coverpoint x { bins d = default; } cross cp; endgroup\n\
               static constraint sk { x > 0; }\n\
               task int t(); assert (x); disable fork; wait (0); void'(randomize() with {}); phase.jump(p); endtask endclass\n";
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    let got: Vec<_> = found.iter().map(|f| f.rule).collect();
    for want in [
        "SV-NAM-002", "SV-NAM-003", "SV-DEP-006", "SV-BAN-001", "SV-COV-015", "SV-COV-009", "SV-COV-008",
        "SV-RND-009", "SV-TYP-010", "SVA-LBL-001", "SV-CON-010", "SV-CON-004", "SV-RND-014", "UVM-PHS-007",
    ] {
        assert!(got.contains(&want), "{want} not in {got:?}");
    }
    assert_eq!(got.iter().filter(|r| **r == "SV-BAN-001").count(), 2, "{got:?}");
}

#[test]
fn syntax_rules_batch_three() {
    let src = "class c; rand int x; constraint k { x inside {[10:2], [1:5]}; }\n\
               task t(); int a; event e; c o, q[$];\n\
               if ((a = 3) > 1) a += 1;\n\
               repeat (2) for (a = 0; a < 2; a++) begin o = new(); q.push_back(new); end\n\
               @(posedge e) -> e; @(e) -> e; #($urandom_range(1, 5));\n\
               s.start(sqr, null, -2); s.start(sqr, null, -1);\n\
               endtask endclass\n\
               module m; logic clk, a, b;\n\
               assert property (@(posedge clk) a ##[1:$] b[=0] ##1 b[->1]);\n\
               endmodule\n";
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    let count = |id: &str| found.iter().filter(|f| f.rule == id).count();
    assert_eq!(count("SV-RND-021"), 1);
    assert_eq!(count("SV-COR-020"), 1);
    // Once for each enclosing loop; a `new` argument is not slang's NewClassExpression.
    assert_eq!(count("SV-OOP-010"), 2);
    assert_eq!(count("SV-CON-013"), 1);
    assert_eq!(count("SVA-CYC-001"), 2);
    assert_eq!(count("SV-SVA-004"), 1);
    assert_eq!(count("UVM-SEQ-007"), 1);
}
