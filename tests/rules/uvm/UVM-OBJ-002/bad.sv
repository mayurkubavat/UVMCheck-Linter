// Test Case: UVM-OBJ-002 (UvmObjectionInLoop)
// Extracted from sample_tb.sv (Test Case 7)

`include "uvm_mock.svh"

// --- TEST CASE 7: UVM-OBJ-002 (Objection raised/dropped inside loop) ---
class LoopObjectionDriver extends uvm_component;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    task run_phase(uvm_phase phase);
        // VIOLATION UVM-OBJ-002: raising and dropping objections inside high-frequency loop
        for (int i = 0; i < 5; i++) begin
            phase.raise_objection(this);
            #1;
            phase.drop_objection(this);
        end

        // Compliant pattern: raise once before loop, drop once after
        phase.raise_objection(this);
        for (int i = 0; i < 5; i++) begin
            #1;
        end
        phase.drop_objection(this);
    endtask
endclass
