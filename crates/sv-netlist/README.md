# `sv-netlist`

Pure-Rust IEEE 1800-2023 RTL netlist lowering, symbolic procedural execution, and bit-level driver analysis (`svlang::netlist`).

## Overview

`sv-netlist` lowers parsed CST [`sv_syntax::Tree`]s and an [`sv_semantic::ElaboratedDesign`] into a format-neutral, topologically sorted word-level [`Netlist`] alongside bound SVA [`sv_semantic::BoundProperty`] trees. Back-end tools such as Btor2 or AIGER model checkers, equivalence checkers, RTL linters, and cycle simulators consume this IR without re-implementing SystemVerilog elaboration or procedural execution.

## Pipeline

1. **Object allocation and array classification**: flattens elaborated module and interface instances into packed signal slots and classifies 1-D unpacked arrays as either preserved [`Memory`] objects or scalarized element slots.
2. **Declaration initializers and `initial` execution**: folds constant variable and memory initializers into register `init` values and [`Memory::init`].
3. **Continuous, gate, port, and procedural lowering**: lowers `assign` statements, gate primitives, hierarchical port connections, `always_comb`, `always_latch`, and clocked `always_ff` / `always @(posedge ...)` blocks (with optional asynchronous reset extraction, static loop unrolling, and subroutine inlining).
4. **Bit-level driver validation and resolution**: checks IEEE 1800 multiple-driver, `always_comb` latch, and procedural-vs-continuous rules per bit, then stitches disjoint partial-slice drivers and resolves `wire` multi-driver nets.
5. **SVA property lowering and topological sorting**: lowers clocked SVA expressions into [`Netlist::values`], checks combinational loops via Tarjan strongly connected components (treating `$past` and sequential elements as state boundaries), and orders nodes topologically.

## Quickstart

```rust
use sv_netlist::{lower_trees, LowerConfig, LowerOutcome};
use sv_syntax::{parse_source, Options};

let tree = parse_source(
    "module counter (input logic clk, rst, output logic [7:0] q);\n\
       always_ff @(posedge clk) if (rst) q <= 8'd0; else q <= q + 8'd1;\n\
     endmodule\n",
    "counter.sv",
    Options::default(),
);

match lower_trees(&[tree], &[], &[], &LowerConfig::default()) {
    LowerOutcome::Lowered(nl) => {
        assert_eq!(nl.states.len(), 1);
        println!("{}", nl.dump());
    }
    LowerOutcome::Unsupported(diags) | LowerOutcome::Error(diags) => {
        for d in diags {
            eprintln!("{d}");
        }
    }
}
```
