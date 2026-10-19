// Test Case: SV-COV-012 (WideCoverpointWithoutBins)
// Extracted from sample_tb.sv (Test Case 188)

`include "uvm_mock.svh"

// --- TEST CASE 188: SV-COV-012 (WideCoverpointWithoutBins) ---
class WideAutoBinBench;
    bit [15:0] packet_size;
    covergroup cg_wide_bins;
        // VIOLATION SV-COV-012: Wide 16-bit coverpoint without bins or auto_bin_max defaults to 64 bins!
        cp_size: coverpoint packet_size;
    endgroup
    function new();
        cg_wide_bins = new();
    endfunction
endclass
