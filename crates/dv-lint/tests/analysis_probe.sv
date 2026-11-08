class pkt;
  rand bit [3:0] len;
  rand int x;
  rand byte b;
  constraint c1 { len > 10; x inside {[1:3], 7}; soft x == 9; x dist {1 := 2, [20:30] :/ 1}; }
  constraint c2 { len < 5; x != 3; }
  constraint c3 { b > 200; }
endclass
class ok;
  rand bit [3:0] len;
  constraint short_c { len < 5; }
  constraint long_c  { len > 10 && len != 12; }
  constraint d { soft len == 3; }
endclass
module m(input logic clk);
  logic a, b, c; logic [7:0] v, u; logic [15:0] w;
  always @(posedge clk) a = b;
  always @(posedge clk) c <= a;
  always @(posedge clk) begin logic t; t = v; u <= t; end
  always @(negedge clk) b = c;
  covergroup cg @(posedge clk);
    cp: coverpoint v;
    cx: cross cp, w;
    cy: cross cp, u;
  endgroup
  import "DPI-C" function int f(input int a);
  import "DPI-C" function void f(input longint a);
  import "DPI-C" cg_fn = function int g(input int a);
endmodule
module n; import "DPI-C" function int f(input int x); endmodule
