// Test Case: SV-COR-008 (SelfComparison)
// Extracted from sample_tb.sv (Test Case 172)

`include "uvm_mock.svh"

// --- TEST CASE 172: SV-COR-008 (SelfComparison) ---
class SelfCompBench;
    function void test_comp(int pkt_id);
        // VIOLATION SV-COR-008: Variable compared to itself!
        if (pkt_id == pkt_id) begin
            $display("Self comparison tautology");
        end
    endfunction
endclass
