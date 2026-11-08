// Test Case: SV-STY-015 (TrailingWhitespace)
// Extracted from sample_tb.sv (Test Case 118)

`include "uvm_mock.svh"

// --- TEST CASE 118: SV-STY-015 (TrailingWhitespace) ---
class TrailingWhitespaceBench;
    // VIOLATION SV-STY-015: Trailing whitespace at end of line!    
    int dummy_val = 42;
endclass
