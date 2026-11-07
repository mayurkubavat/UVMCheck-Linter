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
    assert_eq!(r.len(), 367);
    let mut ids: Vec<_> = r.iter().map(|r| r.id).collect();
    ids.dedup();
    assert_eq!(ids.len(), 367);
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

#[test]
fn syntax_rules_batch_three_b() {
    let src = "`define M(a) a /* TODO in body */ + 1\n\
               class c; rand int x; int q[$];\n\
               constraint k { x dist {1 := 0, 2 :/ 0}; x dist {1 := 0, 2}; }\n\
               // FIXME before code\n\
               task t(); x = q[q.size()]; x = q[q.size() - 1]; x = `M(2); endtask\n\
               // TODO dropped before a macro use\n\
               `M(3);\n\
               endclass\n\
               `ifdef NOPE\n// TODO in a branch not taken\n`endif\n\
               // XXX at the end\n";
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    let count = |id: &str| found.iter().filter(|f| f.rule == id).count();
    assert_eq!(count("SV-ARR-001"), 1);
    assert_eq!(count("SV-RND-012"), 1);
    let todo: Vec<_> = found.iter().filter(|f| f.rule == "SV-STY-003").map(|f| f.message.as_str()).collect();
    // The body comment once per expansion.
    assert_eq!(todo.len(), 4, "{todo:?}");
    assert!(todo.iter().all(|m| !m.contains("dropped") && !m.contains("not taken")), "{todo:?}");
}

#[test]
fn syntax_rules_batch_four() {
    let src = "module m; int q[$]; string s; int a, b, i;\n\
               initial begin\n\
               if ((a == 1) & b) a = 1;\n\
               if ((a = 1)) ;\n\
               for (i = 0; i < 3; i++) begin\n\
               // a comment before the change\n\
               i += 1; s = {s, \"x\"}; s = {2{s}}; end\n\
               foreach (q[n]) fork q[n] = 1; join_none\n\
               end endmodule\n";
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    let count = |id: &str| found.iter().filter(|f| f.rule == id).count();
    assert_eq!(count("SV-COR-005"), 1);
    assert_eq!(count("SV-COR-007"), 1);
    assert_eq!(count("SV-COR-011"), 1);
    // A replication is not slang's ConcatenationExpression.
    assert_eq!(count("SV-PRF-010"), 1);
    assert_eq!(count("SV-CON-012"), 1);
}

#[test]
fn syntax_rules_batch_five() {
    let src = "class b; endclass\n\
               class d extends b;\n\
               function void f();\n\
               $fsdbDumpvars;\n\
               uvm_config_db#(int)::set(null, \"*\", \"x\", 1);\n\
               uvm_config_db#(int)::get(this, \"\", \"x\", x);\n\
               // a comment before a void cast\n\
               void'(uvm_config_db#(int)::get(this, \"\", \"x\", x));\n\
               port.try_get(p);\n\
               p = c::type_id::create(\"c\", null);\n\
               c1.constraint_mode(0);\n\
               if ($cast(a, b)) ; else if ($cast(a, y)) ; else if ($cast(a, z)) ;\n\
               a = {<< {b}}; a = {>> byte {b}};\n\
               endfunction endclass\n";
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    let count = |id: &str| found.iter().filter(|f| f.rule == id).count();
    for id in ["PORT-PRP-001", "UVM-CFG-003", "UVM-CFG-004", "UVM-TLM-002", "UVM-STR-005", "SV-LSP-001", "SV-OCP-001", "SV-STR-001"] {
        assert_eq!(count(id), 1, "{id}");
    }
}

