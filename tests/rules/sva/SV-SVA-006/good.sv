// SV-SVA-006 (ComplexInlineProperty): named sequences keep the assertion readable
module sva006_good_chk(input logic clk, rst_n, cs, valid, done, input logic [1:0] cmd);
    sequence s_wr_cmd_phase;
        $rose(cs) ##1 (cmd == 2'b01) ##1 valid[*4];
    endsequence
    a_wr_done: assert property (@(posedge clk) disable iff (!rst_n) s_wr_cmd_phase |-> ##[1:3] done);
    a_req_ack: assert property (@(posedge clk) disable iff (!rst_n) valid |=> done);
endmodule
