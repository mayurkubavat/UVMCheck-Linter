use sv_semantic::expr::{ExprArena, ExprBinder, ExprKind};
use sv_semantic::{
    elaborate_with_params, BitState, CVal, CycleBound, CycleRange, PortDir, PropExpr, RtlType, SeqExpr, SvaBinder,
    SymbolKind,
};
use sv_syntax::{parse_source, Options, SyntaxKind};

#[test]
fn elaborated_design_per_instance_symbols_and_param_overrides() {
    let src = r#"
module leaf #(parameter int W = 4) (
    input  logic         clk,
    input  logic [W-1:0] d,
    output logic [W-1:0] q
);
    genvar i;
    for (i = 0; i < 2; i++) begin : g
        logic [W-1:0] stage;
    end
endmodule

module top #(parameter int TOP_W = 8) (
    input  logic             clk,
    output logic [TOP_W-1:0] out_a,
    output logic [3:0]       out_b
);
    leaf #(.W(TOP_W)) u0 (.clk(clk), .d('0), .q(out_a));
    leaf #(.W(4))     u1 (.clk,      .d('1), .q(out_b));
endmodule
"#;
    let tree = parse_source(src, "test.sv", Options::default());
    assert!(tree.errors.is_empty());
    let trees = [tree];
    let params = vec![("TOP_W".to_string(), "12".to_string())];
    let elab = elaborate_with_params(&trees, &["top".to_string()], &params);
    assert!(elab.diags.is_empty(), "diags: {:?}", elab.diags);

    let d = &elab.design;
    assert_eq!(d.tops.len(), 1);
    let u0 = d.instance_by_path("top.u0").expect("top.u0");
    let u1 = d.instance_by_path("top.u1").expect("top.u1");
    assert_ne!(u0, u1);

    let u0_q = d.symbol_by_path("top.u0.q").expect("top.u0.q");
    let u1_q = d.symbol_by_path("top.u1.q").expect("top.u1.q");
    assert_ne!(u0_q, u1_q, "two instances must own distinct SymbolIds");
    assert_eq!(d.symbol(u0_q).ty.bit_width(), Some(12));
    assert_eq!(d.symbol(u1_q).ty.bit_width(), Some(4));
    assert!(matches!(d.symbol(u0_q).kind, SymbolKind::Port { dir: PortDir::Output, is_net: false }));

    let g0_stage = d.symbol_by_path("top.u0.g[0].stage").expect("top.u0.g[0].stage");
    let g1_stage = d.symbol_by_path("top.u0.g[1].stage").expect("top.u0.g[1].stage");
    assert_ne!(g0_stage, g1_stage);
    assert_eq!(d.symbol(g0_stage).ty.bit_width(), Some(12));

    // ScopeNode browser tree includes generate scopes and child instances.
    let browser = d.scope_tree();
    assert_eq!(browser.len(), 1);
    assert_eq!(browser[0].path, "top");
    assert!(browser[0].children.iter().any(|c| c.path == "top.u0" && c.module == "leaf"));

    // Unknown `-G` parameter override must fail closed.
    let bad = elaborate_with_params(&trees, &["top".to_string()], &[("NO_SUCH_PARAM".into(), "1".into())]);
    assert!(bad.diags.iter().any(|dg| dg.message.contains("unknown parameter override")));
}

