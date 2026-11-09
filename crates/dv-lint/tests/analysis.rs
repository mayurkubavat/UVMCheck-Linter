//! The S4 analyzers (rules/analysis): constraints, cross bins, clocked
//! races, DPI prototypes and analysis ports without coverage.

const IDS: &[&str] =
    &["SV-RND-030", "SV-RND-031", "SV-RND-032", "SV-COV-018", "SV-RCE-002", "DPI-PRT-001", "UVM-TLM-007", "SV-COV-019"];

fn run(src: &str) -> Vec<(u32, &'static str, String)> {
    run_with(src, &[])
}

/// `src` with the UVM stub (testdata/common/uvm_mock.svh) on the include path.
fn run_uvm(src: &str) -> Vec<(u32, &'static str, String)> {
    let common = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/common");
    run_with(&format!("`include \"uvm_mock.svh\"\n{src}"), &["-I".to_string(), common.display().to_string()])
}

fn run_with(src: &str, opts: &[String]) -> Vec<(u32, &'static str, String)> {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("dv-lint-analysis-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let f = d.join("t.sv");
    std::fs::write(&f, src).unwrap();
    let mut args = opts.to_vec();
    args.push(f.display().to_string());
    let (design, trees) = sv_sema::load_with_trees(&args).unwrap();
    let (files, found) = dv_lint::lint_all(&design, &trees, |r| IDS.contains(&r));
    std::fs::remove_dir_all(&d).ok();
    found.iter().map(|x| (x.position_in(&files).1, x.rule, x.message.clone())).collect()
}

