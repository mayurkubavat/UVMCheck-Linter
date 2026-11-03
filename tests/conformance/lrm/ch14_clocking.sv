// Clause 14: clocking blocks. 14.3 declaration, 14.4 skews, 14.10 clocking events,
// 14.11 cycle delay, 14.12 default clocking, 14.14 global clocking, 14.16 synchronous drives.
module ch14_clocking(input logic clk, input logic [7:0] din, output logic [7:0] dout);
  default clocking cb @(posedge clk);
    default input #1step output #2;
    input din;
    output dout;
  endclocking
  global clocking gclk @(posedge clk); endclocking
  initial begin
    ##2;
    cb.dout <= cb.din;
    @(cb);
    ##1 cb.dout <= 8'h0;
  end
endmodule
