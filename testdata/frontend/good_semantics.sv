// The legal neighbours of bad_semantics.sv: no compile errors.
interface bus_if (input logic clk);
  logic v;
  clocking cb @(posedge clk); output v; endclocking
  modport drv_mp (clocking cb);
endinterface

typedef struct { rand bit a; bit b; } pair_t;

class item;
  rand int w;
  rand bit [3:0] x[4];
  rand pair_t p;
  virtual bus_if vif;
  virtual bus_if.drv_mp mp;

  constraint order_c {
    solve w before x;
    solve p.a before w;
    foreach (x[i]) x[i] < w;
  }

  function void check_name(string s, string opts[$]);
    if (s inside {opts}) w = 1;
    if (s[0] inside {"A", "B"}) w = 2;
    mp = vif;
  endfunction
endclass

module sub (input logic clk); logic v; bus_if bi (clk); endmodule

module tb;
  logic clk;
  sub u_sub (clk);
  assign u_sub.v = 1'b0;
  assign u_sub.bi.v = 1'b0;
  initial tb.u_sub.v = 1'b1;
endmodule

// A bind in a module the top does not instantiate binds nothing, so the
// path is only checked when `bound` is elaborated (no -top here: it is).
module bound; bind sub bus_if u_mon (.clk); endmodule
module tb2;
  logic clk;
  sub u_sub (clk);
  assign u_sub.u_mon.v = 1'b0;
endmodule
