//! End-to-end API tests for `svlang::{Driver, Compilation}` RTL lowering,
//! `ScopeNode` design browser tree, `-G` parameter overrides, deterministic
//! `Netlist::dump`, and external CLI SVA binding (`Compilation::bind_property_in_scope`).

use std::path::Path;

use svlang::{Compilation, Driver, ExprArena, LowerConfig, LowerOutcome, PortDir, PropExpr, PropKind, SvaEdge};

#[test]
fn driver_param_override_and_deterministic_netlist_dump() {
    let fifo_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/netlist/fifo.sv");
    let comp = Driver::new().add_file(&fifo_path).param("WIDTH", "12").param("DEPTH", "8").compile().unwrap();
    assert!(comp.diagnostics().is_empty(), "diags: {:?}", comp.diagnostics());

    let nl1 = match comp.lower_netlist(&LowerConfig::default()) {
        LowerOutcome::Lowered(nl) => nl,
        other => panic!("expected Lowered, got {other:?}"),
    };
    assert_eq!(nl1.top, "sync_fifo");
    assert_eq!(nl1.memories.len(), 1);
    assert_eq!(nl1.memories[0].width, 12);
    assert_eq!(nl1.memories[0].depth, 8);

    // Verify byte-for-byte determinism across independent compilations and dumps!
    let comp2 = Driver::new().add_file(&fifo_path).param("WIDTH", "12").param("DEPTH", "8").compile().unwrap();
    let nl2 = match comp2.lower_netlist(&LowerConfig::default()) {
        LowerOutcome::Lowered(nl) => nl,
        other => panic!("expected Lowered, got {other:?}"),
    };
    let dump1 = nl1.dump();
    let dump2 = nl2.dump();
    assert_eq!(dump1, dump2, "Netlist::dump() must be deterministic across runs");
    assert!(dump1.starts_with("netlist sync_fifo\n"));
    assert!(dump1.contains("memory m0 sync_fifo.mem : u12.4 depth 8 [0..=7]"), "dump:\n{dump1}");
}

#[test]
fn elaborated_design_scope_browser_tree() {
    let src = r#"
module leaf #(parameter int W = 4) (
    input  logic         clk,
    input  logic [W-1:0] d,
    output logic [W-1:0] q
);
    logic [W-1:0] stage;
    always_ff @(posedge clk) begin
        stage <= d;
        q     <= stage;
    end
endmodule

module top_hier (
    input  logic       clk,
    input  logic [7:0] din,
    output logic [7:0] dout
);
    for (genvar g = 0; g < 2; g++) begin : lane
        leaf #(.W(4)) u_leaf (
            .clk (clk),
            .d   (din[g*4 +: 4]),
            .q   (dout[g*4 +: 4])
        );
    end
endmodule
"#;
    let comp = Compilation::from_source("hier.sv", src);
    assert!(comp.diagnostics().is_empty());
    let elab = comp.elaborate();
    let roots = elab.design.scope_tree();
    assert_eq!(roots.len(), 1);
    let top = &roots[0];
    assert_eq!(top.name, "top_hier");
    assert_eq!(top.module, "top_hier");
    assert_eq!(top.path, "top_hier");
    assert!(top.signals.iter().any(|s| s.name == "din" && s.width == 8 && s.dir == Some(PortDir::Input)));
    assert!(top.signals.iter().any(|s| s.name == "dout" && s.width == 8 && s.dir == Some(PortDir::Output)));

    // Two distinct elaborated `leaf` instances under `lane[0]` and `lane[1]`
    assert_eq!(elab.design.instances.len(), 3);
    let leaf_paths: Vec<&str> =
        elab.design.instances.iter().filter(|i| i.definition == "leaf").map(|i| i.path.as_str()).collect();
    assert_eq!(leaf_paths, vec!["top_hier.lane[0].u_leaf", "top_hier.lane[1].u_leaf"]);
}

