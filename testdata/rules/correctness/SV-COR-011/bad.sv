// Test Case: SV-COR-011 (ModifyingLoopIndexInsideFor)
// Extracted from sample_tb.sv (Test Case 190)

`include "uvm_mock.svh"

// --- TEST CASE 190: SV-COR-011 (ModifyingLoopIndexInsideFor) ---
class LoopIndexModBench;
    function void iterate_items();
        for (int i = 0; i < 10; i++) begin
            if (i == 4) begin
                // VIOLATION SV-COR-011: Modifying loop index variable inside loop body!
                i++;
            end
        end
    endfunction
endclass
