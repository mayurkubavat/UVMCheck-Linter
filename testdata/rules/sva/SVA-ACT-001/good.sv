// Compliant Clean Test Case: SVA-ACT-001 (AssertWithoutElse)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_assertwithoutelse extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