#[test]
fn expr_binder_inserts_explicit_conversions_and_preserves_four_state() {
    let src = r#"
module m (
    input  logic signed [3:0] a,
    input  logic        [7:0] b,
    output logic        [7:0] y_ext,
    output logic        [7:0] y_mix,
    output logic        [3:0] xz_const
);
    assign y_ext = a;
    assign y_mix = a + b;
    assign xz_const = 4'b10xz;
endmodule
"#;
    let tree = parse_source(src, "conv.sv", Options::default());
    let trees = [tree];
    let elab = elaborate_with_params(&trees, &[], &[]);
    assert!(elab.diags.is_empty());
    let top = elab.design.tops[0];
    let assigns: Vec<_> = elab
        .design
        .instance(top)
        .items
        .iter()
        .filter(|it| it.kind == sv_semantic::ElabItemKind::ContinuousAssign)
        .collect();
    assert_eq!(assigns.len(), 3);

    let mut arena = ExprArena::new();
    let mut binder = ExprBinder::new(&trees, &elab.design, top, "", &[], &mut arena);
    let u8_ty = RtlType::Integral { width: 8, signed: false, four_state: true };

    // 1. `assign y_ext = a;` -> `a` (signed 4b) sign-extends to 8b (`Resize { width: 8, signed: true }`), then `CastSign(false)`.
    let rhs0 = rhs_of_assign(&trees[0], assigns[0].node);
    let e0 = binder.bind_assigned(0, rhs0, &u8_ty).expect("bind y_ext");
    let ExprKind::CastSign { arg: r0, signed: false } = binder.arena.get(e0).kind else {
        panic!("expected outer CastSign(false), got {:?}", binder.arena.get(e0));
    };
    assert!(
        matches!(binder.arena.get(r0).kind, ExprKind::Resize { width: 8, signed: true, .. }),
        "expected signed Resize to 8 bits, got {:?}",
        binder.arena.get(r0)
    );

    // 2. `assign y_mix = a + b;` -> `b` is unsigned, so `a` converts `CastSign(false)` BEFORE zero-extending `Resize { width: 8, signed: false }`!
    let rhs1 = rhs_of_assign(&trees[0], assigns[1].node);
    let e1 = binder.bind_assigned(0, rhs1, &u8_ty).expect("bind y_mix");
    let ExprKind::Binary { lhs, .. } = binder.arena.get(e1).kind else {
        panic!("expected Binary Add, got {:?}", binder.arena.get(e1));
    };
    let ExprKind::Resize { arg: cast_a, width: 8, signed: false } = binder.arena.get(lhs).kind else {
        panic!("expected zero-extend Resize(8, signed=false) on `a`, got {:?}", binder.arena.get(lhs));
    };
    assert!(matches!(binder.arena.get(cast_a).kind, ExprKind::CastSign { signed: false, .. }));

    // 3. `4'b10xz` preserves exact X and Z bit states!
    let rhs2 = rhs_of_assign(&trees[0], assigns[2].node);
    let u4_ty = RtlType::Integral { width: 4, signed: false, four_state: true };
    let e2 = binder.bind_assigned(0, rhs2, &u4_ty).expect("bind xz");
    let cv = binder.arena.get(e2).cval.as_ref().expect("const 4'b10xz");
    assert_eq!(cv.bit_state(0), BitState::Z);
    assert_eq!(cv.bit_state(1), BitState::X);
    assert_eq!(cv.bit_state(2), BitState::Zero);
    assert_eq!(cv.bit_state(3), BitState::One);
    assert!(binder.diags.is_empty());
}

#[test]
fn expr_binder_fails_closed_on_unknown_ident_and_variable_wildcard_eq() {
    let src = r#"
module m (input logic [3:0] a, input logic [3:0] b, output logic y1, output logic y2);
    assign y1 = typo_sig;
    assign y2 = (a ==? b);
endmodule
"#;
    let tree = parse_source(src, "bad.sv", Options::default());
    let trees = [tree];
    let elab = elaborate_with_params(&trees, &[], &[]);
    let top = elab.design.tops[0];
    let assigns: Vec<_> = elab.design.instance(top).items.iter().collect();
    let mut arena = ExprArena::new();
    let mut binder = ExprBinder::new(&trees, &elab.design, top, "", &[], &mut arena);

    assert!(binder.bind_self(0, rhs_of_assign(&trees[0], assigns[0].node)).is_none());
    assert!(binder.diags.iter().any(|d| d.message.contains("undeclared identifier 'typo_sig'")));

    binder.diags.clear();
    assert!(binder.bind_self(0, rhs_of_assign(&trees[0], assigns[1].node)).is_none());
    assert!(binder.diags.iter().any(|d| d.message.contains("wildcard equality")));
}

#[test]
fn expr_binder_binds_lrm_ch11_operators() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("testdata/conformance/lrm/ch11_operators.sv");
    let src = std::fs::read_to_string(&path).expect("read ch11_operators.sv");
    let tree = parse_source(&src, &path.to_string_lossy(), Options::default());
    assert!(tree.errors.is_empty());
    let trees = [tree];
    let elab = elaborate_with_params(&trees, &[], &[]);
    assert!(!elab.design.tops.is_empty());
    let top = elab.design.tops[0];
    let mut arena = ExprArena::new();
    let mut binder = ExprBinder::new(&trees, &elab.design, top, "", &[], &mut arena);
    let mut bound_count = 0usize;
    for item in &elab.design.instance(top).items {
        if item.kind == sv_semantic::ElabItemKind::ContinuousAssign {
            let rhs = rhs_of_assign(&trees[item.tree], item.node);
            if binder.bind_self(item.tree, rhs).is_some() {
                bound_count += 1;
            }
        }
    }
    // Also bind initializers on declarations in `top`.
    for &sid in &elab.design.instance(top).symbols {
        let sym = elab.design.symbol(sid);
        if let Some((tr, init_n)) = sym.init_node {
            if binder.bind_assigned(tr, init_n, &sym.ty).is_some() {
                bound_count += 1;
            }
        }
    }
    assert!(bound_count > 0, "expected bound expressions in ch11_operators.sv");
    assert!(binder.diags.is_empty(), "unexpected binder diags on ch11_operators.sv: {:?}", binder.diags);
}

