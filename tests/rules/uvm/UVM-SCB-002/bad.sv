// Test Case: UVM-SCB-002 (ScoreboardLeftoverQueue)
// Extracted from sample_tb.sv (Test Case 64)

`include "uvm_mock.svh"

// --- TEST CASE 64: UVM-SCB-002 (Scoreboard Leftover Queue Check) ---
class DrainScoreboard extends uvm_scoreboard;
    `uvm_component_utils(DrainScoreboard)
    // VIOLATION UVM-SCB-002: expected_q is never verified for emptiness in check_phase or report_phase!
    int expected_q[$];

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    function void check_phase(uvm_phase phase);
        super.check_phase(phase);
        // Missing: expected_q.size() drain check!
    endfunction
endclass
