// slang: --allow-hierarchical-const
// A hierarchical reference in a constant expression (IEEE 1800 11.2.1 forbids it).
module child;
  localparam int P = 3;
endmodule
module probe;
  child u();
  localparam int Q = u.P;
endmodule
