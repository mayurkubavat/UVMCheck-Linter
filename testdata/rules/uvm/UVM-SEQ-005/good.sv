// Compliant Clean Test Case: UVM-SEQ-005 (EmptySequenceBody)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_emptysequencebody extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
