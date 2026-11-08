// SV-DBC-001: no violations.
interface checked_if (input logic clk);
  logic valid, ready;
  clocking cb @(posedge clk);
    output valid;
    input ready;
  endclocking
  // valid stays high until ready.
  assert property (@(posedge clk) valid && !ready |=> valid);
endinterface

// No clocking block or modport: a bundle of wires, not a protocol.
interface plain_if;
  logic a, b;
endinterface

// Checked by a bound checker module.
interface bound_if (input logic clk);
  logic req, gnt;
  modport host (output req, input gnt);
endinterface

interface bound_if_checker (input logic clk, req, gnt);
  assert property (@(posedge clk) req |-> ##[1:4] gnt);
endinterface

module top;
  logic clk;
  checked_if c (clk);
  plain_if p ();
  bound_if b (clk);
endmodule

bind bound_if bound_if_checker chk (.clk(clk), .req(req), .gnt(gnt));
