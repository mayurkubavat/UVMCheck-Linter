//! Elaboration (builder phase N4). The hierarchy is checked against
//! Slang's (`uvmcheck_sym_dump --hier`) on sv-tests and the repo by
//! tools/sym_gate.py --hier; these cases pin the rules that decide it.

fn trees(src: &str) -> Vec<sv_syntax::Tree> {
    let t = sv_syntax::parse_source(src, "t.sv", sv_syntax::Options::default());
    assert!(t.errors.is_empty(), "syntax errors: {:?}", t.errors);
    vec![t]
}

fn hier(src: &str) -> Vec<String> {
    sv_sema::hierarchy(&trees(src))
}

#[test]
fn instances_generate_blocks_and_parameters() {
    let h = hier(
        "module leaf #(parameter W = 4, localparam D = W*2) ();\nendmodule\n\
         module top;\n parameter N = 3;\n leaf #(.W(8)) u1();\n leaf u2[1:0]();\n\
         for (genvar i = 0; i < N; i++) begin : g leaf #(i+1) x(); end\n\
         if (N > 2) begin localparam Q = 5; end else begin : nope end\n\
         case (N) 3: begin : c3 end default: ; endcase\n\
         if (N == 3) if (N > 1) begin : dn end\nendmodule\n",
    );
    for l in [
        "$root instance top : top",
        "top instance u1 : leaf",
        "top.u1 parameter W = 8",
        "top.u1 parameter D = 16",
        "top instance u2[0] : leaf",
        "top.u2[1] parameter W = 4",
        "top generate g[2]",
        "top.g[2] parameter i = 2",
        "top.g[2].x parameter W = 3",
        "top generate genblk2",
        "top.genblk2 parameter Q = 5",
        "top generate c3",
        "top generate dn",
    ] {
        assert!(h.contains(&l.to_string()), "{l} in {h:?}");
    }
    assert!(!h.iter().any(|l| l.contains("nope") || l.starts_with("$root instance leaf")));
}

#[test]
fn defparam_bind_and_type_parameters() {
    let h = hier(
        "module c #(parameter W = 1) (); endmodule\nmodule p(input s); endmodule\n\
         module tp #(type T = int, parameter T V = 5) (); endmodule\n\
         module t; c u(); defparam u.W = 16; bind c p pr(.s()); tp #(byte, 3) d(); endmodule\n\
         program pg; endprogram\n",
    );
    for l in ["t.u parameter W = 16", "t.u instance pr : p", "t.d parameter V = 8'sd3", "$root instance pg : pg"] {
        assert!(h.contains(&l.to_string()), "{l} in {h:?}");
    }
}

#[test]
fn string_overrides_are_integral_values() {
    // `M` is overridden with a string: 8 bits a character, and `N` follows.
    let h = hier(
        "module s #(parameter M = 0, localparam N = M + 1) (); endmodule\nmodule t; s #(.M(\"ab\")) i(); endmodule\n",
    );
    for l in ["t.i parameter M = 16'd24930", "t.i parameter N = 32'd24931"] {
        assert!(h.contains(&l.to_string()), "{l} in {h:?}");
    }
}

#[test]
fn zero_width_part_select_is_an_error() {
    let (_, d) = sv_sema::elaborate(&trees(
        "module t; parameter integer c = 0; bit [7:0] a, b; initial b[4+:c] = a[1+:2]; endmodule\n",
    ));
    let msgs: Vec<String> = d.into_iter().map(|d| d.message).collect();
    assert_eq!(msgs, ["indexed part-select width must be positive, not 0"]);
}

#[test]
fn common_cells_constructs_parse() {
    // Typed ports and parameters with packed dimensions, attributes on a
    // connection, a doubly parenthesized implication.
    trees(
        "package p; typedef struct packed { int a; } s_t; localparam s_t [1:0] S = '0; endpackage\n\
         module m import p::*; (input s_t [1:0] x, input logic clk); endmodule\n\
         module t; logic a, b, clk; m i((* async *) .x(), .clk(clk));\n\
         x: assert property (@(posedge clk) ((a && !b |=> b))); endmodule\n",
    );
}
