use sv_syntax::{outline, parse_source, Options, SyntaxKind, Tree};

fn parse(src: &str) -> Tree {
    parse_source(src, "t.sv", Options::default())
}

fn lines(t: &Tree) -> Vec<String> {
    outline(t).into_iter().map(|e| format!("{} {}", e.kind, e.name)).collect()
}

fn leaves(t: &Tree) -> usize {
    t.token_range(t.root).len()
}

#[test]
fn round_trip_keeps_every_byte() {
    let src = "// header\n`define W 8\n/* c */ module m #(parameter P = `W) (input logic [P-1:0] a);\n  `ifdef X wire x; `endif\n  always @(posedge a[0]) begin : blk\n    if (a) $display(\"%0d\", a); else ;\n  end\nendmodule : m\n// trailing\n";
    let t = parse(src);
    assert!(t.errors.is_empty(), "{:?}", t.errors);
    assert_eq!(t.file_text(0), src);
    assert_eq!(leaves(&t), t.tokens.len());
}

#[test]
fn class_outline() {
    let src = r#"
package p;
  typedef class c;
  typedef struct packed { bit a; bit b; } pair_t;
  typedef int q_t[$];
  interface class ifc;
    pure virtual function void f();
  endclass
  virtual class c #(type T = int) extends base #(T) implements ifc;
    rand int x;
    constraint c1 { x < 10; }
    extern constraint c2;
    extern virtual task run(int n);
    pure virtual function void f();
    function new(string name = "c");
      super.new(name);
    endfunction : new
    covergroup cg; coverpoint x; endgroup
  endclass
  task c::run(int n);
    fork
      begin #1; end
      wait fork;
    join_none
  endtask
  function automatic int unsigned c::helper();
    return 0;
  endfunction
  constraint c::c2 { x > 0; }
endpackage
"#;
    let t = parse(src);
    assert!(t.errors.is_empty(), "{:?}", t.errors);
    assert_eq!(
        lines(&t),
        [
            "package p",
            "typedef pair_t",
            "typedef q_t",
            "class ifc",
            "function f",
            "class c",
            "constraint c1",
            "constraint c2",
            "task run",
            "function f",
            "function new",
            "covergroup cg",
            "task c::run",
            "function c::helper",
            "constraint c::c2",
        ]
    );
}

#[test]
fn statements_nest() {
    let src = "module m; initial begin\n  case (x) 1, 2: y = 1; default: begin end endcase\n  for (int i = 0; i < 4; i++) if (i) continue;\n  do x++; while (x < 3);\n  a1: assert (x) else $error(\"no\");\n  randcase 1: x = 1; 2: x = 2; endcase\nend endmodule\n";
    let t = parse(src);
    assert!(t.errors.is_empty(), "{:?}", t.errors);
    let kinds: Vec<SyntaxKind> = t.preorder().into_iter().map(|n| t.kind(n)).collect();
    for k in [SyntaxKind::CaseStmt, SyntaxKind::LoopStmt, SyntaxKind::IfStmt, SyntaxKind::DoWhileStmt, SyntaxKind::AssertStmt, SyntaxKind::StmtLabel] {
        assert!(kinds.contains(&k), "missing {k:?}");
    }
}

#[test]
fn generate_constructs() {
    let src = "module m; genvar i;\n for (i = 0; i < 2; i++) begin : g wire w; end\n if (P) begin : a end else begin : b end\n generate case (P) 0: begin end default: ; endcase endgenerate\nendmodule\n";
    let t = parse(src);
    assert!(t.errors.is_empty(), "{:?}", t.errors);
    assert_eq!(t.file_text(0), src);
}

#[test]
fn errors_are_reported_and_tokens_kept() {
    // Missing endclass: the error is reported, the module still parses,
    // and no token is lost.
    let src = "class c;\n  function void f(); endfunction\nendmodule\nmodule m; endmodule\n";
    let t = parse(src);
    assert!(!t.errors.is_empty());
    assert_eq!(t.file_text(0), src);
    assert_eq!(leaves(&t), t.tokens.len());
}

#[test]
fn macros_expand_inside_the_tree() {
    let src = "`define DECL(n) int n;\nclass c; `DECL(a) `DECL(b) endclass\n";
    let t = parse(src);
    assert!(t.errors.is_empty(), "{:?}", t.errors);
    assert_eq!(t.file_text(0), src);
    let decls = t.preorder().into_iter().filter(|&n| t.kind(n) == SyntaxKind::DataDecl).count();
    assert_eq!(decls, 2);
}

/// Canonical forms checked against Slang (build/uvmcheck_cst_exprs).
#[test]
fn expressions_match_slang() {
    let src = r#"module m;
  initial begin
    a = b + c * d ** 2 - -e;
    x <= y ? p : q ? r : s;
    if (a <= b && !c || d inside {[1:3], 5}) z = {2{a, b}};
    v = int'(w) + T'(u) + 8'(k) + {<< 8 {s}};
    o = '{default: 0} ;
    n = obj.m(.a(1), , i++) [3+:4];
    q = #5 a | b & c ^ d;
    r = cond -> a <-> b;
    void'(f(x << 2 >>> 1));
  end
endmodule
"#;
    let t = parse(src);
    assert!(t.errors.is_empty(), "{:?}", t.errors);
    assert_eq!(
        sv_syntax::canonical_exprs(&t),
        [
            "(= a (- (+ b (* c (** d 2))) (- e)))",
            "(<= x (? y p (? q r s)))",
            "(|| (&& (<= a b) (! c)) (inside d {[1:3],5}))",
            "(= z {2{a,b}})",
            "(= v (+ (+ (+ int'(w) T'(u)) 8'(k)) {<<8{s}}))",
            "(= o '{default:0})",
            "(= n obj.m(.a(1),,(post++ i))[3+:4])",
            "(= q #5(| a (^ (& b c) d)))",
            "(= r (-> cond (<-> a b)))",
            "(>>> (<< x 2) 1)",
        ]
    );
}

#[test]
fn declarations_and_statements() {
    let src = r#"class c extends base;
  rand bit [W-1:0] data[$];
  local static uvm_config_db#(virtual bus_if)::type_id x = null, y;
  typedef enum logic [1:0] { IDLE = 0, BUSY = 1 + 1 } state_t;
  function automatic int unsigned f(int a);
    uvm_object o;
    for (int i = 0, j = n - 1; i < j; i++, j--) o = T::type_id::create($sformatf("o%0d", i), this);
    foreach (data[k]) data[k] = k;
    -> done;
    return a;
  endfunction
endclass
"#;
    let t = parse(src);
    assert!(t.errors.is_empty(), "{:?}", t.errors);
    assert_eq!(t.file_text(0), src);
    let n = |k: SyntaxKind| t.preorder().into_iter().filter(|&x| t.kind(x) == k).count();
    assert_eq!(n(SyntaxKind::Declarator), 8);
    assert_eq!(n(SyntaxKind::ReturnStmt), 1);
    assert_eq!(n(SyntaxKind::EventTrigger), 1);
    assert_eq!(
        sv_syntax::canonical_exprs(&t),
        ["(- W 1)", "(+ 1 1)", "(- n 1)", "(< i j)", "(post++ i)", "(post-- j)", "(= o T::type_id::create($sformatf(\"o%0d\",i),this))", "(= data[k] k)"]
    );
}

#[test]
fn begin_keywords_makes_identifiers() {
    let src = "`begin_keywords \"1364-2001\"\nmodule b; reg logic; endmodule\n`end_keywords\n";
    let t = parse(src);
    assert!(t.errors.is_empty(), "{:?}", t.errors);
}