#[test]
fn bind_external_cli_property_in_scope_and_fail_closed_on_typo() {
    let fifo_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/netlist/fifo.sv");
    let comp = Driver::new().add_file(&fifo_path).compile().unwrap();
    let elab = comp.elaborate();
    let mut arena = ExprArena::new();

    // 1. Bare property spec bound inside `sync_fifo` (resolving internal signal `cnt` and `let do_push`)
    let bp = comp
        .bind_property_in_scope(
            &elab.design,
            "sync_fifo",
            "@(posedge clk) disable iff (rst) do_push |=> (cnt > 0)",
            &mut arena,
        )
        .expect("valid external property should bind cleanly");
    assert_eq!(bp.kind, PropKind::Assert);
    assert_eq!(bp.clock.as_ref().map(|c| c.edge), Some(SvaEdge::Posedge));
    assert!(bp.disable_iff.is_some());
    assert!(matches!(bp.expr, PropExpr::NonOverlappedImplication { .. }));

    // 2. Full labeled `cover property (...)` directive
    let cov = comp
        .bind_property_in_scope(
            &elab.design,
            "sync_fifo",
            "c_drain: cover property (@(posedge clk) full ##[1:8] empty);",
            &mut arena,
        )
        .expect("labeled cover directive should bind cleanly");
    assert_eq!(cov.kind, PropKind::Cover);
    assert_eq!(cov.label.as_deref(), Some("c_drain"));

    // 3. Undeclared signal in CLI property MUST fail closed (`Err`), never silently coerce to 0!
    let err = comp
        .bind_property_in_scope(&elab.design, "sync_fifo", "@(posedge clk) non_existent_sig == 1'b1", &mut arena)
        .expect_err("undeclared identifier in external property must fail closed");
    assert!(
        err.iter().any(|d| d.message.contains("undeclared") && d.message.contains("non_existent_sig")),
        "got {err:?}"
    );

    // 4. Unknown hierarchical scope path -> `Err`
    assert!(comp
        .bind_property_in_scope(&elab.design, "sync_fifo.no_such_inst", "@(posedge clk) full", &mut arena)
        .is_err());
}

#[test]
fn prop_expr_value_symbol_value_and_lower_external_property() {
    let fifo_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/netlist/fifo.sv");
    let comp = Driver::new().add_file(&fifo_path).compile().unwrap();
    let elab = comp.elaborate();
    let mut nl = comp.lower_netlist(&LowerConfig::default()).unwrap_netlist();

    // Every elaborated scalar/packed symbol in `sync_fifo` resolves via `nl.symbol_value`.
    let top_inst = &elab.design.instances[0];
    let cnt_sym =
        top_inst.symbols.iter().copied().find(|&s| elab.design.symbol(s).name == "count").expect("count symbol");
    let cnt_vid = nl.symbol_value(cnt_sym).expect("count ValueId");
    assert_eq!(nl.value(cnt_vid).width, elab.design.symbol(cnt_sym).ty.bit_width().unwrap());

    // Embedded `p_full_count` SVA property expressions resolve via `nl.prop_expr_value`.
    assert_eq!(nl.properties.len(), 1);
    let clk_eid = nl.properties[0].clock.as_ref().unwrap().signal;
    assert!(nl.prop_expr_value(clk_eid).is_some());

    // External `$isunknown` property fails closed without mutating `nl` when `allow_four_state_ops = false`.
    let snap_before = nl.dump();
    let mut bad_arena = ExprArena::new();
    let unk_prop = comp
        .bind_property_in_scope(&elab.design, "sync_fifo", "@(posedge clk) !$isunknown(count)", &mut bad_arena)
        .unwrap();
    let errs = nl.lower_external_property(&elab.design, &bad_arena, &unk_prop, false).unwrap_err();
    assert!(errs.iter().any(|d| d.unsupported && d.message.contains("$isunknown")), "errs: {errs:?}");
    assert_eq!(nl.dump(), snap_before, "failed lower_external_property must not mutate Netlist");

    // Valid external property lowers cleanly and maintains topological order.
    let mut ext_arena = ExprArena::new();
    let ext_prop = comp
        .bind_property_in_scope(
            &elab.design,
            "sync_fifo",
            "@(posedge clk) disable iff (rst) do_push |=> (count > 0)",
            &mut ext_arena,
        )
        .unwrap();
    let pairs = nl.lower_external_property(&elab.design, &ext_arena, &ext_prop, false).unwrap();
    assert!(!pairs.is_empty());
    for (_, vid) in &pairs {
        assert!((vid.0 as usize) < nl.values.len());
    }
    for (idx, node) in nl.values.iter().enumerate() {
        if let svlang::netlist::ValueKind::Op { args, .. } = &node.kind {
            for a in args {
                assert!((a.0 as usize) < idx, "topological order broken after lower_external_property");
            }
        }
    }
}

