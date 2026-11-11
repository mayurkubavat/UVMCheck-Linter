//! Integration tests verifying all 18 known-answer RTL lowering designs across
//! sequential clocks, async resets, signed context rules, 4-state guards, and drivers.

mod sim;

use sim::{lower_src, lower_testdata, NetlistSim};
use sv_netlist::{CheckKind, Edge, LowerConfig, LowerOutcome, NetOp, ValueKind};
use sv_semantic::{BitState, CVal};

#[test]
fn s7_01_async_reset_metadata_and_sim() {
    let nl = lower_testdata("async_reset.sv", &[], &LowerConfig::default()).unwrap_netlist();
    assert_eq!(nl.states.len(), 1);
    let st = &nl.states[0];
    assert_eq!(st.width, 4);
    assert_eq!(st.clock.unwrap().edge, Edge::Posedge);
    let ar = st.async_reset.expect("async_reset must be recorded, not rewritten to sync");
    assert!(!ar.active_high, "negedge rst_n / if (!rst_n) must record active_high = false");
    // Verify `st.next` is the pure synchronous `d + 1` (does NOT contain `rst_n`).
    if let ValueKind::Op { op, .. } = &nl.value(st.next).kind {
        assert_eq!(*op, NetOp::Add);
    } else {
        panic!("expected pure synchronous Add on st.next, got {:?}", nl.value(st.next).kind);
    }

    let mut sim = NetlistSim::new(&nl);
    sim.set_u("rst_n", 0);
    sim.set_u("d", 7);
    assert_eq!(sim.out_i("q"), 0, "async reset holds q = 0 immediately without clock");
    sim.set_u("rst_n", 1);
    sim.tick_posedge("clk");
    assert_eq!(sim.out_i("q"), 8, "first posedge outside reset loads d + 1");
}

#[test]
fn s7_02_child_clock_driven_by_parent_divider() {
    let nl = lower_testdata("child_clock.sv", &[], &LowerConfig::default()).unwrap_netlist();
    assert_eq!(nl.states.len(), 2);
    let div_st = nl.states.iter().find(|s| s.name.ends_with("div_clk")).unwrap();
    let leaf_st = nl.states.iter().find(|s| s.name.ends_with("u_leaf.dout")).unwrap();
    assert_eq!(
        leaf_st.clock.unwrap().signal,
        div_st.out,
        "child register clock must stitch directly to parent's div_clk StateOut"
    );

    let mut sim = NetlistSim::new(&nl);
    sim.set_u("d", 11);
    // Tick 1: `div_clk` transitions `0 -> 1`, triggering `u_leaf.dout <= 11`!
    sim.tick_posedge("clk");
    assert_eq!(sim.out_i("q"), 11);
    // Tick 2: `div_clk` transitions `1 -> 0` (negedge, no posedge on `u_leaf.dout`), so `q` holds `11`!
    sim.set_u("d", 3);
    sim.tick_posedge("clk");
    assert_eq!(sim.out_i("q"), 11);
    // Tick 3: `div_clk` transitions `0 -> 1`, capturing `3`!
    sim.tick_posedge("clk");
    assert_eq!(sim.out_i("q"), 3);
}

#[test]
fn s7_03_counter_ok_and_param_override() {
    let nl = lower_testdata("counter_ok.sv", &[("W", "3")], &LowerConfig::default()).unwrap_netlist();
    assert_eq!(nl.states.len(), 1);
    assert_eq!(nl.states[0].width, 3, "-G W=3 must set counter state width to 3");
    assert_eq!(nl.checks.len(), 1);
    assert_eq!(nl.checks[0].label.as_deref(), Some("chk_bound"));

    let mut sim = NetlistSim::new(&nl);
    sim.set_u("rst", 1);
    sim.set_u("en", 1);
    sim.tick_posedge("clk");
    assert_eq!(sim.out_i("cnt"), 0);
    sim.set_u("rst", 0);
    for expected in 1..=7 {
        sim.tick_posedge("clk");
        assert_eq!(sim.out_i("cnt"), expected);
    }
    // Modular 3-bit wrap `7 -> 0`.
    sim.tick_posedge("clk");
    assert_eq!(sim.out_i("cnt"), 0);
    assert!(sim.failed_asserts.is_empty());
}