#[test]
fn syntax_rules_batch_six() {
    let src = "module m(input clk, rst, a, b, c);\n\
               property p2(x); @(posedge clk) 0 |=> b; endproperty\n\
               assert property (@(posedge clk) disable iff (rst) a |-> b);\n\
               covergroup cg @(posedge clk); cpa: coverpoint a; cpb: coverpoint b { bins b0 = {0}; }\n\
               cpc: coverpoint c { bins b1 = {1}; }\n\
               x: cross cpa, cpb, cpc; y: cross cpb, cpc { ignore_bins i = binsof(cpb); } endgroup\n\
               endmodule\n\
               class k; rand int q[];\n\
               constraint k1 { q.size() < 10; }\n\
               function void build_phase(uvm_phase phase); s.start(sqr); endfunction\n\
               task t(); s.start(null); endtask\n\
               endclass\n";
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    let count = |id: &str| found.iter().filter(|f| f.rule == id).count();
    for id in ["SV-COV-006", "SV-COV-010", "SV-COV-012", "UVM-PHS-004", "SVA-VAC-001", "SVA-RST-001", "SV-PRF-012", "UVM-SEQ-012"] {
        assert!(count(id) >= 1, "{id}");
    }
}

/// The `(rule, line)` pairs an `expect.json` lists for `bad.sv`.
fn expected(json: &str) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    let mut rule = None;
    for part in json.split(',').flat_map(|p| p.split('{')) {
        let Some((k, v)) = part.split_once(':') else { continue };
        let v = v.trim().trim_matches(|c: char| c == '}' || c == ']' || c.is_whitespace());
        match k.trim().trim_matches('"') {
            "rule" => rule = Some(v.trim_matches('"').to_string()),
            "line" => {
                if let (Some(r), Ok(l)) = (rule.take(), v.parse()) {
                    out.push((r, l));
                }
            }
            _ => {}
        }
    }
    out
}

/// Each uvm/ral rule finds what its repo fixture expects.
#[test]
fn uvm_and_ral_rules_on_fixtures() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ids = [
        "RAL-ACC-001", "RAL-ACC-002", "RAL-BLK-001", "RAL-COL-001", "RAL-MAP-001", "RAL-MAP-002",
        "UVM-ALS-001", "UVM-DAG-001", "UVM-FAC-004", "UVM-FAC-005", "UVM-LOG-002", "UVM-OBJ-002",
        "UVM-OBJ-003", "UVM-OBJ-004", "UVM-OBJ-005", "UVM-OBS-002", "UVM-PHS-005", "UVM-PHS-006",
        "UVM-PHS-008", "UVM-PHS-010", "UVM-RAL-001", "UVM-REP-005", "UVM-SCB-001", "UVM-SCB-002",
        "UVM-SEQ-005", "UVM-SEQ-006", "UVM-SEQ-008", "UVM-SEQ-009", "UVM-SEQ-010", "UVM-SEQ-011",
        "UVM-SEQ-014", "UVM-SEQ-015", "UVM-STY-003", "UVM-TLM-004", "UVM-TLM-005", "UVM-TLM-006",
    ];
    for id in ids {
        let area = if id == "UVM-RAL-001" { "oop" } else { "uvm" };
        let dir = root.join("tests/rules").join(area).join(id);
        let json = std::fs::read_to_string(dir.join("expect.json")).unwrap();
        let args = [
            format!("+incdir+{}", root.join("tests/common").display()),
            dir.join("bad.sv").display().to_string(),
        ];
        let (d, trees) = sv_sema::load_with_trees(&args).unwrap();
        let (files, found) = dv_lint::lint_all(&d, &trees, |r| r == id);
        let lines: Vec<u32> = found.iter().map(|f| f.position_in(&files).1).collect();
        let want: Vec<u32> = expected(&json).into_iter().filter(|(r, _)| r == id).map(|(_, l)| l).collect();
        assert!(!want.is_empty(), "{id}: no expectation");
        for l in want {
            assert!(lines.contains(&l), "{id}: line {l} not in {lines:?}");
        }
    }
}

