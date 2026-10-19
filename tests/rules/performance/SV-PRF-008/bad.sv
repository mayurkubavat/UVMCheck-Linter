// Test Case: SV-PRF-008 (QueueSlicePopFirst)
// Extracted from sample_tb.sv (Test Case 85)

`include "uvm_mock.svh"

// --- TEST CASE 85: SV-PRF-008 (QueueSlicePopFirst) ---
class QueueSliceBench;
    int q[$];
    function void pop_item();
        // VIOLATION SV-PRF-008: Slicing head duplicates entire queue!
        q = q[1:$];
    endfunction
endclass
