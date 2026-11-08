// Test Case: SV-DED-009 (EmptyAlwaysOrInitial)
// Extracted from sample_tb.sv (Test Case 160)

`include "uvm_mock.svh"

// --- TEST CASE 160: SV-DED-009 (EmptyAlwaysOrInitial) ---
module empty_proc_mod;
    // VIOLATION SV-DED-009: initial block with empty body
    initial begin
    end
endmodule
