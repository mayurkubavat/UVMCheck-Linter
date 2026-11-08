//! With `-top`, a module instantiated only where elaboration does not go is
//! not walked by the AST rules; one nothing instantiates still is, as slang
//! elaborates it on its own (own test binary: the tops are global).

#[test]
fn modules_only_instantiated_in_untaken_branches_are_not_walked() {
    dv_lint::add_tops(&["top".to_string()]);
    let src = "module used(input logic a, output logic y);\n\
               always_comb if (a) begin y = 1'b0; y = a; end else begin y = 1'b0; y = a; end\n\
               endmodule\n\
               module unused(input logic a, output logic y);\n\
               always_comb if (a) begin y = 1'b0; y = a; end else begin y = 1'b0; y = a; end\n\
               endmodule\n\
               module skipped(input logic a, output logic y);\n\
               always_comb if (a) begin y = 1'b0; y = a; end else begin y = 1'b0; y = a; end\n\
               endmodule\n\
               module top; logic a, y; used u(.a(a), .y(y));\n\
               if (0) begin : g skipped s(.a(a), .y(y)); end\n\
               endmodule\n";
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    let mut lines: Vec<u32> = found.iter().filter(|f| f.rule == "SV-BUG-001").map(|f| f.loc.line).collect();
    lines.sort();
    assert_eq!(lines, vec![2, 5]);
}
