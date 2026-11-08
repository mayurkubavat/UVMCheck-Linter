// Clause 17: checkers. 17.2 declaration with formal arguments, 17.4 context inference,
// 17.5 checker procedures, 17.6 covergroups in checkers, 17.7 checker variables.
checker req_ack_chk(logic req, logic ack, event clk = $inferred_clock);
  default clocking @clk; endclocking
  bit seen;
  always_ff @clk seen <= req;
  a_ack: assert property (req |-> ##[1:3] ack);
  covergroup cg @clk;
    coverpoint req;
  endgroup
  cg cov = new();
endchecker

module ch17_checkers(input logic clk, input logic req, input logic ack);
  default clocking @(posedge clk); endclocking
  req_ack_chk u_chk(req, ack);
endmodule
