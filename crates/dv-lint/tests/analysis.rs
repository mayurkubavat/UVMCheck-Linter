//! The S4 analyzers (rules/analysis): constraints, cross bins, clocked
//! races and DPI prototypes.

const IDS: &[&str] = &["SV-RND-030", "SV-RND-031", "SV-RND-032", "SV-COV-018", "SV-RCE-002", "DPI-PRT-001"];

fn run(src: &str) -> Vec<(u32, &'static str, String)> {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("dv-lint-analysis-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let f = d.join("t.sv");
    std::fs::write(&f, src).unwrap();
    let (design, trees) = sv_sema::load_with_trees(&[f.display().to_string()]).unwrap();
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
