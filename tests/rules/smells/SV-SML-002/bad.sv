// Test Case: SV-SML-002 (MagicDelayInSequence)
// Extracted from sample_tb.sv (Test Case 89)

`include "uvm_mock.svh"

// --- TEST CASE 89: SV-SML-002 (MagicDelayInSequence) ---
class MagicDelaySequence extends uvm_sequence;
    `uvm_object_utils(MagicDelaySequence)
    function new(string name = "MagicDelaySequence");
        super.new(name);
    endfunction
    task body();
        // VIOLATION SV-SML-002: Hardcoded physical delay inside sequence!
        #100;
    endtask
endclass
