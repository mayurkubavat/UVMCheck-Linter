// Clause 11: operators and expressions. 11.4 arithmetic, logical, bitwise, reduction, shift,
// wildcard equality, inside, implication, conditional, concatenation and replication,
// 11.4.14 streaming, 11.9 tagged union expressions, 11.11 min:typ:max, 11.12 let.
module ch11_operators;
  let max2(x, y) = (x > y) ? x : y;
  logic [7:0] a = 8'h5A, b = 8'h0F;
  logic [15:0] cat;
  logic [31:0] stream;
  byte bytes [4] = '{1, 2, 3, 4};
  bit ok;
  int i = 3;
  initial begin
    cat = {a, b};
    cat = {2{b}};
    ok = (a ==? 8'b0101_xxxx) && (a inside {[8'h50:8'h5F], 8'hFF}) && (|a) && !(&b);
    ok = (a != b) -> (b < a);
    ok = (a <-> b) || (a === b) || (a !== b);
    stream = {<<8{bytes}};
    {>>{bytes}} = stream;
    i += max2(i, 4) ** 2 >>> 1;
    i = (i % 5) << 2;
    a = ~a ^ (a & b | b);
    i = (1:2:3);
    $display("%0d", i);
  end
endmodule
