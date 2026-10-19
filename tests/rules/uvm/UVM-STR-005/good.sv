// Compliant Clean Test Case: UVM-STR-005 (ComponentNullParentInCreate)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_componentnullparentincreate extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
