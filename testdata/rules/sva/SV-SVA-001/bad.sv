// Test Case: SV-SVA-001 (ImmediateAssertInCombinatorialLogic)
// Extracted from sample_tb.sv (Test Case 27)

`include "uvm_mock.svh"

// --- TEST CASE 27: SV-SVA-001 (Immediate Assertion in Combinatorial Logic) ---
module combo_assertion_checker;
    logic a, b;
    logic out;

    // VIOLATION SV-SVA-001: Simple immediate assertion evaluates transient delta glitches in always_comb!
    always_comb begin
        out = a ^ b;
        assert (out != 1'b0) else $error("Transient delta glitch false-failure!");
    end
endmodule
