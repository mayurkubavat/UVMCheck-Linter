// Compliant Clean Test Case: SV-OOP-007 (StaticMemberInUvmComponent)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_staticmemberinuvmcomponent extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