#[test]
fn early_return_before_finish_item() {
    let src = "class item; endclass\n\
               class my_seq;\n  item req;\n\
               task body(); start_item(req); if (req == null) return; finish_item(req); endtask\n\
               task other(); if (req == null) return; endtask\n\
               endclass\n";
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (files, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |r| r == "UVM-SEQ-004");
    let got: Vec<_> = found.iter().map(|f| f.position_in(&files)).map(|(_, l, c)| (l, c)).collect();
    assert_eq!(got, vec![(4, 6)]);
}

/// An element-select receiver reports as 'reg' and always fires, as in slang.
#[test]
fn mirror_set_on_element_select() {
    let src = "class fld; function void set(int v); endfunction endclass\n\
               class blk; fld foo[4]; fld bar;\n\
               function void f(); foo[2].set(1); bar.set(3); (bar).set(1); endfunction\nendclass\n";
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |r| r == "RAL-ACC-001");
    assert_eq!(found.len(), 1);
    assert!(found[0].message.starts_with("Direct mirror update 'reg.set(...)'"));
}

#[test]
fn group_g_rules() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/g_misc");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "sv" || x == "svh"))
        .collect();
    files.sort();
    let mut found = Vec::new();
    for p in files {
        let tree = sv_syntax::parse_file(&p, Default::default());
        let d = sv_sema::build(std::slice::from_ref(&tree));
        found.extend(dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true).1.into_iter().map(|f| f.rule));
    }
    let count = |id: &str| found.iter().filter(|r| **r == id).count();
    for id in [
        "DPI-TYP-001", "SV-LOC-001", "SV-PRF-011", "SV-PRF-013", "SV-SYS-001", "SV-DED-005", "SV-DED-006",
        "SV-DED-011", "SV-MOD-001", "SV-MAC-001", "SV-ERR-003", "UVM-TST-002", "PORT-PTH-001", "SEC-LOG-001",
        "SV-PRF-005", "SV-PRF-004", "SV-PRF-003", "SV-PRF-009", "SV-PRF-008",
    ] {
        assert!(count(id) >= 1, "{id}");
    }
    // One NBA per specialization-free class site plus the module one; the
    // interface member and the local are not class properties.
    assert_eq!(count("SV-PRF-009"), 7);
}

/// Group C (concurrency, SVA, coverage): each rule fires on its repo fixture
/// as often as the C++ rule does.
#[test]
fn concurrency_sva_coverage_rules() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests");
    let cases = [
        ("SV-CNC-002", "concurrency/SV-CNC-002", 1),
        ("SV-CNC-003", "concurrency/SV-CNC-003", 2),
        ("SV-CNC-004", "concurrency/SV-CNC-004", 2),
        ("SV-CON-006", "concurrency/SV-CON-006", 1),
        ("SV-CON-008", "concurrency/SV-CON-008", 1),
        ("SV-CON-009", "concurrency/SV-CON-009", 1),
        ("SV-CON-011", "concurrency/SV-CON-011", 1),
        ("SV-CON-014", "concurrency/SV-CON-014", 3),
        ("SV-CON-015", "concurrency/SV-CON-015", 3),
        ("SV-COV-001", "coverage/SV-COV-001", 1),
        ("SV-COV-002", "coverage/SV-COV-002", 1),
        ("SV-COV-004", "coverage/SV-COV-004", 1),
        ("SV-COV-005", "coverage/SV-COV-005", 1),
        ("SV-COV-011", "coverage/SV-COV-011", 1),
        ("SV-COV-013", "coverage/SV-COV-013", 1),
        ("SV-COV-016", "coverage/SV-COV-016", 1),
        ("SV-COV-017", "coverage/SV-COV-017", 1),
        ("SV-DLY-003", "concurrency/SV-DLY-003", 1),
        ("SV-DLY-004", "concurrency/SV-DLY-004", 1),
        ("SV-DRV-001", "rtl_tb_bridge/SV-DRV-001", 1),
        ("SV-EVT-001", "concurrency/SV-EVT-001", 1),
        ("SV-FOR-001", "concurrency/SV-FOR-001", 1),
        ("SV-INT-001", "rtl_tb_bridge/SV-INT-001", 1),
        ("SV-IPC-001", "concurrency/SV-IPC-001", 1),
        ("SV-RSC-001", "concurrency/SV-RSC-001", 3),
        ("SV-RSC-002", "concurrency/SV-RSC-002", 1),
        ("SV-SVA-001", "sva/SV-SVA-001", 1),
        ("SV-SVA-002", "sva/SV-SVA-002", 1),
        ("SV-SVA-003", "sva/SV-SVA-003", 1),
        ("SV-SVA-005", "sva/SV-SVA-005", 1),
        ("SV-THR-001", "concurrency/SV-THR-001", 1),
        ("SV-THR-002", "concurrency/SV-THR-002", 1),
        ("SV-THR-003", "concurrency/SV-THR-003", 1),
        ("SVA-ACT-001", "sva/SVA-ACT-001", 1),
        ("SVA-ACT-002", "sva/SVA-ACT-002", 1),
        ("SVA-ACT-003", "sva/SVA-ACT-003", 4),
        ("SVA-CTX-001", "sva/SVA-CTX-001", 1),
    ];
    for (id, dir, want) in cases {
        let opts = sv_syntax::Options { include_dirs: vec![root.join("common")], ..Default::default() };
        let tree = sv_syntax::parse_file(&root.join("rules").join(dir).join("bad.sv"), opts);
        let d = sv_sema::build(std::slice::from_ref(&tree));
        let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
        let got = found.iter().filter(|f| f.rule == id).count();
        assert_eq!(got, want, "{id}");
    }
}

