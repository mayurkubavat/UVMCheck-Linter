// Uses what a.sv declares, so rules must read nodes of the other file's
// tree with that tree.
module mf_top;
  import mf_pkg::*;
  typedef enum logic [1:0] { LOW, HIGH } level_e;
  level_e lvl;
  state_e st;
  initial begin
    shape s;
    s = new();
    lvl = HIGH;
    st = BUSY;
    if (st == DONE && lvl == LOW) $display("done");
  end
endmodule
