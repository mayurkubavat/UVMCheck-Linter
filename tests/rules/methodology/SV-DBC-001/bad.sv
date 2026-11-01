// SV-DBC-001: violations are marked.
interface axi_lite_if (input logic clk); // VIOLATION
  logic valid, ready;
  logic [31:0] addr;
  clocking cb @(posedge clk);
    output valid, addr;
    input ready;
  endclocking
  modport master (clocking cb);
endinterface