#[test]
fn group_e_random_and_correctness_rules() {
    let src = "class beat; rand bit [7:0] data; endclass\n\
               class item;\n\
               localparam int MAX = 200;\n\
               rand bit [3:0] mode; rand int len; rand int unsigned d;\n\
               randc bit [31:0] wide; randc bit flag;\n\
               rand beat beats[];\n\
               real util; int unsigned bytes, cycles;\n\
               constraint c1 { mode -> len > 10; }\n\
               constraint c2 { d dist { 0 := 50, [1:MAX] := 50 }; }\n\
               constraint c3 { solve mode before d; }\n\
               function new(int len = 1);\n\
               len = len;\n\
               srandom(7);\n\
               endfunction\n\
               function void f(int a, int b);\n\
               util = bytes / cycles;\n\
               if (util == 1.5) a = 1;\n\
               if (a == 1) b = 1; else if (a == 1) b = 2;\n\
               g(b, a);\n\
               endfunction\n\
               function void g(int a, int b); endfunction\n\
               endclass\n";
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    let count = |id: &str| found.iter().filter(|f| f.rule == id).count();
    for id in [
        "SV-RND-019", "SV-RND-025", "SV-RND-026", "SV-RND-006", "SV-RND-017", "SV-RND-023", "SV-BUG-003", "SV-TST-001",
        "SV-BUG-005", "SV-BUG-006", "SV-BUG-002", "SV-BUG-007",
    ] {
        assert_eq!(count(id), 1, "{id}: {found:?}");
    }
    let hint = found.iter().find(|f| f.rule == "SV-BUG-003").unwrap();
    assert!(hint.message.contains("write 'this.len = len;'"), "{}", hint.message);
}

