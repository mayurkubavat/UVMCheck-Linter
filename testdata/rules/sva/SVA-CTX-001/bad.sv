// Test Case: SVA-CTX-001 (ConcurrentAssertInFunction)
// Extracted from sample_tb.sv (Test Case 98)

`include "uvm_mock.svh"

// --- TEST CASE 98: SVA-CTX-001 (ConcurrentAssertInFunction) ---
class AssertInFuncBench;
    function void evaluate_signals(logic clk, logic req, logic gnt);
        // VIOLATION SVA-CTX-001: Concurrent assertion inside function!
        assert property (@(posedge clk) req |-> ##1 gnt);
    endfunction
endclass
