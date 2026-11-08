// Test Case: SV-DED-008 (UnusedGenerateBlock)
// Extracted from sample_tb.sv (Test Case 158)

`include "uvm_mock.svh"

// --- TEST CASE 158: SV-DED-008 (UnusedGenerateBlock) ---
class DeadIfBench;
    function void test_dead_branch();
        // VIOLATION SV-DED-008: Constant false condition with no else clause!
        if (0) begin
            $display("Unreachable code");
        end
    endfunction
endclass