#[test]
fn methodology_rules_group_a() {
    let src = "`define DEPTH 16\n\
               `define TWO_STEPS(a) a = 0; a = 1;\n\
               package p;\n\
               typedef enum {A, B} e_t;\n\
               class c;\n\
               integer n;\n\
               string mode;\n\
               bit [31:0] h;\n\
               rand int x;\n\
               constraint c1 { soft x == 1; }\n\
               constraint c2 { soft x == 2; }\n\
               function void f(string op);\n\
               case (mode) \"a\": ; \"b\": ; \"c\": ; endcase\n\
               if (op == \"r\") ; else if (op == \"w\") ;\n\
               h[7:0] = 0; h[15:8] = 0; h[31:16] = 0;\n\
               endfunction\n\
               endclass\n\
               endpackage\n";
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    let at = |id: &str| found.iter().filter(|f| f.rule == id).map(|f| f.message.as_str()).collect::<Vec<_>>();
    assert_eq!(at("SV-CNS-001"), ["`define DEPTH holds the constant '16'. As a macro it has no type, no scope and can be redefined by any later file. Declare it as a parameter or localparam in a package instead."]);
    assert_eq!(at("SV-CNS-002").len(), 1);
    assert_eq!(at("SV-CNS-006").len(), 1);
    assert_eq!(at("SV-CNS-007").len(), 1);
    assert_eq!(at("SV-CNS-004").len(), 1);
    assert_eq!(at("SV-CPL-001").len(), 1);
    assert_eq!(at("SV-CNS-005").len(), 1);
    assert_eq!(at("SV-RND-027").len(), 1);
}

#[test]
fn identical_branches_and_mismatched_fields() {
    let src = "class item;\n\
               bit [31:0] addr, data;\n\
               function void do_copy(item r);\n\
               addr = r.addr;\n\
               data = r.addr;\n\
               endfunction\n\
               function int pick(bit s, int a, int b);\n\
               if (s) begin a = a + b; return a; end\n\
               else begin a = a + b; return a; end\n\
               endfunction\n\
               endclass\n";
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    let of = |id: &str| found.iter().filter(|f| f.rule == id).map(|f| (f.loc.line, f.message.clone())).collect::<Vec<_>>();
    let bug1 = of("SV-BUG-001");
    assert_eq!(bug1.len(), 1, "{found:?}");
    assert_eq!(bug1[0].0, 9);
    assert!(bug1[0].1.contains("branch at line 8"), "{}", bug1[0].1);
    let bug4 = of("SV-BUG-004");
    assert_eq!(bug4.len(), 1, "{found:?}");
    assert!(bug4[0].1.contains("field 'data' is paired with 'r.addr'"), "{}", bug4[0].1);
}

#[test]
fn virtual_class_new_only_in_named_specializations() {
    let src = "virtual class base; endclass\n\
               class leaf extends base; endclass\n\
               class maker #(type T = base);\n\
               function T make(); T o; o = new(); return o; endfunction\n\
               endclass\n\
               module top;\n\
               base b;\n\
               maker #(leaf) m;\n\
               initial b = new();\n\
               endmodule\n";
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    let lines: Vec<u32> = found.iter().filter(|f| f.rule == "SV-OOP-013").map(|f| f.loc.line).collect();
    // The default specialization (T = base) is not reported, as in slang.
    assert_eq!(lines, vec![9], "{found:?}");
}

fn rule_lines(src: &str, rule: &str) -> Vec<u32> {
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    let mut lines: Vec<u32> = found.iter().filter(|f| f.rule == rule).map(|f| f.loc.line).collect();
    lines.sort();
    lines
}

#[test]
fn language_gotcha_rules() {
    let src = "module m;\n\
               logic [3:0] a; logic [7:0] w;\n\
               initial if (a == 4'b1x0x) $display(\"x\");\n\
               initial w = {a, 5};\n\
               function void f(); a <= 1; endfunction\n\
               endmodule\n";
    assert_eq!(rule_lines(src, "SV-LANG-001"), vec![3]);
    assert_eq!(rule_lines(src, "SV-LANG-003"), vec![4]);
    assert_eq!(rule_lines(src, "SV-LANG-005"), vec![5]);
}

