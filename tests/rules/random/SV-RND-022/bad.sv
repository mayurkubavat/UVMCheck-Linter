// Test Case: SV-RND-022 (ConstraintCallingNonPureFunction)
// Extracted from sample_tb.sv (Test Case 217)

`include "uvm_mock.svh"

// --- TEST CASE 217: SV-RND-022 (ConstraintCallingNonPureFunction) ---
class ImpureConstraintBench;
    rand int data;
    function int display_and_get();
        $display("Impure side-effect!");
        return 10;
    endfunction : display_and_get

    constraint c_impure {
        // VIOLATION SV-RND-022: Calling function with side-effects in constraint!
        data == display_and_get();
    }
endclass : ImpureConstraintBench