#[test]
fn s7_04_initial_value_decl_and_initial_block() {
    let nl = lower_testdata("initial_value.sv", &[], &LowerConfig::default()).unwrap_netlist();
    let r_decl = nl.states.iter().find(|s| s.name.ends_with("r_decl")).unwrap();
    let r_init = nl.states.iter().find(|s| s.name.ends_with("r_init")).unwrap();
    assert_eq!(r_decl.init.as_ref().unwrap().i(), 3);
    assert_eq!(r_init.init.as_ref().unwrap().i(), 5);

    let mut sim = NetlistSim::new(&nl);
    assert_eq!(sim.out_i("a"), 3);
    assert_eq!(sim.out_i("b"), 5);
    sim.tick_posedge("clk");
    assert_eq!(sim.out_i("a"), 4);
    assert_eq!(sim.out_i("b"), 7);
}

#[test]
fn s7_05_multi_driver_disjoint_always_ff() {
    let nl = lower_testdata("multi_driver_ff.sv", &[], &LowerConfig::default()).unwrap_netlist();
    assert_eq!(nl.states.len(), 2, "disjoint nibble drivers must produce two 4-bit State slices");
    assert!(nl.undriven.is_empty());

    let mut sim = NetlistSim::new(&nl);
    sim.set_u("lo_in", 0xA);
    sim.set_u("hi_in", 0x5);
    sim.tick_posedge("clk");
    assert_eq!(sim.out_i("q"), 0x5A);
}

#[test]
fn s7_06_multi_driver_disjoint_packed_struct_fields() {
    let nl = lower_testdata("multi_driver_struct.sv", &[], &LowerConfig::default()).unwrap_netlist();
    assert_eq!(nl.states.len(), 2);
    assert!(nl.undriven.is_empty());

    let mut sim = NetlistSim::new(&nl);
    sim.set_u("a_in", 0xC); // MSB field `a` [7:4]
    sim.set_u("b_in", 0x3); // LSB field `b` [3:0]
    sim.tick_posedge("clk");
    assert_eq!(sim.out_i("packed_out"), 0xC3);
}

#[test]
fn s7_07_oob_read_returns_four_state_x() {
    let nl = lower_testdata("oob_read.sv", &[], &LowerConfig::default()).unwrap_netlist();
    let mut sim = NetlistSim::new(&nl);
    sim.set_u("vec", 0b1010);

    // Static OOB read `vec[5]` is always `1'bx`.
    assert_eq!(sim.out_cval("bit_const_oob").bit_state(0), BitState::X);

    // In-bounds dynamic read `idx = 1` -> `vec[1] == 1`, `arr[1] == 2`.
    sim.set_u("idx", 1);
    assert_eq!(sim.out_i("bit_dyn"), 1);
    assert_eq!(sim.out_i("mem_dyn"), 2);

    // Out-of-bounds dynamic read `idx = 6` -> `1'bx` and `4'bxxxx`!
    sim.set_u("idx", 6);
    assert_eq!(sim.out_cval("bit_dyn").bit_state(0), BitState::X);
    let mem_oob = sim.out_cval("mem_dyn");
    assert!((0..4).all(|b| mem_oob.bit_state(b) == BitState::X));
}

#[test]
fn s7_08_oob_write_is_masked_noop() {
    let nl = lower_testdata("oob_write.sv", &[], &LowerConfig::default()).unwrap_netlist();
    let mut sim = NetlistSim::new(&nl);
    sim.set_u("rst", 1);
    sim.tick_posedge("clk");
    assert_eq!(sim.out_i("q"), 0b1010);

    // Dynamic OOB index `idx = 5` (plus static OOB `q[6] <= 1`) must NOT corrupt `q`!
    sim.set_u("rst", 0);
    sim.set_u("idx", 5);
    sim.set_u("val", 1);
    sim.tick_posedge("clk");
    assert_eq!(sim.out_i("q"), 0b1010);

    // In-bounds index `idx = 0`, `val = 1` updates bit 0 cleanly (`0b1011`).
    sim.set_u("idx", 0);
    sim.set_u("val", 1);
    sim.tick_posedge("clk");
    assert_eq!(sim.out_i("q"), 0b1011);
}

