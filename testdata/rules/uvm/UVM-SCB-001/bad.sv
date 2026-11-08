// Test Case: UVM-SCB-001 (ScoreboardSilentDrop)
// Extracted from sample_tb.sv (Test Case 63)

`include "uvm_mock.svh"

// --- TEST CASE 63: UVM-SCB-001 (Scoreboard Silent Drop) ---
class DropScoreboard extends uvm_scoreboard;
    `uvm_component_utils(DropScoreboard)
    int expected_q[$];

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    function void write_actual(int act);
        // VIOLATION UVM-SCB-001: Silent return on empty expected queue without reporting an error!
        if (expected_q.size() == 0) return;
    endfunction
endclass
