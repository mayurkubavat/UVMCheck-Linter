// A fork inside the clocked for-loop starts a thread every clock.
timeunit 1ns;
timeprecision 1ns;
module clocked_fork;
  logic clk;
  int n;
  always_ff @(posedge clk) begin : pump
    for (int i = 0; i < n; i++) begin
      fork
        n <= n + 1;
      join
    end
  end
  initial begin : paced
    forever begin
      @(posedge clk);
      fork
        n <= n + 1;
      join
    end
  end
endmodule : clocked_fork
