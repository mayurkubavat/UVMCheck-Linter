//! Demonstrates RTL netlist lowering, parameter overrides, and external SVA property
//! binding and lowering through the `svlang` SDK.

use svlang::{Driver, ExprArena, LowerConfig};

fn main() -> Result<(), svlang::Error> {
    let rtl = r#"
module fifo_counter #(
    parameter int DEPTH = 8,
    localparam int CNT_W = $clog2(DEPTH + 1)
) (
    input  logic             clk,
    input  logic             rst,
    input  logic             push,
    input  logic             pop,
    output logic [CNT_W-1:0] count,
    output logic             full,
    output logic             empty
);
    assign full  = (count == CNT_W'(DEPTH));
    assign empty = (count == '0);

    let do_push = push && !full;
    let do_pop  = pop  && !empty;

    always_ff @(posedge clk) begin
        if (rst)
            count <= '0;
        else if (do_push && !do_pop)
            count <= count + 1'b1;
        else if (!do_push && do_pop)
            count <= count - 1'b1;
    end

    assert property (@(posedge clk) disable iff (rst) count <= CNT_W'(DEPTH));
endmodule
"#;

    let comp = Driver::new().add_source("fifo_counter.sv", rtl).top("fifo_counter").param("DEPTH", "16").compile()?;

    let (diags, elab) = comp.check();
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let mut nl = comp.lower_netlist(&LowerConfig::default()).unwrap_netlist();

    let mut ext_arena = ExprArena::new();
    let ext_prop = comp
        .bind_property_in_scope(
            &elab.design,
            "fifo_counter",
            "@(posedge clk) disable iff (rst) (do_push && !do_pop) |=> (count > 0)",
            &mut ext_arena,
        )
        .expect("external SVA property binds cleanly");

    let lowered_pairs = nl
        .lower_external_property(&elab.design, &ext_arena, &ext_prop, false)
        .expect("external SVA property lowers onto netlist");

    println!("{}", nl.dump());
    println!("external property lowered {} boolean/clock/reset expressions", lowered_pairs.len());
    Ok(())
}
