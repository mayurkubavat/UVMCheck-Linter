// Test Case: UVM-PHS-007 (UvmPhaseJumpHazard)
// Extracted from sample_tb.sv (Test Case 150)

`include "uvm_mock.svh"

// --- TEST CASE 150: UVM-PHS-007 (UvmPhaseJumpHazard) ---
class PhaseJumpBench extends uvm_component;
    `uvm_component_utils(PhaseJumpBench)
    function new(string name = "PhaseJumpBench", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    task run_phase(uvm_phase phase);
        // VIOLATION UVM-PHS-007: Manual phase jump causes objection leaks and desynchronization!
        phase.jump(uvm_pre_reset_phase::get());
    endtask
endclass
