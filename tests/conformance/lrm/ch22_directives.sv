// Clause 22: compiler directives. 22.5 `define with arguments and defaults, `undef,
// 22.6 `ifdef/`elsif/`else/`ifndef, 22.7 `timescale, 22.8 `default_nettype,
// 22.11 `pragma, 22.12 `line, 22.13 `__FILE__/`__LINE__, 22.14 `begin_keywords.
`timescale 1ns/1ps
`default_nettype none
`define MAX(a, b = 0) ((a) > (b) ? (a) : (b))
`define STR(x) `"x`"
`ifdef NOT_DEFINED
  `define MODE 0
`elsif ALSO_NOT
  `define MODE 1
`else
  `define MODE 2
`endif
`pragma my_tool keep
`begin_keywords "1800-2017"
module ch22_directives;
  initial $display("%0d %0d %s %s %0d", `MAX(3), `MODE, `STR(hi), `__FILE__, `__LINE__);
endmodule
`end_keywords
`undef MODE
`default_nettype wire
