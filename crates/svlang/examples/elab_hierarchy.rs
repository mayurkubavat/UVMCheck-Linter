//! Elaborates a parameterized SystemVerilog module hierarchy with `svlang`.
//!
//!     cargo run -p svlang --example elab_hierarchy

use svlang::Driver;

const RTL: &str = r#"
module lane #(parameter int WIDTH = 8) (input logic clk);
  localparam int BYTES = WIDTH / 8;
endmodule

module crossbar #(parameter int LANES = 3, parameter int WIDTH = 16) (input logic clk);
  for (genvar g = 0; g < LANES; g++) begin : gen_lane
    lane #(.WIDTH(WIDTH + g * 8)) u_lane (.clk(clk));
  end
endmodule
"#;

fn main() {
    let comp = Driver::new().add_source("crossbar.sv", RTL).top("crossbar").compile().expect("compile crossbar");

    let (diags, elab) = comp.check();
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    for line in &elab.hierarchy {
        println!("{line}");
    }
}
