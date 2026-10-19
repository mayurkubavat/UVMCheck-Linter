// Test Case: SV-COR-006 (CaseWithoutDefault)
// Extracted from sample_tb.sv (Test Case 155)

`include "uvm_mock.svh"

// --- TEST CASE 155: SV-COR-006 (CaseWithoutDefault) ---
class CaseDefaultBench;
    function void decode_state(int state);
        // VIOLATION SV-COR-006: case statement lacks default fallback branch!
        case (state)
            0: $display("Idle");
            1: $display("Active");
        endcase
    endfunction
endclass
