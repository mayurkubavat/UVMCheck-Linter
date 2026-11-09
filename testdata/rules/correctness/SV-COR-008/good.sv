// Compliant Clean Test Case: SV-COR-008 (SelfComparison)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_selfcomparison extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass

// Two equal literals are a constant test, not a variable compared to itself.
// A macro argument's default expands to one (UVM's own compare macros).
`define cmp_named(VALUE, NAME="") \
  if (NAME == "") $display("%s", `"VALUE`"); \
  else $display("%s", NAME);

module self_cmp_literals;
  int v;
  initial begin
    `cmp_named(v)
    `cmp_named(v, "value")
  end
endmodule
