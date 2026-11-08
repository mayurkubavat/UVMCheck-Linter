// slang: --allow-bare-value-param-assigment
// A parameter value without parentheses: `child #W u()` (23.10.2 wants #(W)).
module child #(parameter int P = 1);
endmodule
module probe;
  localparam int W = 4;
  child #W u();
endmodule
