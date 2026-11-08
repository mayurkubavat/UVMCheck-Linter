// Test Case: SV-RND-010 (CyclicSolveBefore)
// Extracted from sample_tb.sv (Test Case 104)

`include "uvm_mock.svh"

// --- TEST CASE 104: SV-RND-010 (CyclicSolveBefore) ---
class CyclicSolveBench;
    rand int a;
    rand int b;
    constraint c_cycle {
        // VIOLATION SV-RND-010: Cyclic dependency in solve..before!
        solve a before b;
        solve b before a;
    }
endclass
