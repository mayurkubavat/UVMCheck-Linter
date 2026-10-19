// Test Case: SV-DEP-003 (LegacyAlwaysBlock)
// Extracted from sample_tb.sv (Test Case 50)

`include "uvm_mock.svh"

// --- TEST CASE 50: SV-DEP-003 (Legacy Always Block) ---
module legacy_always_tester;
    logic a, b, c;
    // VIOLATION SV-DEP-003: Legacy untyped always block should be always_comb!
    always @(*) begin
        c = a & b;
    end
endmodule
