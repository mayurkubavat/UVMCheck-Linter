//! Constructs from real testbenches (riscv-dv, ibex, cva6) that the native
//! front end once rejected or built differently from Slang.

use std::io::Write;

fn opts(defines: &[&str]) -> sv_syntax::Options {
    sv_syntax::Options { defines: defines.iter().map(|d| d.to_string()).collect(), ..Default::default() }
}

fn clean(src: &str, defines: &[&str]) -> Vec<sv_syntax::Tree> {
    let t = sv_syntax::parse_source(src, "t.sv", opts(defines));
    assert!(t.errors.is_empty() && t.pp_diagnostics.is_empty(), "errors: {:?} {:?}", t.errors, t.pp_diagnostics);
    vec![t]
}

#[test]
fn paste_next_to_whitespace_is_dropped() {
    // riscv-dv: `covergroup ``NAME``_cg` names NAME_cg, not covergroupNAME_cg.
    let t = clean(
        "`define CG(N) covergroup ``N``_cg; \\\n  cp : coverpoint x; \\\n endgroup\n\
         module m; bit [3:0] x; `CG(add) endmodule\n",
        &[],
    );
    let s = sv_sema::scopes::build(&t);
    assert!(s.dump_full(None, None).iter().any(|l| l == "m covergroup add_cg"));
}

#[test]
fn colon_before_a_comment_is_not_a_dist_weight() {
    // cva6 vendor RTL: `3'b000://comment` in a case item.
    clean(
        "module m; logic [2:0] x; logic y; always_comb case (x) 3'b000://c\n y = 1; default: ; endcase endmodule\n",
        &[],
    );
}

#[test]
fn based_literal_digits_from_a_macro() {
    // ibex: `32'h`DM_ADDR` with DM_ADDR=1A11_0000 lexes the digits as `1` and `A11_0000`.
    let t = clean(
        "module m; parameter int unsigned A = 32'h`DM; parameter int unsigned B = 32'h`DM + (32'h`MK + 1); endmodule\n",
        &["DM=1A11_0000", "MK=0000_0FFF"],
    );
    let s = sv_sema::scopes::build(&t);
    let (tys, vals) = sv_sema::symbol_info(&t, &s);
    let lines = s.dump_full(Some(tys.as_slice()), Some(vals.as_slice()));
    assert!(lines.iter().any(|l| l == "m parameter A : int unsigned = 32'd437321728"), "{lines:?}");
    assert!(lines.iter().any(|l| l == "m parameter B : int unsigned = 32'd437325824"), "{lines:?}");
}

