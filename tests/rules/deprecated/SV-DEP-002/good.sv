// Compliant Clean Test Case: SV-DEP-002 (DefparamDeprecation)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_defparamdeprecation extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
