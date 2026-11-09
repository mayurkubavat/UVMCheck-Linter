// Compliant Clean Test Case: SV-LANG-005 (NonBlockingInFunction)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_nonblockinginfunction extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass

// A driver's reset function drives interface signals with '<=': legal in a
// function (IEEE 1800 13.4.4) and the drive SV-INT-001 asks for.
interface lang005_if;
  logic valid;
  logic [7:0] data;
endinterface

class lang005_driver;
  virtual lang005_if vif;
  function void reset_signals();
    vif.valid <= 1'b0;
    vif.data <= '0;
  endfunction
endclass
