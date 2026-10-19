// Test Case: SV-COV-001 (UnreferencedCovergroup)
// Extracted from sample_tb.sv (Test Case 96)

`include "uvm_mock.svh"

// --- TEST CASE 96: SV-COV-001 (UnreferencedCovergroup) ---
class UnreferencedCovergroupBench;
    int payload_len;
    // VIOLATION SV-COV-001: Declared covergroup is never instantiated with new()!
    covergroup len_cg;
        coverpoint payload_len;
    endgroup
    function new();
        // Missing len_cg = new();
    endfunction
endclass
