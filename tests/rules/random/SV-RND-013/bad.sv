// Test Case: SV-RND-013 (UnrestoredRandMode)
// Extracted from sample_tb.sv (Test Case 141)

`include "uvm_mock.svh"

// --- TEST CASE 141: SV-RND-013 (UnrestoredRandMode) ---
class RandModeBench;
    task test_stimulus();
        MockTransaction tr = new();
        // VIOLATION SV-RND-013: rand_mode(0) called without restoring to 1!
        tr.rand_mode(0);
    endtask
endclass