#[test]
fn literal_width_follows_bound_expressions() {
    let src = "class p extends unknown_base; int d; endclass\n\
               module m #(parameter int W = 4);\n\
               int n; p r; logic [W-1:0] v;\n\
               initial n = 'hff + 7;\n\
               initial if (r.d == 9) n = 3;\n\
               function void f(); #10; endfunction\n\
               endmodule\n\
               module top; m #(.W(8)) u(); endmodule\n";
    // The overridden default (line 2), member access through a class with
    // an unknown base (line 5) and a delay in a function (line 6) do not
    // bind in slang, so it never sees those literals.
    assert_eq!(rule_lines(src, "SV-STY-010"), vec![4, 4, 8]);
}

#[test]
fn ref_args_and_string_keys() {
    let src = "class c;\n\
               int a[string];\n\
               function int f(ref int q[$], int v); return q[0] + v; endfunction\n\
               function void g(ref int q[$]); q.push_back(1); endfunction\n\
               endclass\n";
    assert_eq!(rule_lines(src, "SV-API-001"), vec![3]);
    assert_eq!(rule_lines(src, "SV-PRF-006"), vec![2]);
}

#[test]
fn casts_reports_and_pattern_sizes() {
    let src = "class b; endclass\n\
               class c extends b;\n\
               int f[4];\n\
               function void g(b x); c y;\n\
               $cast(y, x);\n\
               void'($cast(y, x));\n\
               if (!$cast(y, x)) return;\n\
               endfunction\n\
               function void h(); f = '{1, 2, 3}; endfunction\n\
               endclass\n";
    assert_eq!(rule_lines(src, "SV-OOP-002"), vec![5, 6]);
    assert_eq!(rule_lines(src, "SV-TYP-009"), vec![9]);
}

#[test]
fn out_of_bounds_selects_and_magic_numbers() {
    let src = "class c;\n\
               logic [7:0] v; int a[4];\n\
               function void f();\n\
               v[8] = 0; a[4] = 1; v[7] = 0;\n\
               if (a[0] == 42) v[2] = 0;\n\
               endfunction\n\
               endclass\n";
    assert_eq!(rule_lines(src, "SV-LANG-002"), vec![4, 4]);
    // 2 on line 5 is the first use of 2 after 8 and 4 on line 4.
    assert_eq!(rule_lines(src, "SV-RDB-002"), vec![4, 4, 4, 5, 5]);
}

#[test]
fn enum_case_with_default() {
    let src = "typedef enum {IDLE, RUN, DONE} st_t;\n\
               module m; st_t s;\n\
               initial case (s) IDLE: ; default: ; endcase\n\
               initial case (s) IDLE, RUN, DONE: ; default: ; endcase\n\
               endmodule\n";
    assert_eq!(rule_lines(src, "SV-FSM-001"), vec![3]);
}

#[test]
fn part_select_reversed() {
    let src = "module m;\n\
               logic [7:0] v;\n\
               logic [0:7] u;\n\
               initial begin v[0:3] = 0; u[3:0] = 0; v[3:0] = 0; u[0:3] = 0; end\n\
               endmodule\n";
    assert_eq!(rule_lines(src, "SV-LANG-004"), vec![4, 4]);
}

#[test]
fn literal_per_generate_copy() {
    // One copy per instance and generate block: an untaken branch is
    // still built once (with its loops unrolled).
    let src = "module leaf #(parameter int W = 2) (output logic [7:0] y);\n\
                 if (W == 1) begin : g_one\n\
                   assign y = 5;\n\
                 end else begin : g_many\n\
                   for (genvar i = 0; i < W; i++) begin : g_bit\n\
                     assign y[i] = 7 > i;\n\
                   end\n\
                 end\n\
               endmodule\n\
               module top;\n\
                 logic [7:0] a, b;\n\
                 leaf #(.W(3)) u_a (.y(a));\n\
                 leaf #(.W(1)) u_b (.y(b));\n\
               endmodule\n";
    let mut lines = rule_lines(src, "SV-STY-010");
    lines.sort();
    assert_eq!(lines, vec![3, 3, 6, 6, 6, 6, 12]);
}