fn rhs_of_assign(t: &sv_syntax::Tree, ca: sv_syntax::NodeId) -> sv_syntax::NodeId {
    let be = t.child(ca, SyntaxKind::BinaryExpr).expect("BinaryExpr in ContinuousAssign");
    let kids: Vec<_> = t.children(be).filter(|&c| t.kind(c) != SyntaxKind::Attribute).collect();
    kids[1]
}

#[test]
fn sva_named_property_disable_iff_lifting_and_nested_rejection() {
    let src = r#"
module m (input logic clk, input logic rst, input logic rst2, input logic req, input logic ack, input logic en);
    property p_with_rst(a, b);
        disable iff (rst) a |-> ##1 b;
    endproperty

    ok_top: assert property (@(posedge clk) p_with_rst(req, ack));
endmodule
"#;
    let tree = parse_source(src, "sva_dis.sv", Options::default());
    assert!(tree.errors.is_empty());
    let trees = [tree];
    let elab = elaborate_with_params(&trees, &[], &[]);
    assert!(elab.diags.is_empty());
    let top = elab.design.tops[0];
    let mut arena = ExprArena::new();
    let mut sva = SvaBinder::new(&trees, &elab.design, &mut arena);
    let props = sva.bind_instance_properties(top);
    assert!(sva.diags.is_empty(), "unexpected diags: {:?}", sva.diags);
    assert_eq!(props.len(), 1);
    assert!(props[0].disable_iff.is_some(), "named property disable iff must lift to BoundProperty");
    assert!(matches!(props[0].expr, PropExpr::OverlappedImplication { .. }));

    for bad_assert in [
        "assert property (@(posedge clk) disable iff (rst2) p_with_rst(req, ack));",
        "assert property (@(posedge clk) not p_with_rst(req, ack));",
        "assert property (@(posedge clk) en |-> p_with_rst(req, ack));",
    ] {
        let bad_src = format!(
            "module m (input logic clk, rst, rst2, req, ack, en);\n\
             property p_with_rst(a, b); disable iff (rst) a |-> ##1 b; endproperty\n\
             {bad_assert}\n\
             endmodule\n"
        );
        let btree = parse_source(&bad_src, "bad_dis.sv", Options::default());
        assert!(btree.errors.is_empty());
        let btrees = [btree];
        let belab = elaborate_with_params(&btrees, &[], &[]);
        let btop = belab.design.tops[0];
        let mut barena = ExprArena::new();
        let mut bsva = SvaBinder::new(&btrees, &belab.design, &mut barena);
        let bprops = bsva.bind_instance_properties(btop);
        assert!(bprops.is_empty(), "nested disable iff should fail closed on: {bad_assert}");
        assert!(
            bsva.diags.iter().any(|d| d.message.contains("`disable iff` is only legal at the top level of a property")),
            "missing top-level disable iff diagnostic on {bad_assert}: {:?}",
            bsva.diags
        );
    }
}

