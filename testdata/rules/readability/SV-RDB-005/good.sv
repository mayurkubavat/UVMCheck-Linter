// SV-RDB-005 good: direct boolean expressions and multi-bit compares.
class fifo_model;
  bit full;
  bit [1:0] mode;
  int q[$];
  function bit can_push();
    return !full;
  endfunction
  function bit not_empty();
    return q.size() > 0;
  endfunction
  function bit mode_one();
    return mode == 1;
  endfunction
endclass

// A counter one bit wide only when its width parameter is 1.
module fifo_count #(parameter int unsigned UsageWidth = 1) (input logic [UsageWidth-1:0] cnt_q, output logic empty_o);
  assign empty_o = (cnt_q == 0);
endmodule
