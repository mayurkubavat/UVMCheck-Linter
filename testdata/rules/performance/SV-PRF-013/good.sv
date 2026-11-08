// The fork runs once. The clocked process does its own work.
timeunit 1ns;
timeprecision 1ns;
module clocked_fork;
  logic clk;
  int n;
  initial begin : once
    fork
      n = n + 1;
    join
  end
  always_ff @(posedge clk) begin : pump
    n <= n + 1;
  end
endmodule : clocked_fork
