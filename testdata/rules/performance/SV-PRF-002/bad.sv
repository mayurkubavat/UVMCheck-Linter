// Test Case: SV-PRF-002 (ArrayReductionWidthTruncation)
// Extracted from sample_tb.sv (Test Case 23)

`include "uvm_mock.svh"

// --- TEST CASE 23: SV-PRF-002 (Array Reduction Width Truncation) ---
class ReductionTester extends uvm_object;
    `uvm_object_utils(ReductionTester)
    function new(string name = "ReductionTester");
        super.new(name);
    endfunction

    function int unsigned compute_total();
        byte unsigned payload[] = '{8'h80, 8'h80, 8'h10}; // Sum is 272 (0x110)
        int unsigned total;
        // VIOLATION SV-PRF-002: payload.sum() evaluates at 8-bit width, wrapping to 16 before 32-bit assignment!
        total = payload.sum();
        return total;
    endfunction
endclass
