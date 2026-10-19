// Compliant Clean Test Case: SV-DED-001 (UnusedVariable)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_unusedvariable extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
