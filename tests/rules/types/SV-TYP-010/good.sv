// Compliant Clean Test Case: SV-TYP-010 (TaskWithNonVoidReturnType)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_taskwithnonvoidreturntype extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