#[test]
fn error_reporting_rules() {
    let src = "class uvm_report_catcher; endclass\n\
               class uvm_scoreboard; endclass\n\
               class demote extends uvm_report_catcher;\n\
               function int catch(); set_severity(UVM_INFO); return 0; endfunction\n\
               endclass\n\
               class quiet_sb extends uvm_scoreboard;\n\
               function void check(); endfunction\n\
               endclass\n\
               class loud_sb extends uvm_scoreboard;\n\
               function void check(); uvm_report_error(\"X\", \"bad\"); endfunction\n\
               endclass\n";
    assert_eq!(rule_lines(src, "UVM-REP-006"), vec![4]);
    assert_eq!(rule_lines(src, "UVM-TST-001"), vec![6]);
}

#[test]
fn failure_not_reported() {
    let src = "class c;\n\
               rand int a;\n\
               function void uvm_report_error(string i, string m); endfunction\n\
               function void f(c o);\n\
               if (!randomize()) $display(\"failed\");\n\
               if (!randomize()) uvm_report_error(\"X\", \"failed\");\n\
               if (!$cast(o, this)) begin end\n\
               endfunction\n\
               endclass\n";
    assert_eq!(rule_lines(src, "SV-ERR-001"), vec![5, 7]);
}

#[test]
fn use_after_failed_null_check() {
    let src = "class item; int d; endclass\n\
               class c;\n\
               item a, b; int x;\n\
               function void f(item o);\n\
               if (a == null) $display(\"no a\");\n\
               x = a.d;\n\
               if (b == null) b = new();\n\
               x = b.d;\n\
               if (!$cast(a, o)) $display(\"bad\");\n\
               if (a.randomize()) x = a.d;\n\
               endfunction\n\
               endclass\n";
    assert_eq!(rule_lines(src, "SV-ERR-002"), vec![6, 10]);
}

#[test]
fn assoc_mutating_read() {
    let src = "class c;\n\
               int s[int]; int q[$];\n\
               function void f(int k);\n\
               if (s[k] == 1) ;\n\
               if (q[0] == 1) ;\n\
               if (s.exists(k)) ;\n\
               endfunction\n\
               endclass\n";
    assert_eq!(rule_lines(src, "SV-ASC-001"), vec![4]);
}

#[test]
fn clocking_output_read() {
    let src = "interface bi(input logic clk); logic d, i;\n\
               clocking cb @(posedge clk); output d; input i; endclocking\n\
               endinterface\n\
               class c; virtual bi vif; int x;\n\
               task t(); x = vif.cb.i; x = vif.cb.d + 1; endtask\n\
               endclass\n";
    assert_eq!(rule_lines(src, "SV-CB-001"), vec![5, 5]);
}

#[test]
fn compiler_errors() {
    let src = "module m;\n\
               int a;\n\
               initial b = 1;\n\
               endmodule\n";
    assert_eq!(rule_lines(src, "SV-CMP-001"), vec![3]);
    assert!(rule_lines("module m; endmodule\n", "SV-CMP-001").is_empty());
}

#[test]
fn unused_variable() {
    let src = "class c;\n\
               int q[$];\n\
               function void f();\n\
               int a;\n\
               int b = a;\n\
               int k;\n\
               int u;\n\
               foreach (q[k]) u = k;\n\
               endfunction\n\
               endclass\n";
    // `a` is read only by an initializer, `k` is shadowed by the loop.
    assert_eq!(rule_lines(src, "SV-DED-001"), vec![4, 5, 6]);
}

#[test]
fn unreferenced_parameter() {
    let src = "class box #(type T = int, int N = 2, int M = 3);\n\
               T items[N];\n\
               function int f(); return M; endfunction\n\
               endclass\n\
               class user; box#(byte, 4) b; endclass\n";
    // A dimension is not a reference; `int N` after `type T` is a value parameter.
    assert_eq!(rule_lines(src, "SV-DED-012"), vec![1]);
}

