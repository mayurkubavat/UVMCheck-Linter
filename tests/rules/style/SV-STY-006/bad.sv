// Test Case: SV-STY-006 (MismatchedEndLabel)
// Extracted from sample_tb.sv (Test Case 42)

`include "uvm_mock.svh"

// --- TEST CASE 42: SV-STY-006 (Mismatched End Label) ---
class EndLabelTester extends uvm_object;
    `uvm_object_utils(EndLabelTester)
    function new(string name = "EndLabelTester");
        super.new(name);
    endfunction

    // VIOLATION SV-STY-006: End label 'wrong_routine_name' does not match routine 'execute_task'!
    function void execute_task();
        $display("Executing task");
    endfunction : wrong_routine_name
endclass : EndLabelTester