#[test]
fn s7_09_signed_context_ieee_11_8() {
    let nl = lower_testdata("signed_context.sv", &[], &LowerConfig::default()).unwrap_netlist();
    let mut sim = NetlistSim::new(&nl);
    // `a = -4` (`4'b1100`), `u = 1` (`8'd1`).
    sim.set_u("a", -4);
    sim.set_u("u", 1);
    // `s_ext = a` sign-extends `-4` to `-4` (`8'hFC`).
    assert_eq!(sim.out_i("s_ext"), -4);
    // `mixed_sum = a + u`: mixed signed+unsigned -> unsigned 8-bit context -> `a` zero-extends to `12`, `12 + 1 = 13`!
    assert_eq!(sim.out_i("mixed_sum"), 13);
}

#[test]
fn s7_10_signed_port_boundary_coercions() {
    let nl = lower_testdata("signed_port.sv", &[], &LowerConfig::default()).unwrap_netlist();
    let mut sim = NetlistSim::new(&nl);
    // `u4 = 4'hF` (unsigned `15` -> zero-extended to `8'd15` on child input),
    // `s4 = -4'sd2` (`4'hE` -> sign-extended to `-2` = `8'hFE` = `254` on signed child input).
    sim.set_u("u4", 15);
    sim.set_u("s4", -2);
    // Child drives `narrow_s4 = -3` (4-bit signed) into parent's 8-bit signed `wide_from_child` -> sign-extended `-3`!
    assert_eq!(sim.out_i("wide_from_child"), -3);
    // `15 + (-2) = 13`!
    assert_eq!(sim.out_i("sum_out"), 13);
}

#[test]
fn s7_11_wildcard_eq_preserves_xz_and_rejects_nonconst_rhs() {
    let nl = lower_testdata("wildcard_eq.sv", &[], &LowerConfig::default()).unwrap_netlist();
    let has_wild_eq =
        nl.values.iter().any(|v| matches!(&v.kind, ValueKind::Op { op: NetOp::WildEq { rhs }, .. } if !rhs.known()));
    assert!(has_wild_eq, "WildEq must preserve 4-state x/z bits in rhs CVal");

    let mut sim = NetlistSim::new(&nl);
    // Pattern `4'b1x0z`: both `4'b1000` and `4'b1101` match; `4'b1010` does not.
    sim.set_u("a", 0b1000);
    assert_eq!(sim.out_i("hit_eq"), 1);
    sim.set_u("a", 0b1101);
    assert_eq!(sim.out_i("hit_eq"), 1);
    sim.set_u("a", 0b1010);
    assert_eq!(sim.out_i("hit_eq"), 0);

    // Non-constant RHS on `==?` must fail closed with `LowerOutcome::Error`.
    let bad = lower_src(
        "module m(input logic [3:0] a, input logic [3:0] b, output logic y);\n\
           assign y = (a ==? b);\n\
         endmodule\n",
    );
    assert!(matches!(bad, LowerOutcome::Error(_)), "expected Error on dynamic ==? RHS, got {bad:?}");
}

#[test]
fn s7_12_isunknown_fail_closed_by_default_and_configurable() {
    let default_out = lower_testdata("isunknown.sv", &[], &LowerConfig::default());
    assert!(
        matches!(default_out, LowerOutcome::Unsupported(_)),
        "default LowerConfig must reject $isunknown as Unsupported, got {default_out:?}"
    );

    let allowed_cfg = LowerConfig { allow_four_state_ops: true, ..LowerConfig::default() };
    let nl = lower_testdata("isunknown.sv", &[], &allowed_cfg).unwrap_netlist();
    let mut sim = NetlistSim::new(&nl);
    sim.set_u("x", 5);
    assert_eq!(sim.out_i("unk"), 0);
    sim.set_cval("x", CVal::parse_literal("4'b10x1").unwrap());
    assert_eq!(sim.out_i("unk"), 1);
}