#[test]
fn unused_routine_argument() {
    let src = "class item; rand int addr; endclass\n\
               class c;\n\
               function void a(int x, int y, int _z);\n\
               $display(x);\n\
               endfunction\n\
               task drive(int addr, int lim);\n\
               item it = new();\n\
               void'(it.randomize() with { addr == addr; addr < lim; });\n\
               endtask\n\
               task keep(int addr);\n\
               item it = new();\n\
               void'(it.randomize() with { addr == local::addr; });\n\
               endtask\n\
               virtual function void v(int unused_ok, int w); $display(1); endfunction\n\
               endclass\n";
    // Names in a randomize-with block resolve into the object first.
    assert_eq!(rule_lines(src, "SV-DED-013"), vec![3, 6]);
}

#[test]
fn unused_local_method() {
    let src = "class c;\n\
               local function void used(); $display(1); endfunction\n\
               local function void dead(); $display(2); endfunction\n\
               function void run(); used(); endfunction\n\
               endclass\n";
    assert_eq!(rule_lines(src, "SV-DED-014"), vec![3]);
}

#[test]
fn style_engine() {
    let long = "x".repeat(130);
    let src = format!(
        "class c;\n\
         \tint a; \n\
         function void f(); endfunction\n\
         task t(); endtask : t\n\
         endclass\n\
         interface i; endinterface\n\
         // {long}\n"
    );
    assert_eq!(rule_lines(&src, "SV-STY-101"), vec![2]);
    assert_eq!(rule_lines(&src, "SV-STY-102"), vec![2]);
    assert_eq!(rule_lines(&src, "SV-STY-103"), vec![7]);
    assert_eq!(rule_lines(&src, "SV-STY-104"), vec![3, 5, 6]);
}

#[test]
fn naming_engine() {
    let src = "package Foo; endpackage\n\
               package bar_pkg; endpackage\n\
               interface bus; endinterface\n\
               class c;\n\
               localparam int lowp = 1;\n\
               localparam int OK_P = 3;\n\
               constraint bad { }\n\
               constraint good_c { }\n\
               covergroup g;\n\
               a: coverpoint lowp;\n\
               cp_b: coverpoint lowp;\n\
               x: cross a, cp_b;\n\
               endgroup\n\
               typedef enum {A} st;\n\
               typedef int w_t;\n\
               endclass\n";
    assert_eq!(rule_lines(src, "SV-NAM-101"), vec![1]);
    assert_eq!(rule_lines(src, "SV-NAM-102"), vec![3]);
    assert_eq!(rule_lines(src, "SV-NAM-109"), vec![5]);
    assert_eq!(rule_lines(src, "SV-NAM-112"), vec![7]);
    assert_eq!(rule_lines(src, "SV-NAM-105"), vec![9]);
    assert_eq!(rule_lines(src, "SV-NAM-106"), vec![10]);
    assert_eq!(rule_lines(src, "SV-NAM-107"), vec![12]);
    assert_eq!(rule_lines(src, "SV-NAM-110"), vec![14]);
    assert!(rule_lines(src, "SV-NAM-111").is_empty());
}

#[test]
fn queue_concat_append() {
    let src = "typedef int iq_t[$];\n\
               class c #(type T = int);\n\
               int q[$]; iq_t r; int a[4]; int x;\n\
               function void f();\n\
               q = {q, x};\n\
               r = {x, r};\n\
               a = {a[1:3], x};\n\
               q = {q, x, x};\n\
               endfunction\n\
               endclass\n\
               class u; c#(byte) b; endclass\n";
    // Once per specialization: the default one and c#(byte).
    assert_eq!(rule_lines(src, "SV-PRF-007"), vec![5, 5, 6, 6]);
}
