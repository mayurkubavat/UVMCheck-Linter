//! Minimal custom SystemVerilog linter built on `svlang`.
//!
//! Demonstrates combining a CST check (blocking `=` inside `always_ff`) with an
//! OOP semantic model check (non-virtual methods on polymorphic classes):
//!
//!     cargo run -p svlang --example custom_linter

use svlang::{Driver, SyntaxKind};

const SAMPLE_SV: &str = r#"
module fifo_ctrl (input logic clk, input logic rst_n, input logic d, output logic q);
  always_ff @(posedge clk or negedge rst_n) begin
    if (!rst_n) q = 1'b0; // <-- blocking assignment in sequential block
    else        q <= d;
  end
endmodule

class packet;
  rand bit [7:0] payload;
  function bit [7:0] crc(); // <-- non-virtual method in a base class
    return payload ^ 8'h5a;
  endfunction
endclass

class ext_packet extends packet;
  function bit [7:0] crc();
    return ~payload;
  endfunction
endclass
"#;

fn main() {
    let comp = Driver::new().add_source("sample.sv", SAMPLE_SV).compile().expect("valid inputs");
    let mut loc = comp.locator();

    // 1. CST pass: flag blocking `=` inside `always_ff`.
    for (tree_idx, tree) in comp.trees().iter().enumerate() {
        for node in tree.preorder() {
            if tree.kind(node) != SyntaxKind::ProceduralBlock {
                continue;
            }
            let Some((first, _)) = tree.token_bounds(node) else { continue };
            if tree.tokens[first as usize].text.as_ref() != "always_ff" {
                continue;
            }
            for sub in tree.subtree(node) {
                if tree.kind(sub) == SyntaxKind::BinaryExpr {
                    let has_blocking_eq = tree.node(sub).children.iter().any(|ch| {
                        matches!(*ch, svlang::syntax::Child::Token(tok) if tree.tokens[tok as usize].text.as_ref() == "=")
                    });
                    if has_blocking_eq {
                        let (tok, _) = tree.token_bounds(sub).unwrap();
                        let pos = loc.loc(tree_idx, tree, tok);
                        println!(
                            "{}:{}:{}: [RTL-001] blocking '=' inside always_ff",
                            loc.files()[pos.file as usize],
                            pos.line,
                            pos.column
                        );
                    }
                }
            }
        }
    }

    // 2. Semantic class-model pass: flag non-virtual method overrides.
    let design = comp.design();
    for cls in design.classes() {
        let Some(base_id) = cls.base else { continue };
        let base = design.class(base_id);
        for m in &cls.methods {
            if m.is_builtin || m.name == "new" {
                continue;
            }
            if let Some(bm) = base.methods.iter().find(|bm| bm.name == m.name && !bm.is_builtin) {
                if !bm.is_virtual {
                    println!(
                        "{}:{}:{}: [OOP-001] '{}::{}' shadows non-virtual '{}::{}'",
                        design.file_name(m.loc),
                        m.loc.line,
                        m.loc.column,
                        cls.name,
                        m.name,
                        base.name,
                        bm.name
                    );
                }
            }
        }
    }
}
