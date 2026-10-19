// Test Case: SV-COR-007 (AssignmentInExpression)
// Extracted from sample_tb.sv (Test Case 167)

`include "uvm_mock.svh"

// --- TEST CASE 167: SV-COR-007 (AssignmentInExpression) ---
class AssignmentInConditionBench;
    function void check_cond(int a, int b);
        // VIOLATION SV-COR-007: Assignment '=' used inside if predicate!
        if (a = b) begin
            $display("Equal");
        end
    endfunction
endclass
