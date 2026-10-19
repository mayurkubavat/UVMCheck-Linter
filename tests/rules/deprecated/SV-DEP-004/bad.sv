// Test Case: SV-DEP-004 (RandomSystemFunction)
// Extracted from sample_tb.sv (Test Case 51)

`include "uvm_mock.svh"

// --- TEST CASE 51: SV-DEP-004 (Legacy $random System Function) ---
module legacy_random_tester;
    int rand_val;
    initial begin
        // VIOLATION SV-DEP-004: $random is superseded by $urandom and $urandom_range!
        rand_val = $random % 10;
    end
endmodule
