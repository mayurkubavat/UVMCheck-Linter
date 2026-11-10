//! System calls, assignability, constant expressions, declaration before
//! use and top-level interface ports (builder slice N6.2c). Slang's own
//! unit tests are the wider check (tools/gates/slang_unit_gate.py); these cases
//! pin each rule, legal forms included.

fn errors(src: &str) -> Vec<String> {
    let t = sv_syntax::parse_source(src, "t.sv", sv_syntax::Options::default());
    assert!(t.errors.is_empty(), "syntax errors: {:?}", t.errors);
    let t = vec![t];
    let scopes = sv_semantic::scopes::build(&t);
    let mut e: Vec<String> = sv_semantic::calls::check(&t).into_iter().map(|d| d.message).collect();
    e.extend(sv_semantic::checks::check(&t, &scopes).into_iter().map(|d| d.message));
    e.extend(sv_semantic::order::check(&t, &scopes).into_iter().map(|d| d.message));
    e.extend(sv_semantic::elaborate_full(&t, &[]).diags.into_iter().map(|d| d.message));
    e
}

fn assert_has(e: &[String], want: &[&str]) {
    for w in want {
        assert!(e.iter().any(|x| x == w), "missing {w:?} in {e:?}");
    }
}

#[test]
fn legal_code_has_no_errors() {
    let e = errors(
        "interface I; logic x; modport mp(input x); endinterface\n\
         module sub(I.mp p); endmodule\n\
         module m(input logic clk);\n\
         localparam int W = $clog2(16);\n\
         typedef enum {A, B} e_t;\n\
         int seed, i, q[4], k;\n\
         string s;\n\
         logic [W-1:0] v [$bits(q)];\n\
         I bus(); sub u(.p(bus));\n\
         for (genvar g = 0; g < W; g++) assign v[g] = g;\n\
         genvar j;\n\
         for (j = 0; j < 2; j++) begin : b int k; end\n\
         if (W > 2) begin : c end\n\
         $info(\"%d\", W);\n\
         initial begin\n\
           $display; $display(); $finish;\n\
           i = $random(seed); i = $urandom(1); i = $countbits(i, , 1);\n\
           $sformat(s, \"%d\", i); i = $sscanf(s, \"%d\", i); void'($cast(k, i));\n\
           i++; --q[0]; {i, k} = 2; i <= 1;\n\
           if ((i + 1) <= 3) i = $past(i, , , @(posedge clk));\n\
           foreach (q[n]) q[n] = n;\n\
         end\n\
         specify $setup(clk, clk, 1); $width(posedge clk, 1); endspecify\n\
         function [W-1:0] f; input [W-1:0] W; f = W; endfunction\n\
         endmodule\n",
    );
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn system_call_argument_counts() {
    let e = errors(
        "module m; int a;\n\
         specify $setup(a); $hold(a, a, 1, a, a); $foobar(a); endspecify\n\
         initial begin a = $clog2(); a = $clog2(3, 4); $readmemb(\"x\"); a = $fclose; a = $typename(a, a); end\n\
         endmodule\n",
    );
    assert_has(
        &e,
        &[
            "too few arguments for '$setup'; expected 3 but 1 were provided",
            "too many arguments for '$hold'; expected 4 but 5 were provided",
            "unknown system timing check '$foobar'",
            "too few arguments for '$clog2'; expected 1 but 0 were provided",
            "too many arguments for '$clog2'; expected 1 but 2 were provided",
            "too few arguments for '$readmemb'; expected 2 but 1 were provided",
            "too many arguments for '$typename'; expected 1 but 2 were provided",
        ],
    );
}

#[test]
fn not_assignable() {
    let e = errors(
        "module m; parameter P = 1; int i; string s;\n\
         initial begin ++(1 + 1); (1 + 1) = 2; P = 3; $swrite(1, \"a\"); i = $fgets(1, i);\n\
         i = $dist_uniform(5, 1, 2); i = $sscanf(s, \"%d\", i, 1); end\n\
         endmodule\n",
    );
    assert_eq!(e.iter().filter(|x| *x == "expression is not assignable").count(), 7, "{e:?}");
}

#[test]
fn constant_expressions() {
    let e = errors(
        "module m(input int j = foo); int foo; logic [7:0] w;\n\
         localparam int a = $countbits(foo, 0) + $bits(foo);\n\
         logic [foo:0] x;\n\
         struct { int x = foo; } s;\n\
         if (w) begin end\n\
         endmodule\n",
    );
    let n = e.iter().filter(|x| x.starts_with("reference to non-constant variable")).count();
    assert_eq!(n, 5, "{e:?}");
}

#[test]
fn declared_before_use() {
    let e = errors(
        "module m;\n\
         function int f; return x1; endfunction int x1;\n\
         initial x2 = 1; int x2;\n\
         t1 v; typedef logic t1;\n\
         initial g(); task g; endtask\n\
         typedef enum e2; e2 y; typedef enum {E} e2;\n\
         endmodule\n\
         interface I; logic a; modport mp(input a, b); logic b; endinterface\n\
         module n(a); input [s-1:0] a; parameter s = 2; endmodule\n",
    );
    let got: Vec<&str> = e.iter().map(String::as_str).filter(|x| x.contains("before its declaration")).collect();
    assert_eq!(
        got,
        [
            "identifier 'x1' used before its declaration",
            "identifier 'x2' used before its declaration",
            "identifier 't1' used before its declaration",
            "identifier 'b' used before its declaration",
            "identifier 's' used before its declaration",
        ]
    );
}

#[test]
fn top_level_interface_ports() {
    let e = errors(
        "interface I; modport mp(); endinterface\n\
         module m(I a, I.mp b, interface c, input logic d); endmodule\n\
         module n(x); I x; endmodule\n\
         module used(I a); endmodule\n\
         module top; I i(); used u(i); endmodule\n",
    );
    assert_eq!(
        e,
        [
            "top-level module 'm' has unconnected interface port 'a'",
            "top-level module 'm' has unconnected interface port 'b'",
            "top-level module 'm' has unconnected interface port 'c'",
            "top-level module 'n' has unconnected interface port 'x'",
        ]
    );
}

fn class_errors(src: &str) -> Vec<String> {
    let t = sv_syntax::parse_source(src, "t.sv", sv_syntax::Options::default());
    assert!(t.errors.is_empty(), "syntax errors: {:?}", t.errors);
    sv_semantic::classes::check(&[t]).into_iter().map(|d| d.message).collect()
}

#[test]
fn legal_classes_have_no_errors() {
    let e = class_errors(
        "class A; static int n; int p; static int w = $bits(p);\n\
         static function int get; int p; p = n; return p; endfunction\n\
         virtual function void f(int a, output int b); endfunction\n\
         extern function int g(); endclass\n\
         function int A::g(); return p; endfunction\n\
         class B extends A; function void f(int a, output int b); super.f(a, b); endfunction endclass\n\
         class C #(type T = int, int N); endclass\n\
         module m #(parameter int P) (); parameter int Q = 1; localparam R = 2; endmodule\n",
    );
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn class_rules() {
    let e = class_errors(
        "class A; int p; static int s = p; function void f; super.f(); endfunction\n\
         virtual function int v(output int x); endfunction virtual task w(int a); endtask\n\
         extern function int g(); endclass\n\
         function real A::g(); endfunction\n\
         function real A::g(); endfunction\n\
         class B extends A; function int v(input int x); endfunction task w; endtask endclass\n\
         module m #(localparam int L) (); localparam x; parameter type T; endmodule\n",
    );
    assert_has(
        &e,
        &[
            "invalid use of non-static class property 'p'",
            "class 'A' does not have a super class",
            "return type 'real' of method 'g' does not match 'int' declared in its prototype",
            "redefinition of 'A::g'",
            "direction of virtual method argument 'x' differs from the direction given in its superclass method",
            "virtual method 'w' has different number of arguments from its superclass method",
            "local parameter is missing an initializer",
            "parameter declaration is missing an initializer",
        ],
    );
}

fn typed_errors(src: &str) -> Vec<String> {
    let t = sv_syntax::parse_source(src, "t.sv", sv_syntax::Options::default());
    assert!(t.errors.is_empty(), "syntax errors: {:?}", t.errors);
    let t = vec![t];
    let scopes = sv_semantic::scopes::build(&t);
    sv_semantic::typed::check(&t, &scopes).into_iter().map(|d| d.message).collect()
}

#[test]
fn typed_members_and_method_arguments() {
    let src = "package p; int x; endpackage\n\
         module m; typedef enum {A, B} e_t; e_t e; int q[$]; int aa[string]; int d[]; string s;\n\
         typedef struct packed { logic foo; logic bar; } st_t; st_t st;\n\
         class C; int x; endclass C c;\n\
         function int f(int a, int b = 1); endfunction\n\
         initial begin\n\
         e = e.next(); e = e.next(2); q.insert(0, 1); void'(aa.num()); void'(s.substr(0, 1));\n\
         st.foo = 1; c.x = 1; void'(c.randomize()); void'(f(1)); void'(f(1, 2)); void'(p::x);\n\
         begin int e; e = 1; end\n\
         e = e.first(1); void'(q.pop_front(1)); q.push_back(); void'(aa.exists()); d.delete(1);\n\
         st.foi = 1; c.y = 1; void'(f()); void'(p::z);\n\
         end endmodule\n";
    let e = typed_errors(src);
    assert_eq!(
        e,
        [
            "too many arguments for 'first'; expected 0 but 1 were provided",
            "too many arguments for 'pop_front'; expected 0 but 1 were provided",
            "too few arguments for 'push_back'; expected 1 but 0 were provided",
            "too few arguments for 'exists'; expected 1 but 0 were provided",
            "too many arguments for 'delete'; expected 0 but 1 were provided",
            "no member named 'foi' in 'struct packed{logic foo;logic bar;}m.s$1'",
            "no member named 'y' in class 'C'",
            "too few arguments for 'f'; expected 1 but 0 were provided",
            "no member named 'z' in package 'p'",
        ]
    );
}
