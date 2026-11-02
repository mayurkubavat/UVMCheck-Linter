// SV-SVA-006 (ComplexInlineProperty): dense inline temporal logic
module sva006_chk(input logic clk, rst_n, cs, valid, done, input logic [1:0] cmd);
    a_wr_done: assert property (@(posedge clk) disable iff (!rst_n)
        ($rose(cs) ##1 (cmd == 2'b01) ##1 valid[*4]) |-> ##[1:3] done);
endmodule
