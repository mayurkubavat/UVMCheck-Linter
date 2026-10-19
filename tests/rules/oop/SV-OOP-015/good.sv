// Compliant Clean Test Case: SV-OOP-015 (VirtualMethodSignatureMismatch)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_virtualmethodsignaturemismatch extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