#[test]
fn macros_carry_from_one_file_to_the_next() {
    let dir = std::env::temp_dir().join(format!("n6-macros-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b) = (dir.join("a.sv"), dir.join("b.sv"));
    std::fs::File::create(&a).unwrap().write_all(b"`define M 5\npackage a; endpackage\n").unwrap();
    std::fs::File::create(&b).unwrap().write_all(b"package b; int x = `M; endpackage\n").unwrap();
    let trees = sv_syntax::parse_files(&[&a, &b], Default::default());
    assert!(trees.iter().all(|t| t.errors.is_empty() && t.pp_diagnostics.is_empty()));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn top_option_picks_the_roots() {
    let t = clean("module a; endmodule\nmodule b; endmodule\nmodule top; a u(); endmodule\n", &[]);
    let (all, _) = sv_sema::elaborate(&t);
    assert!(all.contains(&"$root instance b : b".to_string()));
    let (only, _) = sv_sema::elaborate_tops(&t, &["top".to_string()]);
    assert!(only.iter().all(|l| !l.starts_with("$root instance b ")), "{only:?}");
    assert!(only.contains(&"top instance u : a".to_string()), "{only:?}");
}

#[test]
fn c_files_in_a_file_list_are_not_sources() {
    assert!(sv_source::is_foreign(std::path::Path::new("dv/common/date.c")));
    assert!(sv_source::is_foreign(std::path::Path::new("lib/dpi.so")));
    assert!(!sv_source::is_foreign(std::path::Path::new("rtl/top.sv")));
    assert!(!sv_source::is_foreign(std::path::Path::new("inc/defs.svh")));
}

fn values(src: &str) -> Vec<String> {
    let t = clean(src, &[]);
    let s = sv_sema::scopes::build(&t);
    let (tys, vals) = sv_sema::symbol_info(&t, &s);
    s.dump_full(Some(tys.as_slice()), Some(vals.as_slice()))
}

/// `sym` has value `val`, whatever its type text.
fn has(lines: &[String], sym: &str, val: &str) {
    let ok = lines.iter().any(|l| l.starts_with(&format!("{sym} ")) && l.ends_with(&format!(" = {val}")));
    assert!(ok, "missing `{sym} = {val}` in {lines:?}");
}

#[test]
fn values_wider_than_128_bits() {
    // common_cells: an array of seed structs is one 192-bit value.
    let l = values(
        "package q; typedef struct packed { int unsigned P; int unsigned X; } s_t;\n\
         localparam s_t [2:0] E = '{ '{P: 32'd3, X: 32'd4}, '{P: 32'd5, X: 32'd6}, '{P: 32'd7, X: 32'd8} };\n\
         localparam bit [199:0] W = 200'h1 << 190; endpackage\n\
         module m #(parameter q::s_t [2:0] S = q::E); endmodule\n",
    );
    has(&l, "q parameter E", "192'd43845049133538382226909491646843496411...e11");
    has(&l, "m parameter S", "192'd43845049133538382226909491646843496411...e11");
    has(&l, "q parameter W", "200'd156927543384667019095894735580191660402...e19");
}

#[test]
fn constant_functions_with_case_and_bit_updates() {
    // fpnew: nested calls, casts to an enum, `res[i] |= ...` and case.
    let l = values(
        "package p; typedef enum logic [1:0] {A, B, C} e_t; typedef logic [0:3] m_t;\n\
         function automatic int w(e_t e); case (e) A: return 8; B: return 16; default: return 32; endcase endfunction\n\
         function automatic int mx(m_t c); int r = 0; for (int i = 0; i < 3; i++) if (c[i] && w(e_t'(i)) > r) r = w(e_t'(i)); return r; endfunction\n\
         function automatic m_t f(m_t c); m_t r = '0; for (int i = 0; i < 4; i++) if (i < 2) r[i] |= c[i]; return r; endfunction\n\
         localparam int MX = mx(4'b0110); localparam m_t F = f(4'b1101); endpackage\n",
    );
    has(&l, "p parameter MX", "32");
    has(&l, "p parameter F", "4'b1100");
}

#[test]
fn literals_take_their_target_type() {
    // ibex `int UseDsp = "no"`; tech_cells `int unsigned Delay = 300ps`.
    let l = values(
        "module m #(parameter int unsigned D = 300ps, parameter int E = 2ns, parameter int F = 2.6,\n\
         parameter int U = \"no\"); endmodule\n\
         `timescale 1ps/1ps\nmodule n #(parameter int unsigned D = 300ps); endmodule\n\
         module k; timeunit 100ps; localparam int T = 1ns; endmodule\n",
    );
    has(&l, "m parameter D", "32'd0");
    has(&l, "m parameter E", "2");
    has(&l, "m parameter F", "3");
    has(&l, "m parameter U", "28271");
    has(&l, "n parameter D", "32'd300");
    has(&l, "k parameter T", "10");
}

#[test]
fn unknown_bits_follow_slang() {
    // cva6: widths from an all-zero config divide by zero.
    let l = values(
        "module m #(parameter int A = 0, parameter int B = 0);\n\
         localparam int unsigned P = A / B; localparam P2 = A / B; localparam logic [7:0] P4 = A / B;\n\
         localparam int P5 = $clog2(A / B); localparam P7 = (A / B) == 0; localparam P8 = 0 && (A / B);\n\
         localparam P11 = (A / B) ? 3 : 4; localparam logic [3:0] P13 = 4'b10x1; localparam bit [3:0] P16 = 4'b10x1;\n\
         localparam P17 = {4'b1010, 4'bxxxx}; localparam P19 = (A/B) & 0; localparam P22 = 4'bzzzz; localparam P23 = 8'hx5;\n\
         localparam int unsigned W = 0; localparam logic [W-1:0] R = 0; localparam logic [3:0] L = {W{1'b1}};\n\
         endmodule\n",
    );
    for (sym, val) in [
        ("P", "32'd0"),
        ("P2", "32'sdx"),
        ("P4", "8'dx"),
        ("P5", "0"),
        ("P7", "1'bx"),
        ("P8", "1'b0"),
        ("P11", "32'sb0xxx"),
        ("P13", "4'b10x1"),
        ("P16", "4'b1001"),
        ("P17", "8'b1010xxxx"),
        ("P19", "0"),
        ("P22", "4'bzzzz"),
        ("P23", "8'bxxxx0101"),
        ("R", "2'b0"),
        ("L", "<unset>"),
    ] {
        has(&l, &format!("m parameter {sym}"), val);
    }
}

#[test]
fn macros_expand_inside_stringified_text() {
    // UVM: `define UVM_VERSION_STRING `"`UVM_NAME``-```UVM_MAJOR_REV...`"
    let l = values(
        "`define N UVM\n`define M 1\n`define S `\"`N``-```M``.```M```\"\n\
         package p; parameter string r = `S; endpackage\n",
    );
    has(&l, "p parameter r", "\"UVM-1.1\"");
}
