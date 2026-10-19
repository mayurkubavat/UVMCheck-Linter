// Test Case: SV-PRF-006 (StringKeyAssociativeArray)
// Extracted from sample_tb.sv (Test Case 116)

`include "uvm_mock.svh"

// --- TEST CASE 116: SV-PRF-006 (StringKeyAssociativeArray) ---
class StringKeyBench;
    // VIOLATION SV-PRF-006: Associative array indexed by string!
    int packet_counter[string];
endclass
