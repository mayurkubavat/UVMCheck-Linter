// slang: --allow-merging-ansi-ports
// An ANSI port redeclared in the body (23.2.2.2 forbids it).
module probe(input logic a, output logic y);
  logic a;
  assign y = a;
endmodule
