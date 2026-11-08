// Test Case: SV-COR-005 (NonShortCircuitLogical)
// Extracted from sample_tb.sv (Test Case 149)

`include "uvm_mock.svh"

// --- TEST CASE 149: SV-COR-005 (NonShortCircuitLogical) ---
class NonShortCircuitBench;
    function bit check_valid(MockTransaction tr);
        // VIOLATION SV-COR-005: Bitwise '&' does not short-circuit, risking null pointer dereference!
        if (tr != null & tr.addr > 0) begin
            return 1;
        end
        return 0;
    endfunction
endclass
