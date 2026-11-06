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
    assert_eq!(r.len(), 222);
    let mut ids: Vec<_> = r.iter().map(|r| r.id).collect();
    ids.dedup();
    assert_eq!(ids.len(), 222);
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
