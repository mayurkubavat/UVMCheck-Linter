// slang: --allow-self-determined-stream-concat
// A streaming concatenation in a self-determined context (11.4.14 allows it
// only as an assignment target or source, or a cast operand).
module probe;
  int x = 5;
  initial $display("%0d", {<<{x}});
endmodule
