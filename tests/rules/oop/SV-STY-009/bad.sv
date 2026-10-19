// Test Case: SV-STY-009 (ExternMethodThreshold)
// Extracted from sample_tb.sv (Test Case 44)

`include "uvm_mock.svh"

// --- TEST CASE 44: SV-STY-009 (Extern Method Threshold Exceeded) ---
class ExternThresholdTester extends uvm_component;
    `uvm_component_utils(ExternThresholdTester)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    // VIOLATION SV-STY-009: 8-line method declared inline inside class instead of using 'extern'!
    function void long_inline_method();
        int step1 = 1;
        int step2 = 2;
        int step3 = 3;
        int step4 = 4;
        int step5 = 5;
        $display("Sum: %0d", step1 + step2 + step3 + step4 + step5);
    endfunction : long_inline_method
endclass : ExternThresholdTester
