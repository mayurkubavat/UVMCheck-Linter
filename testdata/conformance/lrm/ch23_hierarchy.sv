// Clause 23: modules and hierarchy. 23.2 ANSI and non-ANSI ports, parameterized modules,
// 23.3 instantiation (ordered, named, .name, .*), 23.4 nested modules, 23.6 hierarchical
// names, 23.8 upward references, 23.10 parameter overrides and defparam, 23.11 bind.
module ch23_leaf #(parameter int W = 4) (input logic [W-1:0] a, output logic [W-1:0] y);
  assign y = ~a;
endmodule

module ch23_old(a, y);
  input a;
  output y;
  wire a, y;
  assign y = a;
endmodule

module ch23_probe(input logic s);
  initial $display("%b", s);
endmodule

module ch23_hierarchy;
  logic [7:0] a, y;
  logic [3:0] n_a, n_y;
  logic s1, s2;
  ch23_leaf #(.W(8)) u_named (.a(a), .y(y));
  ch23_leaf u_ordered (n_a, n_y);
  ch23_old u_old (.a(s1), .y(s2));
  defparam u_ordered.W = 4;
  module nested; logic inner; endmodule
  nested u_nested();
  initial $display("%0d %b", u_named.W, u_nested.inner);
  bind ch23_leaf ch23_probe u_probe(.s(a[0]));
endmodule
