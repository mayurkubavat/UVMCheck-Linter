// Test Case: SV-STY-016 (MaxLineLength)
// Extracted from sample_tb.sv (Test Case 122)

`include "uvm_mock.svh"

// --- TEST CASE 122: SV-STY-016 (MaxLineLength) ---
class LineLengthBench;
    // VIOLATION SV-STY-016: This single line contains an excessively long description comment that deliberately exceeds one hundred and twenty columns!
    int val = 1;
endclass
