// Test Case: SV-PRF-007 (QueueConcatAppend)
// Extracted from sample_tb.sv (Test Case 84)

`include "uvm_mock.svh"

// --- TEST CASE 84: SV-PRF-007 (QueueConcatAppend) ---
class QueueConcatBench;
    int q[$];
    function void append_items(int data_in);
        // VIOLATION SV-PRF-007: Concatenation forces O(N) reallocation!
        q = {q, data_in};
    endfunction
endclass
