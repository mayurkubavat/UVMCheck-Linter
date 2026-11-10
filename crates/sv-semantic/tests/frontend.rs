//! Compile errors slang reports that the native front end once accepted:
//! `inside` between a string and an integral value, `solve ... before` on
//! a non-random name or in a nested constraint block, a modport used as a
//! value, and hierarchical names that name nothing (with `bind`
//! directives in definitions no instance elaborates binding nothing).
//! The cases are in testdata/frontend; `// error: <message>` marks each
//! expected error on its line.

fn diags(src: &str, tops: &[&str]) -> Vec<(usize, String)> {
    let t = sv_syntax::parse_source(src, "t.sv", sv_syntax::Options::default());
    assert!(t.errors.is_empty(), "syntax errors: {:?}", t.errors);
    let trees = vec![t];
    let tops: Vec<String> = tops.iter().map(|s| s.to_string()).collect();
    let (d, _) = sv_semantic::semantic_diags(&trees, &tops);
    let t = &trees[0];
    let mut out: Vec<(usize, String)> = d
        .into_iter()
        .map(|d| {
            let (tok, _) = t.token_bounds(d.node).expect("a node with tokens");
            let at = t.tokens[tok as usize].origin.offset as usize;
            (src[..at].matches('\n').count() + 1, d.message)
        })
        .collect();
    out.sort();
    out
}

fn testdata(name: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/frontend").join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn bad_cases_report_what_slang_reports() {
    let src = testdata("bad_semantics.sv");
    let mut want: Vec<(usize, String)> = src
        .lines()
        .enumerate()
        .filter_map(|(i, l)| l.split_once("// error: ").map(|(_, m)| (i + 1, m.trim().to_string())))
        .collect();
    want.sort();
    assert!(want.len() >= 6);
    assert_eq!(diags(&src, &[]), want);
}

#[test]
fn good_cases_have_no_errors() {
    assert_eq!(diags(&testdata("good_semantics.sv"), &[]), vec![]);
}

#[test]
fn a_bind_in_a_module_no_top_reaches_binds_nothing() {
    let src = "interface mon_if(input logic clk); logic err; endinterface\n\
               module core(input logic clk); endmodule\n\
               module tb; logic clk; core u_core(.clk); assign u_core.u_mon.err = 1'b0; endmodule\n\
               module binds; bind core mon_if u_mon(.clk); endmodule\n";
    assert_eq!(diags(src, &[]), vec![], "no -top: `binds` is a top and its bind applies");
    assert_eq!(
        diags(src, &["tb"]),
        vec![(3, "could not resolve hierarchical path name 'u_mon'".to_string())],
        "-top tb: nothing elaborates `binds`"
    );
    let unit = src.replace("module binds; bind core mon_if u_mon(.clk); endmodule", "bind core mon_if u_mon(.clk);");
    assert_eq!(diags(&unit, &["tb"]), vec![], "a bind in the compilation unit always applies");
}
