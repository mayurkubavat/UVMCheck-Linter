# `svlang`

Pure-Rust IEEE 1800-2023 SystemVerilog compiler front end and RTL netlist SDK, serving as the Rust counterpart to C++ `slang` with **zero C++ toolchain dependencies**, **zero external crates**, and **zero UVM coupling**.

## Architecture

`svlang` unifies six focused crates behind one SDK:

| Module | Crate | Responsibility |
| --- | --- | --- |
| `svlang::source` | `sv-source` | VCS and Xcelium `-f` / `-F` filelists, `$VAR` expansion, `+incdir+`, `+define+`, `-G` / `--param` / `-pvalue+`, `-y` / `-v` |
| `svlang::lexer` | `sv-lexer` | Lossless IEEE 1800-2023 tokenizer (every source byte preserved) |
| `svlang::preprocessor` | `sv-preprocessor` | Clause 22 macro, `` `include ``, and `` `ifdef `` expansion with full token provenance |
| `svlang::syntax` | `sv-syntax` | Error-recovering lossless Concrete Syntax Tree (CST) |
| `svlang::semantic` | `sv-semantic` | Scopes, 4-state arbitrary-width constant evaluation, elaborated design objects (`ElaboratedDesign`), IEEE 11.6/11.8 bound expressions (`ExprBinder`), bound SVA property/sequence trees (`SvaBinder`), compiler checks, and OOP class model |
| `svlang::netlist` | `sv-netlist` | Format-neutral word-level RTL netlist IR (`Netlist`), symbolic procedural execution (`always_ff` / `always_comb` / `always_latch`), bit-level driver analysis, static loop unrolling, subroutine inlining, SVA lowering, and `blast_memories` |

## Quickstart

```rust
use svlang::{Driver, SyntaxKind};

fn main() -> Result<(), svlang::Error> {
    let comp = Driver::new()
        .define("DATA_W", Some("32".into()))
        .add_source(
            "top.sv",
            "module top #(parameter int W = `DATA_W) (input logic clk, output logic [W-1:0] q);\n\
               always_ff @(posedge clk) q <= q + 1'b1;\n\
             endmodule\n",
        )
        .top("top")
        .compile()?;

    let (diags, elab) = comp.check();
    assert!(diags.is_empty());
    for line in &elab.hierarchy {
        println!("{line}");
    }
    Ok(())
}
```

Or compile existing VCS / Xcelium simulator arguments directly:

```rust
let comp = svlang::Compilation::from_args(["-f", "sim/top.f", "-top", "tb_top"])?;
for diag in comp.diagnostics() {
    eprintln!("{diag}");
}
```

## Examples

- `cargo run -p svlang --example custom_linter`: custom CST and OOP semantic lint rules in ~70 lines of Rust
- `cargo run -p svlang --example elab_hierarchy`: generate-loop unrolling and constant parameter propagation
- `cargo run -p svlang --example rtl_netlist`: parameterized RTL netlist lowering and external SVA property binding
