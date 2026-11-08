// Compliant Clean Test Case: SV-DED-002 (UnreachableStatementsPostReturn)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_unreachablestatementspostreturn extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
