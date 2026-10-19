// Test Case: SV-TYP-001 (MixedSignComparison)
// Extracted from sample_tb.sv (Test Case 20)

`include "uvm_mock.svh"

// --- TEST CASE 20: SV-TYP-001 (Mixed Signed/Unsigned Comparison) ---
class MixedSignTester extends uvm_object;
    `uvm_object_utils(MixedSignTester)
    function new(string name = "MixedSignTester");
        super.new(name);
    endfunction

    function void check_bounds();
        int signed   delta = -5;
        int unsigned limit = 10;
        // VIOLATION SV-TYP-001: delta is coerced to unsigned (4294967291) resulting in logic failure!
        if (delta < limit) begin
            $display("Within limit");
        end
    endfunction
endclass
