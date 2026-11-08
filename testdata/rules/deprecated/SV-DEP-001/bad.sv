// Test Case: SV-DEP-001 (ProgramBlockDeprecation)
// Extracted from sample_tb.sv (Test Case 48)

`include "uvm_mock.svh"

// --- TEST CASE 48: SV-DEP-001 (Program Block Deprecation) ---
// VIOLATION SV-DEP-001: Program block is obsolete and deprecated in modern SystemVerilog/UVM!
program test_program_block;
    initial begin
        $display("Program block executing");
    end
endprogram
