// slang: --relax-enum-conversions
// An int assigned to an enum without a cast (6.19.3 needs one).
module probe;
  typedef enum {RED, GREEN} color_t;
  color_t c;
  int i = 1;
  initial c = i;
endmodule
