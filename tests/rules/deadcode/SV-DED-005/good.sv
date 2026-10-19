// Compliant Clean Test Case: SV-DED-005 (UnreadClassProperty)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_unreadclassproperty extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