#[test]
fn sva_unary_precedence_matches_ieee_1800_table_16_1() {
    let src = r#"
module m (input logic clk, input logic a, input logic b);
    p_not_and:     assert property (@(posedge clk) not a and b);
    p_not_implies: assert property (@(posedge clk) not a implies b);
    p_not_delay:   assert property (@(posedge clk) not a ##1 b);
    p_next_imp:    assert property (@(posedge clk) nexttime a implies b);
    p_alw_not_imp: assert property (@(posedge clk) always not a implies b);
endmodule
"#;
    let tree = parse_source(src, "sva_prec.sv", Options::default());
    assert!(tree.errors.is_empty());
    let trees = [tree];
    let elab = elaborate_with_params(&trees, &[], &[]);
    let top = elab.design.tops[0];
    let mut arena = ExprArena::new();
    let mut sva = SvaBinder::new(&trees, &elab.design, &mut arena);
    let props = sva.bind_instance_properties(top);
    assert!(sva.diags.is_empty(), "unexpected SVA diags: {:?}", sva.diags);
    assert_eq!(props.len(), 5);

    match &props[0].expr {
        PropExpr::And(lhs, rhs) => {
            assert!(matches!(lhs.as_ref(), PropExpr::Not(_)), "lhs of `not a and b` must be Not: {:?}", props[0].expr);
            assert!(matches!(rhs.as_ref(), PropExpr::Seq(SeqExpr::Bool(_))));
        }
        other => panic!("expected And(Not(a), b), got {other:?}"),
    }
    match &props[1].expr {
        PropExpr::Implies(lhs, rhs) => {
            assert!(matches!(lhs.as_ref(), PropExpr::Not(_)));
            assert!(matches!(rhs.as_ref(), PropExpr::Seq(SeqExpr::Bool(_))));
        }
        other => panic!("expected Implies(Not(a), b), got {other:?}"),
    }
    match &props[2].expr {
        PropExpr::Not(inner) => {
            assert!(
                matches!(
                    inner.as_ref(),
                    PropExpr::Seq(SeqExpr::Delay {
                        range: CycleRange { min: 1, max: CycleBound::Const(1) },
                        lhs: Some(_),
                        ..
                    })
                ),
                "expected Not(a ##1 b), got {inner:?}"
            );
        }
        other => panic!("expected Not(a ##1 b), got {other:?}"),
    }
    match &props[3].expr {
        PropExpr::Implies(lhs, rhs) => {
            assert!(matches!(lhs.as_ref(), PropExpr::Nexttime { strong: false, cycles: 1, .. }));
            assert!(matches!(rhs.as_ref(), PropExpr::Seq(SeqExpr::Bool(_))));
        }
        other => panic!("expected Implies(Nexttime(a), b), got {other:?}"),
    }
    match &props[4].expr {
        PropExpr::Always { prop, .. } => match prop.as_ref() {
            PropExpr::Implies(lhs, _) => assert!(matches!(lhs.as_ref(), PropExpr::Not(_))),
            other => panic!("expected Always(Implies(Not(a), b)), got {other:?}"),
        },
        other => panic!("expected outer Always, got {other:?}"),
    }
}

#[test]
fn hierarchical_cli_param_override_typo_and_empty_tops_fail_closed() {
    let src = r#"
module sub #(parameter int W = 4) (output logic [W-1:0] q);
    assign q = '0;
endmodule

module top (output logic [15:0] qa, output logic [3:0] qb);
    sub u0 (.q(qa));
    sub u1 (.q(qb));
endmodule
"#;
    let tree = parse_source(src, "hier_param.sv", Options::default());
    let trees = [tree];

    let ok = elaborate_with_params(&trees, &["top".into()], &[("top.u0.W".into(), "16".into())]);
    assert!(ok.diags.is_empty(), "valid hierarchical override failed: {:?}", ok.diags);
    let u0_q = ok.design.symbol_by_path("top.u0.q").expect("top.u0.q");
    let u1_q = ok.design.symbol_by_path("top.u1.q").expect("top.u1.q");
    assert_eq!(ok.design.symbol(u0_q).ty.bit_width(), Some(16));
    assert_eq!(ok.design.symbol(u1_q).ty.bit_width(), Some(4));

    let bad_inst = elaborate_with_params(&trees, &["top".into()], &[("top.bad_inst.W".into(), "16".into())]);
    assert!(
        bad_inst.diags.iter().any(|d| d.message.contains("top.bad_inst.W")),
        "expected diagnostic on hierarchical instance typo, got {:?}",
        bad_inst.diags
    );

    let bad_leaf = elaborate_with_params(&trees, &["top".into()], &[("top.u0.NO_SUCH".into(), "16".into())]);
    assert!(
        bad_leaf.diags.iter().any(|d| d.message.contains("top.u0.NO_SUCH")),
        "expected diagnostic on hierarchical leaf parameter typo, got {:?}",
        bad_leaf.diags
    );

    let pkg_only = parse_source("package p; localparam int X = 1; endpackage\n", "pkg.sv", Options::default());
    let empty_tops = elaborate_with_params(&[pkg_only], &[], &[("BOGUS".into(), "1".into())]);
    assert!(
        empty_tops.diags.iter().any(|d| d.message.contains("BOGUS")),
        "expected unmatched -G diagnostic even when tops is empty, got {:?}",
        empty_tops.diags
    );
}

#[test]
fn zero_replication_and_empty_concat_follow_ieee_1800_11_4_12_1() {
    let src = r#"
module m (input logic a, input logic b);
    wire [3:0] w_ok     = {4'b1010, {0{1'b1}}};
    wire [3:0] w_undecl = {4'b1010, {0{missing_sig}}};
    wire       w_zero   = {0{a}};
    wire       w_all0   = {{0{a}}, {0{b}}};
    wire       w_empty  = {};
endmodule
"#;
    let tree = parse_source(src, "zero_rep.sv", Options::default());
    let trees = [tree];
    let elab = elaborate_with_params(&trees, &[], &[]);
    let top = elab.design.tops[0];
    let mut arena = ExprArena::new();
    let mut binder = ExprBinder::new(&trees, &elab.design, top, "", &[], &mut arena);

    let init_of = |name: &str| {
        let sid = elab.design.symbol_by_path(&format!("m.{name}")).unwrap();
        elab.design.symbol(sid).init_node.unwrap().1
    };

    let ok_id = binder.bind_self(0, init_of("w_ok")).expect("{4'b1010, {0{1'b1}}} must bind");
    assert!(binder.diags.is_empty(), "unexpected diags on valid zero-rep concat: {:?}", binder.diags);
    assert_eq!(binder.arena.get(ok_id).ty.bit_width(), Some(4));
    assert_eq!(binder.arena.get(ok_id).cval.as_ref().map(CVal::i), Some(0b1010));

    binder.diags.clear();
    assert!(binder.bind_self(0, init_of("w_undecl")).is_none());
    assert!(binder.diags.iter().any(|d| d.message.contains("undeclared identifier 'missing_sig'")));

    for bad_sym in ["w_zero", "w_all0", "w_empty"] {
        binder.diags.clear();
        assert!(binder.bind_self(0, init_of(bad_sym)).is_none(), "{bad_sym} must be rejected");
        assert!(!binder.diags.is_empty(), "{bad_sym} must record an ExprDiag");
    }
}

