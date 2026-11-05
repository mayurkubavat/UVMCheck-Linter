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
