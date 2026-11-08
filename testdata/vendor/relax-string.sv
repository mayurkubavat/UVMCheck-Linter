// slang: --relax-string-conversions
// A string assigned to an integral variable without a cast (6.16).
module probe;
  string s = "A";
  int i;
  initial i = s;
endmodule
