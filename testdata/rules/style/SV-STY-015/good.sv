// Compliant Clean Test Case: SV-STY-015 (TrailingWhitespace)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_trailingwhitespace extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
