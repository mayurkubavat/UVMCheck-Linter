// Test Case: SV-RND-001 (UncheckedRandomize)
// Extracted from sample_tb.sv (Test Case 18)

`include "uvm_mock.svh"

class CrcCalculatorAlpha;
    rand int data;
endclass

// --- TEST CASE 18: SV-RND-001 (Unchecked Randomize) ---
class UncheckedRandomizeSequence extends uvm_sequence;
    `uvm_object_utils(UncheckedRandomizeSequence)
    function new(string name = "UncheckedRandomizeSequence");
        super.new(name);
    endfunction

    task body();
        CrcCalculatorAlpha item;
        item = new();
        // VIOLATION SV-RND-001: Discarding return value of randomize()!
        void'(item.randomize());
    endtask
endclass
