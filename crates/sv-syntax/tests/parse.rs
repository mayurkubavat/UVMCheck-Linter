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
    assert_eq!(n(SyntaxKind::Declarator), 9); // includes the port `a`
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

/// Constraints, covergroups, assertions, clocking, headers: canonical
/// forms checked against Slang (build/uvmcheck_cst_exprs).
#[test]
fn slice3_matches_slang() {
    let src = r#"module m(input logic clk, rst, a, b, input logic [3:0] c);
  default clocking cb @(posedge clk iff !rst);
    default input #1step output #2;
    input a, b;
  endclocking
  default disable iff (rst || c == 0);
  sequence s1(x, int n = 2);
    int v;
    (x && c > 1, v = c + 1) ##[1:n+1] (b ##1 c == v)[*2:$];
  endsequence
  property p1;
    @(posedge clk) disable iff (rst) a |-> s1(b) and not (c[0] throughout b[->1]) or (c != 0) |=> s_eventually [1:3] a;
  endproperty
  a1: assert property (p1) else $error("p1 %0d", c + 1);
  c1: cover property (@(posedge clk) first_match(a ##1 b, $display("x")) intersect (a within b)) ;
  let max(x, y) = (x > y) ? x : y;
  initial begin
    expect (@(posedge clk) a ##1 b) $display("ok"); else $error("bad");
    assume property (@(negedge clk) if (a) b |=> c[1] else !b);
  end
  covergroup cg @(posedge clk);
    option.per_instance = 1;
    cp: coverpoint c iff (!rst) { bins lo[2] = {[0:3], 5} with (item % 2 == 0); bins t = (1 => 2[*2] => 3), (4 => 5); illegal_bins i = default; }
    x: cross cp, a { bins z = binsof(cp) intersect {1} && !binsof(a.b); ignore_bins q = binsof(cp.lo) || binsof(a); }
  endgroup
endmodule
class k #(int W = 4 + 1, type T = int) extends base #(W * 2) implements i1, i2;
  rand bit [W-1:0] d;
  constraint c1 { soft d < 10; d dist { [0:3] := 1, 4 :/ 2 }; d > 1 -> { d < W - 1; } if (W > 2) d != 3; else d != 4; foreach (q[i]) q[i] < i + 1; unique {d, e}; solve d before e; }
  extern function void f(input int a = 1 + 2, ref T b[$]);
endclass
"#;
    let t = parse(src);
    assert!(t.errors.is_empty(), "{:?}", t.errors);
    assert_eq!(t.file_text(0), src);
    assert_eq!(
        sv_syntax::canonical_exprs(&t),
        [
            r#"(! rst)"#,
            r#"(|| rst (== c 0))"#,
            r#"(&& x (> c 1))"#,
            r#"(= v (+ c 1))"#,
            r#"(+ n 1)"#,
            r#"(== c v)"#,
            r#"(!= c 0)"#,
            r#"(+ c 1)"#,
            r#"(? ((> x y)) x y)"#,
            r#"(! b)"#,
            r#"(= option.per_instance 1)"#,
            r#"(! rst)"#,
            r#"(== (% item 2) 0)"#,
            r#"(+ 4 1)"#,
            r#"(* W 2)"#,
            r#"(- W 1)"#,
            r#"(< d 10)"#,
            r#"(> d 1)"#,
            r#"(< d (- W 1))"#,
            r#"(> W 2)"#,
            r#"(!= d 3)"#,
            r#"(!= d 4)"#,
            r#"(< q[i] (+ i 1))"#,
            r#"(+ 1 2)"#,
        ]
    );
}

#[test]
fn slice4_matches_slang() {
    // Specify blocks, UDPs, a checker, attributes, nettype, randsequence
    // and a config; the expected lines are Slang's.
    let src = r#"`timescale 1ns/1ps
(* top_attr = 1 + 2, keep *)
module dff (output reg q, input d, clk, rst, en);
  specparam tRise = 1 + 1 : 2 * 2 : 3, tFall = 4;
  specparam [7:0] w = 8'hff & 8'h0f;
  (* full_case, parallel_case = 2 - 1 *)
  always @(posedge clk) q <= d;
  specify
    specparam PATHPULSE$clk$q = (1, 2), tS = 2 + 1;
    (clk => q) = (tRise, tFall);
    (posedge clk => (q +: d)) = (1:2:3, 4:5:6);
    (negedge rst *> q) = 3;
    if (en == 1'b1) (d +=> q) = tS * 2;
    if (!rst && en) (d -*> q) = 1;
    ifnone (d => q) = 2;
    (d, en *> q) = (1, 2, 3, 4, 5, 6);
    $setup(d, posedge clk &&& (en == 1), tS + 1, ntfr);
    $hold(posedge clk, d &&& en, 2);
    $setuphold(posedge clk, d, 1, 2, ntfr, , , dclk, dd);
    $width(negedge clk, 1:2:3);
    $period(edge [01, 10] clk, 10);
    $recrem(posedge rst, posedge clk, 1, 2);
    pulsestyle_onevent q;
    showcancelled q;
  endspecify
  nettype real rnet;
endmodule

primitive mux (out, sel, a, b);
  output out;
  input sel, a, b;
  table
    0 1 ? : 1 ;
    0 0 ? : 0 ;
    1 ? 1 : 1 ;
    1 ? 0 : 0 ;
    x 0 0 : 0 ;
  endtable
endprimitive

primitive latch (q, clk, d);
  output q; reg q;
  input clk, d;
  initial q = 1'b1;
  table
    // clk d : q : q+
    r 0 : ? : 0 ;
    (01) 1 : ? : 1 ;
    f ? : ? : - ;
    * ? : ? : - ;
  endtable
endprimitive : latch

primitive ansi_udp (output reg q = 1'b0, input a, input b);
  table
    0 0 : ? : 0;
    1 1 : ? : 1;
  endtable
endprimitive

checker my_check (logic a, b, event clk = $global_clock);
  default clocking @clk; endclocking
  logic [1:0] cnt = a + b;
  a1: assert property (a |-> ##1 b) else $error("x %0d", cnt + 1);
  always_ff @clk cnt <= cnt + 1;
  rand bit r;
  cover property (r && a);
endchecker : my_check

module top;
  logic a, b, clk;
  my_check c1 (a, b, posedge clk);
  int x;
  function automatic int F(int w);
    int y = 0;
    randsequence (main)
      main : first := 2 | rand join (0.5 + 0.1) second third := (w + 1) { y++; } | repeat (w - 1) fourth;
      first : { y = y + 1; } fifth(3 + 4);
      int second : { y = y * 2; return y + 1; };
      third : if (w > 3 && y < 2) fourth else fifth(1) ;
      fourth : case (w % 3)
        0, 1 : first;
        2 : third;
        default : fifth(w);
      endcase;
      void fifth(int k) : { y += k << 1; };
    endsequence
    return y;
  endfunction
  initial x = F (* call_attr *) (5);
  assign a = b ? (* cond_attr *) clk : ~clk;
endmodule

config cfg;
  localparam W = 8;
  design work.top;
  default liblist work lib2;
  instance top.c1 liblist lib3;
  cell dff use lib2.dff;
endconfig : cfg
"#;
    let t = parse(src);
    assert!(t.errors.is_empty(), "{:?}", t.errors);
    assert_eq!(t.file_text(0), src);
    assert_eq!(lines(&t), ["module dff", "module top", "function F"]);
    assert_eq!(
        sv_syntax::canonical_exprs(&t),
        [
            r#"(+ 1 2)"#,
            r#"(+ 1 1)"#,
            r#"(* 2 2)"#,
            r#"(& 8'hff 8'h0f)"#,
            r#"(- 2 1)"#,
            r#"(<= q d)"#,
            r#"(+ 2 1)"#,
            r#"(== en 1'b1)"#,
            r#"(* tS 2)"#,
            r#"(&& (! rst) en)"#,
            r#"(== en 1)"#,
            r#"(+ tS 1)"#,
            r#"(+ a b)"#,
            r#"(+ cnt 1)"#,
            r#"(<= cnt (+ cnt 1))"#,
            r#"(&& r a)"#,
            r#"(+ 0.5 0.1)"#,
            r#"(+ w 1)"#,
            r#"(post++ y)"#,
            r#"(- w 1)"#,
            r#"(= y (+ y 1))"#,
            r#"(+ 3 4)"#,
            r#"(= y (* y 2))"#,
            r#"(+ y 1)"#,
            r#"(&& (> w 3) (< y 2))"#,
            r#"(% w 3)"#,
            r#"(+= y (<< k 1))"#,
            r#"(= x F(5))"#,
            r#"(= a (? b clk (~ clk)))"#,
        ]
    );
    let kinds: Vec<SyntaxKind> = t.preorder().into_iter().map(|n| t.kind(n)).collect();
    for k in [SyntaxKind::SpecifyBlock, SyntaxKind::PathDecl, SyntaxKind::TimingCheck, SyntaxKind::UdpDecl,
              SyntaxKind::UdpTable, SyntaxKind::UdpInitial, SyntaxKind::ConfigDecl, SyntaxKind::Production,
              SyntaxKind::RsWeight, SyntaxKind::RsRandJoin, SyntaxKind::RsCase, SyntaxKind::AttrSpec,
              SyntaxKind::NetTypeDecl, SyntaxKind::CheckerDecl] {
        assert!(kinds.contains(&k), "no {k:?}");
    }
}

#[test]
fn reports_what_slang_rejects() {
    // Each source holds one error Slang's parser reports; none of them
    // may be silent.
    for src in [
        "module m;\n  input clk;\nsyntaxerror\n  wire clk;\nendmodule\n",
        "module m; endmodule : n\n",
        "class c; task int t(); endtask endclass\n",
        "class c; covergroup g; x: cross a; endgroup endclass\n",
        "class d extends b; function new(); x = 1; super.new(); endfunction endclass\n",
    ] {
        let t = parse(src);
        assert_eq!(t.errors.len(), 1, "{src}: {:?}", t.errors);
        assert_eq!(t.file_text(0), src);
    }
    for src in [
        "`pragma\n",
        "`line 1 \"f\" 3\n",
        "`line -1 \"f\" 0\n",
        "`define define 1\n",
        "`timescale 9ns/1ps\n",
        "`timescale 1ns/10ns\n",
        "`unconnected_drive pull2\n",
        "module m;\n`resetall\nendmodule\n",
        "package p;\n`default_nettype none\nendpackage\n",
    ] {
        let t = parse(src);
        assert_eq!(t.pp_diagnostics.len(), 1, "{src}: {:?}", t.pp_diagnostics);
    }
    // And the legal forms stay quiet.
    for src in [
        "`timescale 100us / 1ps\n`pragma protect begin\n`line 3 \"x\" 0\n`resetall\nmodule m; endmodule\n`resetall\n",
        "extern module x();\n`resetall\ninterface class ic; endclass\n`default_nettype wire\n",
        "class d extends b; function new(); int x; super.new(); x = 1; endfunction endclass\n",
        "module m; initial begin : a end : a\nalias x = y;\nif (1) $error(\"e\");\nendmodule : m\n",
        "`celldefine module m; endmodule `endcelldefine\n",
    ] {
        let t = parse(src);
        assert!(t.errors.is_empty() && t.pp_diagnostics.is_empty(), "{src}: {:?} {:?}", t.errors, t.pp_diagnostics);
    }
}

/// Token ranges come from precomputed spans; they must equal a walk of the
/// node's children.
#[test]
fn token_ranges_match_a_walk() {
    fn walk(t: &Tree, id: sv_syntax::NodeId, out: &mut Vec<u32>) {
        for c in &t.node(id).children {
            match *c {
                sv_syntax::Child::Token(k) => out.push(k),
                sv_syntax::Child::Node(n) => walk(t, n, out),
            }
        }
    }
    let t = parse(
        "`define M(x) x + 1\nclass c extends b #(int); int q[$]; function int f(int a); return `M(a); endfunction\n\
         task t(); fork #1; join_none endtask constraint k { q.size() < 4; } endclass\nmodule m; wire w = (; endmodule\n",
    );
    for id in t.preorder() {
        let mut want = Vec::new();
        walk(&t, id, &mut want);
        assert_eq!(t.token_range(id), want);
        assert_eq!(t.token_bounds(id), want.first().map(|&f| (f, *want.last().unwrap())));
    }
}

#[test]
fn single_unit_reads_files_into_one_tree() {
    let dir = std::env::temp_dir().join(format!("sv_unit_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.sv");
    let b = dir.join("b.sv");
    std::fs::write(&a, "`define W 8\ntypedef logic [`W-1:0] byte_t;\n").unwrap();
    std::fs::write(&b, "module m; byte_t x; endmodule\n").unwrap();
    let t = sv_syntax::parse_unit(&[&a, &b], Default::default());
    assert!(t.errors.is_empty(), "{:?}", t.errors);
    assert_eq!(t.mains, vec![0, 1]);
    assert_eq!(t.files.len(), 2);
    assert!(t.tokens.iter().any(|k| k.origin.file == 1 && &*k.text == "byte_t"));
    std::fs::remove_dir_all(&dir).ok();
}
