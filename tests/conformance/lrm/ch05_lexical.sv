// IEEE 1800-2023 clause 5: lexical conventions.
// 5.4 comments, 5.6 identifiers (escaped), 5.7 numbers, 5.8 time literals,
// 5.9 string literals, 5.10 structure literals, 5.11 array literals, 5.12 attributes.
module ch05_lexical;
  typedef struct { int a; byte b; } pair_t;
  logic [15:0] hex_v = 16'hBE_EF;   /* block comment */
  logic [7:0] bin_v = 8'b1010_zx01;
  int dec_v = 'd42;
  logic [3:0] ones = '1;
  real r = 1.5e3;
  time t = 10ns;
  string s = "tab\there \"quoted\"";
  pair_t p = '{a: 1, b: 8'h2};
  int arr [3] = '{1, 2, 3};
  logic \escaped$name ;
  (* keep = 1 *) logic attr_v;
  initial begin
    \escaped$name = 1'b0;
    attr_v = 1'b1;
    $display("%h %b %0d %b %f %t %s %0d %0d", hex_v, bin_v, dec_v, ones, r, t, s, p.a, arr[2]);
  end
endmodule
