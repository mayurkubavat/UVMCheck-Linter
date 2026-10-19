// Test Case: SV-OOP-012 (ChainedAssignmentAliasing)
// Extracted from sample_tb.sv (Test Case 187)

`include "uvm_mock.svh"

// --- TEST CASE 187: SV-OOP-012 (ChainedAssignmentAliasing) ---
class ChainedAssignBench;
    BasePacket p1, p2, p3;
    function void alias_handles();
        p3 = new();
        // VIOLATION SV-OOP-012: Chained handle assignment creates subtle object aliasing!
        p1 = p2 = p3;
    endfunction
endclass
