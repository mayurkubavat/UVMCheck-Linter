// Compliant Clean Test Case: SV-STY-001 (ClassMemberPrefix)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_classmemberprefix extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
