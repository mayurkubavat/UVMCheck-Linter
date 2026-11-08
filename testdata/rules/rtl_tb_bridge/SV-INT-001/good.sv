// Compliant Clean Test Case: SV-INT-001 (BlockingAssignmentToInterface)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_blockingassignmenttointerface extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
