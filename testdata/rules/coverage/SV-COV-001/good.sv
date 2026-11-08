// Compliant Clean Test Case: SV-COV-001 (UnreferencedCovergroup)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_unreferencedcovergroup extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
