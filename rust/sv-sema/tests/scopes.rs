//! Scopes and name checks (builder phase N1). The scope dump is checked
//! against Slang's on sv-tests and the repo by tools/sym_gate.py; these
//! cases pin the shapes that decide it.

use sv_sema::{names, scopes};

fn trees(src: &str) -> Vec<sv_syntax::Tree> {
    vec![sv_syntax::parse_source(src, "t.sv", sv_syntax::Options::default())]
}

fn dump(src: &str) -> Vec<String> {
    scopes::build(&trees(src)).dump()
}

fn errors(src: &str) -> Vec<String> {
    let t = trees(src);
    names::check(&t, &scopes::build(&t)).into_iter().map(|d| d.message).collect()
}

#[test]
fn ansi_ports_name_a_net_or_variable() {
    let d = dump("module m (input int a, output int b, output [1:0] c, input var logic e, intf.mp p);\nendmodule\ninterface intf; modport mp(); endinterface\n");
    for l in ["m net a", "m variable b", "m net c", "m variable e", "m port p", "m port a"] {
        assert!(d.contains(&l.to_string()), "{l} in {d:?}");
    }
    assert!(!d.iter().any(|l| l.starts_with("m net p") || l.starts_with("m variable p")));
}

#[test]
fn non_ansi_ports_take_their_declarations() {
    let d =
        dump("module n (a, b, .d(x), {e1, e2});\n input int a; output b; reg b; input x; input e1, e2;\nendmodule\n");
    for l in ["n port a", "n port b", "n port d", "n variable a", "n variable b", "n net x", "n net e1"] {
        assert!(d.contains(&l.to_string()), "{l} in {d:?}");
    }
    assert_eq!(d.iter().filter(|l| l.starts_with("n port")).count(), 3);
}

#[test]
fn classes_subroutines_and_enums() {
    let d = dump("package p;\n class c #(type T = int, N = 2);\n  typedef enum {A, B[2]} e_t;\n  rand T r; covergroup cg; endgroup\n  extern function void f();\n  task t(int x); int y; endtask\n endclass\n function void c::f(); endfunction\nendpackage\n");
    for l in [
        "$root package p",
        "p class c",
        "p.c type_parameter T",
        "p.c type_parameter N",
        "p.c enum_value A",
        "p.c enum_value B0",
        "p.c enum_value B1",
        "p.c variable cg",
        "p.c function f",
        "p.c.t argument x",
        "p.c.t variable y",
    ] {
        assert!(d.contains(&l.to_string()), "{l} in {d:?}");
    }
    // The out-of-block body is the prototype's, not the package's.
    assert!(!d.contains(&"p function c::f".to_string()));
}

#[test]
fn bind_adds_instances_to_its_target() {
    let d = dump("module t; endmodule\nmodule chk; endmodule\nbind t chk u_chk();\n");
    assert!(d.contains(&"t instance u_chk".to_string()), "{d:?}");
}

#[test]
fn name_checks() {
    assert_eq!(errors("module m; reg v; wire v; endmodule\n"), ["redefinition of 'v'"]);
    assert!(errors("module m(a); input a; wire a; endmodule\n").is_empty());
    assert_eq!(errors("typedef missing_t;\n"), ["forward typedef 'missing_t' is never defined"]);
    assert_eq!(errors("typedef other_t my_t;\n"), ["unknown type 'other_t'"]);
    assert!(errors("typedef class c; typedef c c_alias; class c; endclass\n").is_empty());
    assert_eq!(errors("class a; extern constraint k; endclass\n"), ["extern constraint 'k' has no body"]);
    assert!(errors("class a; extern constraint k; endclass\nconstraint a::k { }\n").is_empty());
    assert_eq!(
        errors("class a; function void randomize(); endfunction endclass\n"),
        ["'randomize' is a built-in method and cannot be overridden"]
    );
    assert_eq!(errors("class a; pure virtual task t; endclass\n").len(), 1);
    assert!(errors("virtual class a; pure virtual task t; endclass\n").is_empty());
    assert_eq!(
        errors("module m; virtual class b; endclass\n b o; initial o = new; endmodule\n"),
        ["cannot instantiate 'b': it is an abstract or interface class"]
    );
    assert!(errors(
        "module m; virtual class b; endclass\n class c extends b; endclass\n c o; initial o = new; endmodule\n"
    )
    .is_empty());
    assert_eq!(
        errors("module m; initial randsequence(main) main: first secnd; first: {}; endsequence endmodule\n"),
        ["unknown production 'secnd'"]
    );
}

