// slang: --allow-array-concat-assign-pattern
// An assignment pattern inside an unpacked array concatenation (10.10.1).
module probe;
  int q[$][2];
  initial q = {'{1, 2}, '{3, 4}};
endmodule