#[test]
fn s7_13_overlap_drivers_rejected_across_all_driver_pairs() {
    // 1. Two `always_ff` blocks overlapping on bit `[2]`.
    let out1 = lower_testdata("overlap_drivers.sv", &[], &LowerConfig::default());
    match out1 {
        LowerOutcome::Error(ds) => {
            assert!(ds[0].message.contains("overlapping drivers") && ds[0].message.contains("[2]"), "{ds:?}");
        }
        other => panic!("expected Error on overlapping always_ff drivers, got {other:?}"),
    }

    // 2. Continuous `assign` + `always_ff` on same bit.
    let out2 = lower_src(
        "module m(input logic clk, input logic a, output logic q);\n\
           assign q = a;\n\
           always_ff @(posedge clk) q <= ~a;\n\
         endmodule\n",
    );
    assert!(matches!(out2, LowerOutcome::Error(_)));

    // 3. Two `always_comb` blocks overlapping on struct field `s.a`.
    let out3 = lower_src(
        "module m(input logic [3:0] x, output logic [7:0] y);\n\
           typedef struct packed { logic [3:0] a; logic [3:0] b; } st_t;\n\
           st_t s;\n\
           always_comb begin s.a = x; s.b = 4'd1; end\n\
           always_comb s.a = ~x;\n\
           assign y = s;\n\
         endmodule\n",
    );
    assert!(matches!(out3, LowerOutcome::Error(_)));
}

#[test]
fn s7_14_mixed_edges_preserved() {
    let nl = lower_testdata("mixed_edges.sv", &[], &LowerConfig::default()).unwrap_netlist();
    let pos_st = nl.states.iter().find(|s| s.name.ends_with("q_pos")).unwrap();
    let neg_st = nl.states.iter().find(|s| s.name.ends_with("q_neg")).unwrap();
    assert_eq!(pos_st.clock.unwrap().edge, Edge::Posedge);
    assert_eq!(neg_st.clock.unwrap().edge, Edge::Negedge);

    let mut sim = NetlistSim::new(&nl);
    sim.set_u("clk", 0);
    sim.set_u("d", 6);
    sim.set_and_step("clk", 1); // posedge: `q_pos <= 6`
    assert_eq!(sim.out_i("q_pos"), 6);
    sim.set_and_step("clk", 0); // negedge: `q_neg <= q_pos + 1 = 7`
    assert_eq!(sim.out_i("q_neg"), 7);
}

#[test]
fn s7_15_two_clocks_preserved() {
    let nl = lower_testdata("two_clocks.sv", &[], &LowerConfig::default()).unwrap_netlist();
    let st_a = nl.states.iter().find(|s| s.name.ends_with("stage_a")).unwrap();
    let st_b = nl.states.iter().find(|s| s.name.ends_with("dout")).unwrap();
    assert_ne!(st_a.clock.unwrap().signal, st_b.clock.unwrap().signal);

    let mut sim = NetlistSim::new(&nl);
    sim.set_u("din", 9);
    sim.tick_posedge("clk_a");
    sim.tick_posedge("clk_b");
    assert_eq!(sim.out_i("dout"), 9);
}

#[test]
fn s7_16_clock_typo_fails_closed() {
    let out = lower_testdata("clock_typo.sv", &[], &LowerConfig::default());
    assert!(matches!(out, LowerOutcome::Error(_)), "expected Error on clock typo, got {out:?}");
    assert!(out.diagnostics().iter().any(|d| d.message.contains("clkk")));
}

#[test]
fn s7_17_reset_typo_fails_closed() {
    let out = lower_testdata("reset_typo.sv", &[], &LowerConfig::default());
    assert!(matches!(out, LowerOutcome::Error(_)), "expected Error on reset typo, got {out:?}");
    assert!(out.diagnostics().iter().any(|d| d.message.contains("rstt_n")));
}

#[test]
fn s7_18_assume_inside_task_inherits_enable_and_clock() {
    let nl = lower_testdata("assume_task.sv", &[], &LowerConfig::default()).unwrap_netlist();
    assert_eq!(nl.checks.len(), 1);
    let chk = &nl.checks[0];
    assert_eq!(chk.kind, CheckKind::Assume);
    assert_eq!(chk.label.as_deref(), Some("env_req_en"));
    let clk = chk.clock.expect("task called inside always_ff must inherit active clock");
    assert_eq!(clk.edge, Edge::Posedge);
    let vld_in = nl.inputs.iter().find(|i| i.name == "vld").unwrap().value;
    assert_eq!(chk.enable, vld_in, "task called under `if (vld)` must inherit `vld` as Check::enable");
}
