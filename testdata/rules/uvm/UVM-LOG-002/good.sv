// Compliant Clean Test Case: UVM-LOG-002 (ProhibitedDisplayInUvm)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_prohibiteddisplayinuvm extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