fn types(src: &str) -> Vec<String> {
    let t = trees(src);
    let s = scopes::build(&t);
    let ty = sv_sema::symbol_types(&t, &s);
    s.dump_with(Some(&ty)).into_iter().filter(|l| l.contains(" : ")).collect()
}

#[test]
fn types_as_slang_prints_them() {
    let d = types(
        "package q; typedef int t; endpackage\nmodule m import q::*; #(parameter W = 8, type T = bit) (input [3:0] a, output int b);\n typedef enum {A, B} e_t;\n localparam S = \"ab\";\n t x; e_t e; T y; nettype logic [7:0] n_t; n_t n;\nendmodule\n",
    );
    for l in [
        "m parameter W : logic signed[31:0]",
        "m net a : logic[3:0]",
        "m variable b : int",
        "m typedef e_t : enum{A=32'sd0,B=32'sd1}m.e_t",
        "m parameter S : logic[15:0]",
        "m variable x : q::t",
        "m variable e : m.e_t",
        "m variable y : m.T",
        "m net n : logic[7:0]",
    ] {
        assert!(d.contains(&l.to_string()), "{l} in {d:?}");
    }
}

fn type_errors(src: &str) -> Vec<String> {
    let t = trees(src);
    sv_sema::checks::check(&t, &scopes::build(&t)).into_iter().map(|d| d.message).collect()
}

#[test]
fn type_checks() {
    let bad = [
        "module m; real a; wire b; assign b = a[2]; endmodule\n",
        "module m; real a; always @(posedge a) ; endmodule\n",
        "module m; struct packed { bit [3:0] lo = 1; } p; endmodule\n",
        "module m; wire w; initial w = 1; endmodule\n",
        "module m; int a, b; initial a = b = 5; endmodule\n",
        "module m; function void f(); return 1; endfunction endmodule\n",
        "module m; function int f(); fork join_any return 1; endfunction endmodule\n",
        "module m; task t; fork return; join_none endtask endmodule\n",
        "module m; enum bit [1:0] {a = 0, b = 2'bxx} v; endmodule\n",
        "module m; enum logic [2:0] {a = 4'h2} v; endmodule\n",
        "module m; typedef enum {a, b} e; initial begin e v; v = 1; end endmodule\n",
        "module m; typedef enum {a, b} e; initial begin e v; v += 1; end endmodule\n",
        "module m; specparam d = 5; parameter p = d + 1; endmodule\n",
        "class c; randc int b; constraint k { soft b > 4; } endclass\n",
        "virtual class a; pure constraint k; endclass\nclass b extends a; endclass\n",
    ];
    for src in bad {
        assert_eq!(type_errors(src).len(), 1, "{src}: {:?}", type_errors(src));
    }
    let good = [
        "module m; real a[]; real s; initial s = a[0]; endmodule\n",
        "module m; logic w; initial w = 1; wire x; assign x = w; endmodule\n",
        "module m; int a, b; initial a = (b = 5); endmodule\n",
        "module m; function int f(); fork join_none return 1; endfunction endmodule\n",
        "module m; typedef enum {a, b} e; initial begin e v; v = b; end endmodule\n",
        "module m; enum logic [1:0] {a = 2'b0x, b = 2'b11} v; endmodule\n",
        "virtual class a; pure constraint k; endclass\nclass b extends a; constraint k { } endclass\n",
    ];
    for src in good {
        assert!(type_errors(src).is_empty(), "{src}: {:?}", type_errors(src));
    }
}
