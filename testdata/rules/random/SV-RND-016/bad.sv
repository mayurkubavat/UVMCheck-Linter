// Test Case: SV-RND-016 (ConcurrentRandomizeInFork)
// Extracted from sample_tb.sv (Test Case 162)

`include "uvm_mock.svh"

// --- TEST CASE 162: SV-RND-016 (ConcurrentRandomizeInFork) ---
class ConcurrentRandBench;
    ShadowItem shared_item;
    task test_concurrent_rand();
        shared_item = new();
        fork
            // VIOLATION SV-RND-016: Parallel threads randomizing the same object handle!
            shared_item.randomize();
            shared_item.randomize();
        join
    endtask
endclass
