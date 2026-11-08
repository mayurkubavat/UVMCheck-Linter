// Compliant Clean Test Case: UVM-SEQ-009 (SequenceBodyWithoutStartItem)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_sequencebodywithoutstartitem extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
