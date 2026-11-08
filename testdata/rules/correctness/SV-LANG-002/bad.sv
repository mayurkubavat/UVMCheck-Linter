// Test Case: SV-LANG-002 (BitSelectOutOfBounds)
// Extracted from sample_tb.sv (Test Case 94)

`include "uvm_mock.svh"

// --- TEST CASE 94: SV-LANG-002 (BitSelectOutOfBounds) ---
class BitSelectOutOfBoundsBench;
    logic [31:0] packet_data;
    function void process_bits();
        // VIOLATION SV-LANG-002: Static bit-select [35] is outside declared [31:0]!
        logic bit_val;
        bit_val = packet_data[35];
    endfunction
endclass
