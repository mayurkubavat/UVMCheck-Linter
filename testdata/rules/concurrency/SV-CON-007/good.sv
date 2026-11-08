// Compliant Clean Test Case: SV-CON-007 (UnboundedMailboxInstantiation)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_unboundedmailboxinstantiation extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
