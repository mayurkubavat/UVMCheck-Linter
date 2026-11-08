// Compliant Clean Test Case: SV-COR-005 (NonShortCircuitLogical)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_nonshortcircuitlogical extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