fn lines(got: &[(u32, &'static str, String)], id: &str) -> Vec<u32> {
    got.iter().filter(|g| g.1 == id).map(|g| g.0).collect()
}

#[test]
fn constraint_analyzers() {
    let got = run(include_str!("analysis_probe.sv"));
    // pkt: len > 10 (c1) with len < 5 (c2); b > 200 on a byte. ok: short_c
    // against long_c, as nothing calls constraint_mode.
    assert_eq!(lines(&got, "SV-RND-030"), [6, 7, 12], "{got:?}");
    let soft = got.iter().find(|g| g.1 == "SV-RND-031").expect("soft");
    assert!(soft.0 == 5 && soft.2.contains("'x == 9'") && soft.2.contains("{[1:2], 7}"), "{soft:?}");
    let dead = got.iter().find(|g| g.1 == "SV-RND-032").expect("dist");
    assert!(dead.0 == 5 && dead.2.contains("'[20:30] :/ 1'"), "{dead:?}");
}

#[test]
fn constraint_mode_keeps_blocks_apart() {
    let src = "class ok;\n  rand bit [3:0] len;\n  constraint short_c { len < 5; }\n  constraint long_c { len > 10; }\n\
               constraint both { len > 3; len < 2; }\n  function void f(); short_c.constraint_mode(0); endfunction\nendclass\n";
    let got = run(src);
    // Across blocks: off, since a test may switch one off. Within one: still an error.
    assert_eq!(lines(&got, "SV-RND-030"), [5], "{got:?}");
}

#[test]
fn race_cross_and_dpi() {
    let got = run(include_str!("analysis_probe.sv"));
    // 'a' blocking on posedge, read on posedge; 'b' is written on negedge;
    // the block-local 't' is not shared.
    let race: Vec<_> = got.iter().filter(|g| g.1 == "SV-RCE-002").collect();
    assert_eq!(race.len(), 1, "{got:?}");
    assert!(race[0].0 == 17 && race[0].2.starts_with("'a' is written"), "{race:?}");
    // f: int(int) in m and n, void(longint) once; g is imported as cg_fn.
    assert_eq!(lines(&got, "DPI-PRT-001"), [27], "{got:?}");
    // 64 x 64 and 64 x 64: at most 10,000 bins.
    assert!(lines(&got, "SV-COV-018").is_empty(), "{got:?}");
    let big = run("module m(input logic clk);\n  logic [7:0] v; logic [15:0] w;\n  covergroup cg @(posedge clk);\n\
                   option.auto_bin_max = 256;\n    cp: coverpoint v;\n    cx: cross cp, w;\n  endgroup\nendmodule\n");
    let x: Vec<_> = big.iter().filter(|g| g.1 == "SV-COV-018").collect();
    assert!(x.len() == 1 && x[0].0 == 6 && x[0].2.contains("65536 bins"), "{big:?}");
}

#[test]
fn analysis_port_without_coverage() {
    // `item` reaches `cov` (a covergroup and an analysis imp); `pkt` reaches
    // only a scoreboard FIFO; the driver's copy of `item` is not a monitor's.
    let tb = "class item extends uvm_sequence_item; int a; endclass\n\
              class pkt extends uvm_sequence_item; int b; endclass\n\
              class mon extends uvm_monitor;\n\
                uvm_analysis_port #(item) item_ap;\n\
                uvm_analysis_port #(pkt) pkt_ap;\n\
                function new(string name, uvm_component parent); super.new(name, parent); endfunction\n\
              endclass\n\
              class cov extends uvm_component;\n\
                uvm_analysis_imp #(item, cov) item_imp;\n\
                int a;\n\
                covergroup cg; coverpoint a; endgroup\n\
                function new(string name, uvm_component parent); super.new(name, parent); cg = new; endfunction\n\
                function void write(item t); a = t.a; cg.sample(); endfunction\n\
              endclass\n\
              class sb extends uvm_component;\n\
                uvm_tlm_analysis_fifo #(pkt) pkt_fifo;\n\
                function new(string name, uvm_component parent); super.new(name, parent); endfunction\n\
              endclass\n";
    let got = run_uvm(tb);
    let f: Vec<_> = got.iter().filter(|g| g.1 == "UVM-TLM-007").collect();
    // Line 6 of the file: the `include is line 1.
    assert!(f.len() == 1 && f[0].0 == 6 && f[0].2.contains("'pkt'") && f[0].2.contains("mon.pkt_ap"), "{got:?}");
    // A subscriber with a covergroup covers its type.
    let sub = tb.replace(
        "class sb extends uvm_component;",
        "class pkt_cov extends uvm_subscriber #(pkt);\n  int b;\n  covergroup cg; coverpoint b; endgroup\n\
         function new(string name, uvm_component parent); super.new(name, parent); cg = new; endfunction\n\
         function void write(pkt t); b = t.b; cg.sample(); endfunction\nendclass\nclass sb extends uvm_component;",
    );
    assert!(lines(&run_uvm(&sub), "UVM-TLM-007").is_empty());
    // No covergroup anywhere: a testbench without coverage is a different finding.
    let none =
        tb.replace("covergroup cg; coverpoint a; endgroup", "").replace("cg = new;", "").replace("cg.sample();", "");
    assert!(lines(&run_uvm(&none), "UVM-TLM-007").is_empty());
}

#[test]
fn covergroup_never_sampled() {
    let src = "interface bus_if(input logic clk);\n\
               logic [3:0] v;\n\
               covergroup ev_cg @(posedge clk); coverpoint v; endgroup\n\
               covergroup man_cg; coverpoint v; endgroup\n\
               covergroup lost_cg; coverpoint v; endgroup\n\
               covergroup orphan_cg; coverpoint v; endgroup\n\
               covergroup arg_cg with function sample(logic [3:0] x); coverpoint x; endgroup\n\
               ev_cg e = new;\n\
               man_cg mc = new;\n\
               lost_cg lc = new;\n\
               arg_cg ac[2];\n\
               initial begin ac[0] = new; ac[1] = new; end\n\
               endinterface\n\
               module tb;\n\
               logic clk;\n\
               bus_if bus(clk);\n\
               always @(posedge clk) begin bus.mc.sample(); bus.ac[0].sample(bus.v); end\n\
               endmodule\n";
    let got = run(src);
    // lost_cg: instantiated, never sampled; orphan_cg: never instantiated. The
    // others have an event or are sampled from the module.
    assert_eq!(lines(&got, "SV-COV-019"), [5, 6], "{got:?}");
    let msgs: Vec<_> = got.iter().filter(|g| g.1 == "SV-COV-019").map(|g| g.2.as_str()).collect();
    assert!(msgs[0].contains("instance(s) lc are never sampled") && msgs[1].contains("never instantiated"), "{msgs:?}");
}
