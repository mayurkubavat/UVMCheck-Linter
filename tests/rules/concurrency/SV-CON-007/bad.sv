// Test Case: SV-CON-007 (UnboundedMailboxInstantiation)
// Extracted from sample_tb.sv (Test Case 169)

`include "uvm_mock.svh"

// --- TEST CASE 169: SV-CON-007 (UnboundedMailboxInstantiation) ---
class UnboundedMailboxBench;
    mailbox #(int) data_mb;
    function new();
        // VIOLATION SV-CON-007: Mailbox instantiated without bound limit (unbounded)!
        data_mb = new();
    endfunction
endclass
