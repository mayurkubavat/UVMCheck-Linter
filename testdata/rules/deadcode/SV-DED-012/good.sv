// Compliant Clean Test Case: SV-DED-012 (UnreferencedParameter)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_unreferencedparameter extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
