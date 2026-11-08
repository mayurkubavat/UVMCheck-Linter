// Test Case: SV-COV-015 (MissingIffOnCovergroupClock)
// Extracted from sample_tb.sv (Test Case 211)

`include "uvm_mock.svh"

// --- TEST CASE 211: SV-COV-015 (MissingIffOnCovergroupClock) ---
class CovergroupClockBench;
    bit [7:0] data;
    bit clk;
    // VIOLATION SV-COV-015: Covergroup sampled on clock event without iff reset guard!
    covergroup cg_clocked @(posedge clk);
        cp_d: coverpoint data;
    endgroup : cg_clocked
    function new();
        cg_clocked = new();
    endfunction : new
endclass : CovergroupClockBench
