// Compliant Clean Test Case: SVA-CTX-001 (ConcurrentAssertInFunction)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_concurrentassertinfunction extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
