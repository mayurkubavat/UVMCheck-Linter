// Test Case: SV-PRF-010 (StringConcatInLoop)
// Extracted from sample_tb.sv (Test Case 173)

`include "uvm_mock.svh"

// --- TEST CASE 173: SV-PRF-010 (StringConcatInLoop) ---
class StringConcatLoopBench;
    function void dump_log();
        string log_buf = "";
        for (int i = 0; i < 10; i++) begin
            // VIOLATION SV-PRF-010: Iterative string concatenation inside loop!
            log_buf = {log_buf, "data\n"};
        end
    endfunction
endclass
