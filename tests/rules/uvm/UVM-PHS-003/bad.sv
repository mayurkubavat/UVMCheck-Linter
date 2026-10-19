// Test Case: UVM-PHS-003 (UvmPhaseSignatureMismatch)
// Extracted from sample_tb.sv (Test Case 9)

`include "uvm_mock.svh"

// --- TEST CASE 9: UVM-PHS-003 (Phase Signature Mismatch) ---
class BadPhaseComponent extends uvm_component;
    `uvm_component_utils(BadPhaseComponent)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    // VIOLATION UVM-PHS-003: build_phase declared as task instead of function void
    task build_phase(uvm_phase phase);
    endtask

    // VIOLATION UVM-PHS-003: run_phase declared as function instead of task
    function void run_phase(uvm_phase phase);
    endfunction
endclass
