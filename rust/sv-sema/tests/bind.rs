//! Name binding and class rules (builder phase N5). The IEEE gate checks
//! them on sv-tests (no error on legal code, the illegal cases rejected)
//! and tools' repo checks on the repo; these cases pin the rules.

fn trees(src: &str) -> Vec<sv_syntax::Tree> {
    let t = sv_syntax::parse_source(src, "t.sv", sv_syntax::Options::default());
    assert!(t.errors.is_empty(), "syntax errors: {:?}", t.errors);
    vec![t]
}

fn errors(src: &str) -> Vec<String> {
    let t = trees(src);
    let mut e: Vec<String> = sv_sema::bind::check(&t).into_iter().map(|d| d.message).collect();
    e.extend(sv_sema::iclass::check(&t).into_iter().map(|d| d.message));
    e.extend(sv_sema::checks::check(&t, &sv_sema::scopes::build(&t)).into_iter().map(|d| d.message));
    e
}

#[test]
fn undeclared_names_but_not_what_scopes_declare() {
    let e = errors(
        "package p; typedef int t; localparam K = 1; endpackage\n\
         module m import p::*; (input a);\n wire \\w1 ;\n m2 u(.x(imp));\n assign alsoimp = a;\n\
         int arr[4]; t v = K;\n\
         initial begin foreach (arr[i]) arr[i] = i; v = w1 + imp + alsoimp; my_task; v = nope; end\n\
         task my_task; endtask\nendmodule\nmodule m2(input x); endmodule\n",
    );
    assert_eq!(e, ["use of undeclared identifier 'nope'"]);
}

#[test]
fn class_members_come_through_extends_not_implements() {
    let e = errors(
        "interface class i; typedef int it; pure virtual function void f(it x); endclass\n\
         class b; int bm; endclass\n\
         class c extends b implements i; virtual function void f(it x); bm = 1; endfunction endclass\n\
         function void c::g(); bm = 2; endfunction\n",
    );
    assert_eq!(e, ["use of undeclared identifier 'it'"]);
}

#[test]
fn interface_class_rules() {
    let e = errors(
        "interface class a #(type T = int); pure virtual function void f(T x); endclass\n\
         interface class b #(type T = int); pure virtual function int f(); endclass\n\
         interface class d extends a#(bit), b; endclass\n\
         class e #(type T = a) implements T; endclass\n\
         class g #(int N = 1); static int k; endclass\n\
         module m; initial $display(g::k, g#()::k); endmodule\n",
    );
    for m in [
        "'d' inherits method 'f' with different prototypes",
        "'d' inherits type 'T' from more than one interface class",
        "'T' is a type parameter; a class can only implement an interface class",
        "parameterized class 'g' needs a specialization ('g#()') here",
    ] {
        assert!(e.contains(&m.to_string()), "{m} in {e:?}");
    }
    assert_eq!(e.len(), 4, "{e:?}");
}

#[test]
fn stream_width_and_array_patterns() {
    let e = errors(
        "module m; int a, b; int ok = {<<{a}}; int d = {<<{a, b}};\n\
         typedef struct { int x; int y; } s_t; s_t s2[1:0] = '{0, 0, 1, 1}; s_t s3[2] = '{'{0, 0}, '{1, 1}};\n\
         int r[4] = '{4{0}}; endmodule\n",
    );
    assert_eq!(e, ["stream of 64 bits is wider than its 32-bit target", "array 's2' has 2 elements, not 4"]);
}
