//! Assertions, checkers, covergroups, procedures and primitives (builder
//! slice N6.2b). Slang's own unit tests are the wider check
//! (tools/slang_unit_gate.py); these cases pin each rule, legal forms
//! included.

fn errors(src: &str) -> Vec<String> {
    let t = sv_syntax::parse_source(src, "t.sv", sv_syntax::Options::default());
    assert!(t.errors.is_empty(), "syntax errors: {:?}", t.errors);
    let t = vec![t];
    let mut e: Vec<String> = sv_sema::sva::check(&t).into_iter().map(|d| d.message).collect();
    e.extend(sv_sema::udp::check(&t).into_iter().map(|d| d.message));
    e
}

#[test]
fn legal_assertions_have_no_errors() {
    let e = errors(
        "module m(input logic clk, a, b);\n\
         sequence s(x, y = 1); x ##1 y; endsequence\n\
         property p; @(posedge clk) disable iff (a) s(a) ##1 b; endproperty\n\
         default disable iff a;\n\
         int n;\n\
         a1: assert property (p) else $error(\"p\");\n\
         a2: assert property (s(.x(a)) |-> (b, n = 1, n++));\n\
         c1: cover property (s(a, b)[*2]);\n\
         restrict property (a);\n\
         always_ff @(posedge clk) n <= n + 1;\n\
         endmodule\n",
    );
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn assertion_rules() {
    let e = errors(
        "module m(input logic clk, a, b);\n\
         sequence s(x, y); x ##1 y; endsequence\n\
         sequence r1; a ##1 r2; endsequence\n\
         sequence r2; r1; endsequence\n\
         property p; disable iff (a) b; endproperty\n\
         property q(ref int v); a; endproperty\n\
         default disable iff a;\n\
         default disable iff b;\n\
         assert property (s(a));\n\
         assert property (s(.x(a), .z(b)));\n\
         assert property (s(a, b, a));\n\
         assert property (p and b);\n\
         assert property (p[*2]);\n\
         assert property (s(a, b)[->1]);\n\
         assert property ((a, b) ##1 b);\n\
         restrict property (a) $display(\"x\");\n\
         assert property (a) else assume property (b);\n\
         clocking cb @(posedge clk); property cp; @(posedge clk) a; endproperty endclocking\n\
         always_ff @(posedge clk) begin #1; end\n\
         endmodule\n",
    );
    let want = [
        "scope already has a default disable declaration",
        "assertion item port direction cannot be 'ref'",
        "too few arguments for 's'; expected 2 but 1 were provided",
        "argument 'z' does not exist in 's'",
        "argument 'y' is missing a value",
        "too many arguments for 's'; expected 2 but 3 were provided",
        "cannot instantiate 'p' here because it has a disable iff condition",
        "property instances cannot specify a repetition",
        "sequences cannot specify a non-consecutive or go-to repetition",
        "sequence match item must be a local variable assignment, increment/decrement operator, or subroutine call",
        "restrict statement cannot have an action block",
        "cannot have 'assume' statement in the action block of another concurrent assertion",
        "sequences and properties declared within clocking blocks cannot specify an explicit clock event",
        "always_ff procedures cannot have blocking timing controls",
        "sequence 'r1' is recursive",
        "sequence 'r2' is recursive",
    ];
    for w in want {
        assert!(e.iter().any(|x| x == w), "missing {w:?} in {e:?}");
    }
}

#[test]
fn checker_rules() {
    let e = errors(
        "checker c(input a, inout logic b, input logic clk);\n\
         rand bit v;\n\
         assign v = a;\n\
         always @(posedge clk) begin end\n\
         initial begin @(posedge clk); assert property (a); end\n\
         initial begin v <= 1; end\n\
         always_ff @(posedge clk) begin #1; end\n\
         endchecker\n",
    );
    let want = [
        "checker ports with an explicit direction declared must also declare a type",
        "checker port direction cannot be 'inout'",
        "continuous assignments and blocking procedural assignments to free checker variables are not allowed",
        "always procedures are not allowed in checkers (use always_comb, always_latch, or always_ff)",
        "statement is not allowed in 'initial' procedure inside a checker",
        "procedural timing control statements in checkers can only use event controls",
        "always_ff procedures cannot have blocking timing controls",
    ];
    for w in want {
        assert!(e.iter().any(|x| x == w), "missing {w:?} in {e:?}");
    }
}

#[test]
fn covergroup_rules() {
    let e = errors(
        "module m(input logic clk);\n\
         int a;\n\
         covergroup cg(output int o, int lim) @(posedge clk);\n\
         option.per_instance = 1; option.foo = 1; type_option.weight = 2;\n\
         coverpoint a { bins x[] = {[0:lim]}; }\n\
         endgroup\n\
         endmodule\n",
    );
    assert_eq!(e, ["covergroups cannot have 'output' or 'inout' arguments", "no member named 'foo' in 'option'"]);
}

#[test]
fn legal_primitives_have_no_errors() {
    let e = errors(
        "primitive srff (q, s, r);\n output q; reg q;\n input s, r;\n initial q = 1'b1;\n\
         table\n 1 0 : ? : 1 ;\n f 0 : 1 : - ;\n 0 r : ? : 0 ;\n 0 f : 0 : - ;\n 1 1 : ? : 0 ;\n\
         endtable\nendprimitive\n\
         primitive X1 (q, clk, d);\n output reg q;\n input clk, d;\n\
         table\n r 0 : ? : 0;\n * 0 : 0 : -;\n * ? : b : -;\n ? * : b : -;\n endtable\nendprimitive\n\
         primitive mux (output o, input a, b, s);\n table\n 0 ? 0 : 0;\n 1 ? 0 : 1;\n ? 0 1 : 0;\n ? 1 1 : 1;\n\
         00x : 0;\n 11x : 1;\n endtable\nendprimitive\n\
         module m; wire o, a, b, s; mux u(o, a, b, s); and g(o, a, b); pmos p(o, a, s); endmodule\n",
    );
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn primitive_rules() {
    let e = errors(
        "primitive p1 (input a, output c);\n table 0:0; endtable\nendprimitive\n\
         primitive p2 (a, b, c);\n output a;\n input b;\n input b;\n table 00:0; endtable\nendprimitive\n\
         primitive p3 (output a, input b);\n table\n 0:0;\n 0:1;\n r:1;\n 1:-;\n z:1;\n endtable\nendprimitive\n\
         primitive p4 (output reg a, input b);\n initial a = 3;\n table\n (11):0:1;\n endtable\nendprimitive\n\
         module m; wire o, a; and g(o); pmos p(o, a, a, a); buf b1(o, ); endmodule\n",
    );
    let want = [
        "the first port in a primitive must be an output port",
        "port 'b' already declared",
        "port 'c' is missing a corresponding body declaration",
        "primitive table row duplicates a set of inputs with a different specified output value",
        "edge transition can only be used in a sequential primitive -- are you missing a 'reg' specifier on the output port?",
        "'-' is only allowed in the output field of a sequential primitive",
        "invalid symbol 'z' in state table",
        "initial value '3' of a primitive must be 0, 1 or x",
        "edge transition must be between two different values",
        "instance of 'and' requires at least one input and one output connection",
        "wrong number of port connections for 'pmos' instance (4 given, expected 3)",
        "only simple expressions are allowed for primitive port connections",
    ];
    for w in want {
        assert!(e.iter().any(|x| x == w), "missing {w:?} in {e:?}");
    }
}

#[test]
fn newer_syntax_parses() {
    // Each was a false syntax error before: sequence and event actuals,
    // `[+]`, `dist` in a property, checker instances (scoped, procedural,
    // bound), modport prototypes, `.*` ports, labeled generate blocks,
    // `default` arguments (1800-2023), patterns in parentheses, cross
    // `with` selections, `I.mp` ports, user nettypes with delays.
    let src = "interface I; logic x; modport a(import task t(int, logic), function void f, input x); endinterface\n\
         package p; checker c(q); endchecker endpackage\n\
         class B; function new(int a); endfunction endclass\n\
         class D extends B(default); function new(default); super.new(default); endfunction endclass\n\
         module m2(.*); endmodule\n\
         module n(x, y); I x; I.a y; nettype int nt; nt #3 w; endmodule\n\
         module m(input logic clk, a, b);\n\
         sequence s(ev, x); @(ev) x; endsequence\n\
         p::c c1(a);\n\
         bind m p::c c2(a);\n\
         assert property (##0 a[*] ##1 b[+]);\n\
         assume property (@(posedge clk) a dist {0:=40, 1:=60});\n\
         cover property (s((posedge clk iff a or edge clk, negedge clk), a ##1 b));\n\
         let l(x) = $past(x,,,@(posedge clk));\n\
         case (2) 2: g: begin end endcase\n\
         initial begin p::c c3(a); end\n\
         typedef union tagged { struct { int a; } J; } T;\n\
         T v;\n\
         initial case (v) matches tagged J ('{.q}) : ; endcase\n\
         covergroup cg; coverpoint a; coverpoint b; X: cross a, b { bins one = binsof(a) with (a > 0) || binsof(b); } endgroup\n\
         endmodule\n";
    let t = sv_syntax::parse_source(src, "t.sv", sv_syntax::Options::default());
    assert!(t.errors.is_empty(), "syntax errors: {:?}", t.errors);
}
