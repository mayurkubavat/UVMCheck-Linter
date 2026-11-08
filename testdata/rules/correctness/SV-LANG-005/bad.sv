// Test Case: SV-LANG-005 (NonBlockingInFunction)
// Extracted from sample_tb.sv (Test Case 111)

`include "uvm_mock.svh"

// --- TEST CASE 111: SV-LANG-005 (NonBlockingInFunction) ---
class NonBlockingFuncBench;
    logic [7:0] crc;
    function void update_crc(logic [7:0] byte_in);
        // VIOLATION SV-LANG-005: Non-blocking assignment in function!
        crc <= byte_in ^ 8'h55;
    endfunction
endclass