#[test]
fn bind_property_in_scope_filename_column_and_gen_consts_shadowing() {
    let src = r#"
module shadow_top #(parameter int K = 3) (input logic clk, output logic [7:0] out_v);
    if (1) begin : g
        localparam int K = 7;
        logic [7:0] sig;
        assign sig = 8'(K);
    end
    for (genvar i = 0; i < 2; i++) begin : outer
        for (genvar i = 10; i < 11; i++) begin : inner
            logic [7:0] lane_val;
            assign lane_val = 8'(i);
        end
    end
    assign out_v = g.sig + outer[0].inner[10].lane_val;
endmodule
"#;
    let comp = Compilation::from_source("shadow_top.sv", src);
    let elab = comp.elaborate();
    let mut arena = ExprArena::new();

    // 1. Typo in `<property>` reports `file == "<property>"` (NOT `"shadow_top.sv"`!) and exact line-1 column!
    let prop_text = "@(posedge clk) missing_id == 1'b1";
    let err = comp.bind_property_in_scope(&elab.design, "shadow_top", prop_text, &mut arena).unwrap_err();
    assert_eq!(err[0].file, "<property>");
    assert_eq!(err[0].line, 1);
    assert_eq!(err[0].column, 16, "missing_id starts at 1-based column 16 in {prop_text:?}");

    // 2. Localparam `K = 7` inside `shadow_top.g` shadows module `parameter K = 3`.
    let bp_k =
        comp.bind_property_in_scope(&elab.design, "shadow_top.g", "@(posedge clk) sig == K", &mut arena).unwrap();
    let PropExpr::Seq(svlang::SeqExpr::Bool(eq_k)) = bp_k.expr else { panic!("expected Bool") };
    let svlang::ExprKind::Binary { rhs: rhs_k, .. } = arena.get(eq_k).kind else { panic!("expected Binary") };
    assert_eq!(arena.get(rhs_k).cval.as_ref().map(|c| c.i()), Some(7));

    // 3. Nested `genvar i = 10` inside `shadow_top.outer[0].inner[10]` shadows outer `genvar i = 0`.
    let bp_i = comp
        .bind_property_in_scope(
            &elab.design,
            "shadow_top.outer[0].inner[10]",
            "@(posedge clk) lane_val == i",
            &mut arena,
        )
        .unwrap();
    let PropExpr::Seq(svlang::SeqExpr::Bool(eq_i)) = bp_i.expr else { panic!("expected Bool") };
    let svlang::ExprKind::Binary { rhs: rhs_i, .. } = arena.get(eq_i).kind else { panic!("expected Binary") };
    assert_eq!(arena.get(rhs_i).cval.as_ref().map(|c| c.i()), Some(10));
}

#[test]
fn driver_add_args_errors_warning_display_and_fail_closed_edges() {
    let mut drv = Driver::new();
    let err = drv.add_args(["+incdir+/definitely/missing/svlang_dir_xyz"]).unwrap_err();
    assert!(err.0.contains("include directory not found"), "got {err:?}");

    let warn_diag = svlang::Diagnostic {
        file: "top.f".into(),
        line: 4,
        column: 1,
        stage: svlang::DiagnosticStage::FileList,
        severity: svlang::Severity::Warning,
        message: "unused option".into(),
    };
    assert_eq!(warn_diag.to_string(), "top.f:4:1: warning: unused option");

    // Ambiguous multiple top modules without `LowerConfig::top` -> `LowerOutcome::Error`
    let multi_top = Compilation::from_source("two.sv", "module a; endmodule\nmodule b; endmodule\n");
    assert!(matches!(multi_top.lower_netlist(&LowerConfig::default()), LowerOutcome::Error(_)));

    // Multi-output `buf` lowers cleanly; switch-level `tran` fails closed as `Unsupported`.
    let gates = Compilation::from_source(
        "gates.sv",
        "module g (input wire a, output wire y1, output wire y2); buf u_b(y1, y2, a); endmodule",
    );
    assert!(matches!(gates.lower_netlist(&LowerConfig::default()), LowerOutcome::Lowered(_)));

    let sw_gate =
        Compilation::from_source("sw.sv", "module sw (inout wire a, inout wire b); tran u_t(a, b); endmodule");
    assert!(matches!(sw_gate.lower_netlist(&LowerConfig::default()), LowerOutcome::Unsupported(_)));

    // Mixing `=` and `<=` to the same signal in one clocked block -> `LowerOutcome::Error`.
    let mixed = Compilation::from_source(
        "mix.sv",
        "module mix (input logic clk, d, output logic q);\n\
           always_ff @(posedge clk) begin q = d; q <= ~q; end\n\
         endmodule",
    );
    assert!(matches!(mixed.lower_netlist(&LowerConfig::default()), LowerOutcome::Error(_)));
}
