// slang: --allow-cross-auto-bin-max
// The SystemVerilog 3.1a cross_auto_bin_max option (dropped from IEEE 1800).
module probe;
  bit [1:0] a, b;
  covergroup cg;
    option.cross_auto_bin_max = 4;
    ca: coverpoint a;
    cb: coverpoint b;
    cx: cross ca, cb;
  endgroup
endmodule
