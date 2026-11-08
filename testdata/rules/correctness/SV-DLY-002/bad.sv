// Test Case: SV-DLY-002 (IntraAssignmentDelay)
// Extracted from sample_tb.sv (Test Case 131)

`include "uvm_mock.svh"

// --- TEST CASE 131: SV-DLY-002 (IntraAssignmentDelay) ---
class IntraDelayBench;
    int a, b;
    task run_pipeline();
        // VIOLATION SV-DLY-002: Intra-assignment delay on blocking assignment!
        a = #5 b;
    endtask
endclass
