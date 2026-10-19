// Test Case: SV-LANG-003 (UnsizedLiteralInConcat)
// Extracted from sample_tb.sv (Test Case 101)

`include "uvm_mock.svh"

// --- TEST CASE 101: SV-LANG-003 (UnsizedLiteralInConcat) ---
class UnsizedConcatBench;
    function logic [33:0] pack_header(logic [31:0] payload);
        // VIOLATION SV-LANG-003: Unsized 1 and 0 expand to 32 bits each, causing 64+32 bit vector!
        return {1, 0, payload};
    endfunction
endclass
