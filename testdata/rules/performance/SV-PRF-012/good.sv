// Compliant Clean Test Case: SV-PRF-012 (UnboundedDynamicArrayInConstraint)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_unboundeddynamicarrayinconstraint extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
