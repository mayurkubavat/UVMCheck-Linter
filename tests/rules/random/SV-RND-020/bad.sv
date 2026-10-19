// Test Case: SV-RND-020 (RandcInDynamicContainer)
// Extracted from sample_tb.sv (Test Case 203)

`include "uvm_mock.svh"

// --- TEST CASE 203: SV-RND-020 (RandcInDynamicContainer) ---
class RandcDynContainerBench;
    // VIOLATION SV-RND-020: randc on dynamic array container!
    randc bit [7:0] dyn_payload[];
    // VIOLATION SV-RND-020: randc on queue container!
    randc bit [7:0] q_payload[$];
endclass : RandcDynContainerBench
