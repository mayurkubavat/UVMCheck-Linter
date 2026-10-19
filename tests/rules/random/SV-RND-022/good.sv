// Compliant Clean Test Case: SV-RND-022 (ConstraintCallingNonPureFunction)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_constraintcallingnonpurefunction extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
