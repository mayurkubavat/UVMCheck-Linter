// Test Case: SV-ARR-001 (OffByOneQueueIndex)
// Extracted from sample_tb.sv (Test Case 62)

`include "uvm_mock.svh"

// --- TEST CASE 62: SV-ARR-001 (Off-by-One Queue Index) ---
class OffByOneQueueTester;
    int my_queue[$];
    function void test_access();
        int val;
        // VIOLATION SV-ARR-001: my_queue.size() is out-of-bounds off-by-one!
        val = my_queue[my_queue.size()];
    endfunction
endclass