#[test]
fn package_and_unit_let_sequence_and_property_resolution() {
    let src = r#"
let unit_is_zero(x) = (x == '0);
property unit_next(a, b);
    a |=> b;
endproperty

package assertion_pkg;
    localparam int DELAY = 2;
    let is_max(x) = (x == 8'hff);
    sequence s_ pipe(req, ack);
        req ##DELAY ack;
    endsequence
endpackage
"#;
    let clean_src = src.replace("s_ pipe", "s_pipe");
    let full_src = format!(
        "{clean_src}\n\
         package prop_pkg;\n\
             import assertion_pkg::*;\n\
             property p_handshake(r, g);\n\
                 s_pipe(r, g);\n\
             endproperty\n\
         endpackage\n\
         module top_imp (input logic clk, input logic req, input logic ack, input logic [7:0] v, output logic y, output logic z);\n\
             import assertion_pkg::*;\n\
             import prop_pkg::*;\n\
             assign y = is_max(v);\n\
             assign z = unit_is_zero(v);\n\
             a1: assert property (@(posedge clk) p_handshake(req, ack));\n\
             a2: assert property (@(posedge clk) unit_next(req, ack));\n\
         endmodule\n\
         module top_qual (input logic clk, input logic req, input logic ack, input logic [7:0] v, output logic y);\n\
             assign y = assertion_pkg::is_max(v);\n\
             a1: assert property (@(posedge clk) prop_pkg::p_handshake(req, ack));\n\
         endmodule\n"
    );
    let tree = parse_source(&full_src, "pkg_sva.sv", Options::default());
    assert!(tree.errors.is_empty(), "parse errors: {:?}", tree.errors);
    let trees = [tree];
    let elab = elaborate_with_params(&trees, &["top_imp".into(), "top_qual".into()], &[]);
    assert!(elab.diags.is_empty(), "elab diags: {:?}", elab.diags);

    for top_name in ["top_imp", "top_qual"] {
        let inst = elab.design.instance_by_path(top_name).unwrap();
        let mut arena = ExprArena::new();
        let mut binder = ExprBinder::new(&trees, &elab.design, inst, "", &[], &mut arena);
        for item in &elab.design.instance(inst).items {
            if item.kind == sv_semantic::ElabItemKind::ContinuousAssign {
                let rhs = rhs_of_assign(&trees[item.tree], item.node);
                assert!(binder.bind_self(item.tree, rhs).is_some(), "failed assign in {top_name}: {:?}", binder.diags);
            }
        }
        assert!(binder.diags.is_empty(), "expr diags in {top_name}: {:?}", binder.diags);

        let mut sva = SvaBinder::new(&trees, &elab.design, &mut arena);
        let props = sva.bind_instance_properties(inst);
        assert!(sva.diags.is_empty(), "sva diags in {top_name}: {:?}", sva.diags);
        let expected = if top_name == "top_imp" { 2 } else { 1 };
        assert_eq!(props.len(), expected);
        assert!(matches!(
            props[0].expr,
            PropExpr::Seq(SeqExpr::Delay { range: CycleRange { min: 2, max: CycleBound::Const(2) }, .. })
        ));
    }
}

#[test]
fn let_and_sva_unbased_unsized_actuals_expand_to_context_width() {
    let src = r#"
module m (input logic clk, output logic y_ones, output logic y_zeros);
    let is_all_ones(x) = (x == 8'hff);
    assign y_ones  = is_all_ones('1);
    assign y_zeros = is_all_ones('0);

    property p_match_ones(x);
        @(posedge clk) (x == 8'hff);
    endproperty
    ap: assert property (p_match_ones('1));
endmodule
"#;
    let tree = parse_source(src, "let_unbased.sv", Options::default());
    assert!(tree.errors.is_empty());
    let trees = [tree];
    let elab = elaborate_with_params(&trees, &[], &[]);
    let top = elab.design.tops[0];
    let assigns: Vec<_> = elab
        .design
        .instance(top)
        .items
        .iter()
        .filter(|it| it.kind == sv_semantic::ElabItemKind::ContinuousAssign)
        .collect();

    let mut arena = ExprArena::new();
    let mut binder = ExprBinder::new(&trees, &elab.design, top, "", &[], &mut arena);
    let u1_ty = RtlType::Integral { width: 1, signed: false, four_state: true };
    let e_ones = binder.bind_assigned(0, rhs_of_assign(&trees[0], assigns[0].node), &u1_ty).unwrap();
    let e_zeros = binder.bind_assigned(0, rhs_of_assign(&trees[0], assigns[1].node), &u1_ty).unwrap();
    assert!(binder.diags.is_empty());
    assert_eq!(binder.arena.get(e_ones).cval.as_ref().map(CVal::i), Some(1));
    assert_eq!(binder.arena.get(e_zeros).cval.as_ref().map(CVal::i), Some(0));

    let mut sva = SvaBinder::new(&trees, &elab.design, &mut arena);
    let props = sva.bind_instance_properties(top);
    assert!(sva.diags.is_empty(), "sva diags: {:?}", sva.diags);
    assert_eq!(props.len(), 1);
    let PropExpr::Seq(SeqExpr::Bool(cond_id)) = props[0].expr else {
        panic!("expected Bool leaf in p_match_ones('1), got {:?}", props[0].expr);
    };
    assert_eq!(sva.arena.get(cond_id).cval.as_ref().map(CVal::i), Some(1));
}

#[test]
fn cval_implication_equivalence_unsigned_shifts_and_signed_power() {
    let src = r#"
module m;
    wire c_imp_10  = (1'b1 -> 1'b0);
    wire c_imp_0x  = (1'b0 -> 1'bx);
    wire c_imp_1x  = (1'b1 -> 1'bx);
    wire c_iff_11  = (1'b1 <-> 1'b1);
    wire c_iff_10  = (1'b1 <-> 1'b0);
    wire c_iff_x1  = (1'bx <-> 1'b1);
    wire [31:0] sh_neg_rhs = (32'h1 << 4'sb1111);
    wire [31:0] sh_huge_u  = (32'h1 << 128'h8000_0000_0000_0000_0000_0000_0000_0000);
    wire signed [31:0] pw_pos_neg = (32'sd2 ** -32'sd3);
    wire signed [31:0] pw_one_neg = (32'sd1 ** -32'sd5);
    wire signed [31:0] pw_m1_odd  = (-32'sd1 ** -32'sd3);
    wire signed [31:0] pw_m1_even = (-32'sd1 ** -32'sd2);
    wire signed [31:0] pw_z_neg   = (32'sd0 ** -32'sd1);
endmodule
"#;
    let tree = parse_source(src, "cval_ops.sv", Options::default());
    assert!(tree.errors.is_empty());
    let trees = [tree];
    let elab = elaborate_with_params(&trees, &[], &[]);
    let top = elab.design.tops[0];
    let mut arena = ExprArena::new();
    let mut binder = ExprBinder::new(&trees, &elab.design, top, "", &[], &mut arena);

    let eval_sym = |binder: &mut ExprBinder<'_>, name: &str| -> CVal {
        let sid = elab.design.symbol_by_path(&format!("m.{name}")).unwrap();
        let sym = elab.design.symbol(sid);
        let (_, init_n) = sym.init_node.unwrap();
        let eid = binder.bind_assigned(0, init_n, &sym.ty).unwrap_or_else(|| panic!("bind {name} failed"));
        binder.arena.get(eid).cval.clone().unwrap_or_else(|| panic!("{name} did not constant-fold"))
    };

    assert_eq!(eval_sym(&mut binder, "c_imp_10").i(), 0);
    assert_eq!(eval_sym(&mut binder, "c_imp_0x").i(), 1);
    assert_eq!(eval_sym(&mut binder, "c_imp_1x").bit_state(0), BitState::X);
    assert_eq!(eval_sym(&mut binder, "c_iff_11").i(), 1);
    assert_eq!(eval_sym(&mut binder, "c_iff_10").i(), 0);
    assert_eq!(eval_sym(&mut binder, "c_iff_x1").bit_state(0), BitState::X);
    assert_eq!(eval_sym(&mut binder, "sh_neg_rhs").i(), 0x8000);
    assert_eq!(eval_sym(&mut binder, "sh_huge_u").i(), 0);
    assert_eq!(eval_sym(&mut binder, "pw_pos_neg").i(), 0);
    assert_eq!(eval_sym(&mut binder, "pw_one_neg").i(), 1);
    assert_eq!(eval_sym(&mut binder, "pw_m1_odd").i(), -1);
    assert_eq!(eval_sym(&mut binder, "pw_m1_even").i(), 1);
    assert!(!eval_sym(&mut binder, "pw_z_neg").known(), "0 ** -1 must evaluate to unknown x");
    assert!(binder.diags.is_empty(), "unexpected diags: {:?}", binder.diags);
}

#[test]
fn scope_tree_ascending_generate_order_and_assertion_only_generate() {
    let src = r#"
module top (input logic clk, input logic a);
    for (genvar i = 0; i < 11; i++) begin : g
        logic sig;
        if (1) begin : sub
            logic inner_sig;
        end
    end
    if (1) begin : chk_only
        ap: assert property (@(posedge clk) a);
    end
endmodule
"#;
    let tree = parse_source(src, "scope_tree.sv", Options::default());
    assert!(tree.errors.is_empty());
    let trees = [tree];
    let elab = elaborate_with_params(&trees, &[], &[]);
    assert!(elab.diags.is_empty());

    let browser = elab.design.scope_tree();
    assert_eq!(browser.len(), 1);
    let child_paths: Vec<&str> = browser[0].children.iter().map(|c| c.path.as_str()).collect();
    let mut expected = vec!["top.chk_only".to_string()];
    for i in 0..11 {
        expected.push(format!("top.g[{i}]"));
    }
    let expected_refs: Vec<&str> = expected.iter().map(String::as_str).collect();
    assert_eq!(child_paths, expected_refs);
    assert_eq!(browser[0].children[1].children.len(), 1);
    assert_eq!(browser[0].children[1].children[0].path, "top.g[0].sub");
}

#[test]
fn sva_and_expr_fail_closed_guards() {
    let sva_bad_cases: &[(&str, &str)] = &[
        (
            "module m(input logic clk, input logic [3:0] x); assert property (@(posedge clk) x dist { 1 := 2, 2 := 3 }); endmodule",
            "dist expressions in assertions are unsupported",
        ),
        (
            "module m(input logic clk, a, b); logic v; assert property (@(posedge clk) first_match(a ##1 b, v = 1'b1)); endmodule",
            "first_match",
        ),
        (
            "module m(input logic clk, a, b); sequence s; a |-> b; endsequence assert property (@(posedge clk) s); endmodule",
            "property operator is illegal inside a sequence declaration",
        ),
        (
            "module m(input logic clk, a); assert property (@(posedge clk) nexttime [1:2] a); endmodule",
            "single constant cycle count",
        ),
        (
            "module m(input logic a); clocking cb @(cb); endclocking assert property (@(cb) a); endmodule",
            "clocking block",
        ),
    ];
    for &(src, needle) in sva_bad_cases {
        let tree = parse_source(src, "sva_guard.sv", Options::default());
        assert!(tree.errors.is_empty(), "parse error on {src}: {:?}", tree.errors);
        let trees = [tree];
        let elab = elaborate_with_params(&trees, &[], &[]);
        let top = elab.design.tops[0];
        let mut arena = ExprArena::new();
        let mut sva = SvaBinder::new(&trees, &elab.design, &mut arena);
        let props = sva.bind_instance_properties(top);
        assert!(props.is_empty(), "expected SVA failure on: {src}");
        assert!(
            sva.diags.iter().any(|d| d.message.contains(needle)),
            "expected '{needle}' in SVA diags {:?} for {src}",
            sva.diags
        );
    }

    let expr_src = r#"
module m (input logic cond, input logic [7:0] x);
    typedef struct packed {
        logic [3:0] a;
        logic [3:0] b;
    } pkt_t;
    function automatic logic [3:0] sub4(input logic [3:0] lhs, input logic [3:0] rhs);
        return lhs - rhs;
    endfunction
    logic [7:0] mem [0:3];

    wire [3:0] pat_member = (pkt_t'{a: 4'ha, b: 4'h5}).a;
    wire [3:0] named_ok   = sub4(.rhs(4'd3), .lhs(4'd10));
    wire [3:0] bad_empty  = sub4(, 4'd1);
    wire [3:0] bad_excess = sub4(4'd1, 4'd2, 4'd3);
    wire [3:0] bad_named  = sub4(.bogus(4'd1));
    wire [31:0] bad_dim0  = $size(mem, 0);
    wire [31:0] bad_dim9  = $size(mem, 9);
    wire [31:0] bad_dim_x = $size(mem, 1, 2);
    wire [7:0]  bad_cond  = cond ? mem : 8'h00;
endmodule
"#;
    let tree = parse_source(expr_src, "expr_guard.sv", Options::default());
    assert!(tree.errors.is_empty(), "parse errors: {:?}", tree.errors);
    let trees = [tree];
    let elab = elaborate_with_params(&trees, &[], &[]);
    let top = elab.design.tops[0];
    let mut arena = ExprArena::new();
    let mut binder = ExprBinder::new(&trees, &elab.design, top, "", &[], &mut arena);

    let sym_init = |name: &str| {
        let sid = elab.design.symbol_by_path(&format!("m.{name}")).unwrap();
        let sym = elab.design.symbol(sid);
        (sym.init_node.unwrap().1, sym.ty.clone())
    };

    let (pat_n, pat_ty) = sym_init("pat_member");
    let pat_id = binder.bind_assigned(0, pat_n, &pat_ty).expect("typed pattern member select must bind");
    assert_eq!(binder.arena.get(pat_id).cval.as_ref().map(CVal::i), Some(0xa));
    assert!(binder.diags.is_empty());

    let (nm_n, nm_ty) = sym_init("named_ok");
    let nm_id = binder.bind_assigned(0, nm_n, &nm_ty).expect("named function call must bind");
    let ExprKind::FuncCall { args, .. } = &binder.arena.get(nm_id).kind else {
        panic!("expected FuncCall on named_ok");
    };
    assert_eq!(args.len(), 2);
    assert_eq!(binder.arena.get(args[0]).cval.as_ref().map(CVal::i), Some(10));
    assert_eq!(binder.arena.get(args[1]).cval.as_ref().map(CVal::i), Some(3));
    assert!(binder.diags.is_empty());

    for bad_name in ["bad_empty", "bad_excess", "bad_named", "bad_dim0", "bad_dim9", "bad_dim_x", "bad_cond"] {
        binder.diags.clear();
        let (bn, bty) = sym_init(bad_name);
        assert!(binder.bind_assigned(0, bn, &bty).is_none(), "{bad_name} must fail closed");
        assert!(!binder.diags.is_empty(), "{bad_name} must emit at least one ExprDiag");
    }

    binder.diags.clear();
    let mem_sid = elab.design.symbol_by_path("m.mem").unwrap();
    let mem_ty = elab.design.symbol(mem_sid).ty.clone();
    let scalar_lit = sym_init("pat_member").0;
    assert!(binder.bind_assigned(0, scalar_lit, &mem_ty).is_none(), "scalar assigned to unpacked array must fail");
    assert!(binder.diags.iter().any(|d| d.message.contains("incompatible types in unpacked aggregate assignment")));
}
