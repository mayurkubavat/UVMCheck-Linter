// Test Case: SV-TYP-009 (ArrayLiteralSizeMismatch)
// Extracted from sample_tb.sv (Test Case 198)

`include "uvm_mock.svh"

// --- TEST CASE 198: SV-TYP-009 (ArrayLiteralSizeMismatch) ---
class ArrayLiteralSizeBench;
    int fixed_arr[4];
    function void init_array();
        // VIOLATION SV-TYP-009: Array literal provides 3 elements for 4-element fixed-size array!
        fixed_arr = '{10, 20, 30};
    endfunction : init_array
endclass : ArrayLiteralSizeBench
