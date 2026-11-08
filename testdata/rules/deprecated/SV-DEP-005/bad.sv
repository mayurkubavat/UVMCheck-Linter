// Test Case: SV-DEP-005 (ProceduralContinuousAssign)
// Extracted from sample_tb.sv (Test Case 52)

`include "uvm_mock.svh"

// --- TEST CASE 52: SV-DEP-005 (Procedural Continuous Assign) ---
module proc_assign_tester;
    logic sig;
    initial begin
        // VIOLATION SV-DEP-005: Procedural continuous assignment is non-synthesizable and deprecated!
        assign sig = 1;
    end
endmodule
