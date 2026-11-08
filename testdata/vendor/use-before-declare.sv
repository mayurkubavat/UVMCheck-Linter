// slang: --allow-use-before-declare
// A variable used before its declaration (IEEE 1800 6.18: declare before use).
module probe;
  initial x = 1;
  logic x;
endmodule
