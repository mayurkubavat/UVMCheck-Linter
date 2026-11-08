// Clauses 28-31: gate level, UDPs, specify blocks, timing checks. 28.4-28.10 gate and
// switch primitives with delays and strengths, 29.3-29.8 combinational and sequential UDPs,
// 30.3-30.5 specify paths and specparams, 31.3 $setup/$hold timing checks with notifier.
primitive ch29_mux (out, sel, a, b);
  output out;
  input sel, a, b;
  table
    0 0 ? : 0;
    0 1 ? : 1;
    1 ? 0 : 0;
    1 ? 1 : 1;
  endtable
endprimitive

primitive ch29_dff (q, clk, d);
  output reg q;
  input clk, d;
  initial q = 0;
  table
    (01) 0 : ? : 0;
    (01) 1 : ? : 1;
    (0?) ? : ? : -;
    ?   (??) : ? : -;
  endtable
endprimitive

module ch28_31_gates_udp_specify(input wire a, input wire b, input wire sel, input wire clk, output wire y,
                                 output wire q);
  wire n1, n2;
  reg notifier;
  and #(1, 2) g1 (n1, a, b);
  nor (strong0, weak1) g2 (n2, a, b);
  bufif1 g3 (y, n1, sel);
  pullup (n2);
  ch29_mux m1 (q, sel, n1, n2);
  ch29_dff f1 (, clk, a);
  specify
    specparam tRise = 1.0, tFall = 1.2;
    (a => y) = (tRise, tFall);
    (b *> y) = 2;
    $setup(a, posedge clk, 1, notifier);
    $hold(posedge clk, a, 1, notifier);
  endspecify
endmodule
