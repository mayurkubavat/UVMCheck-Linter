// Test Case: SV-COR-020 (ProceduralAssignmentAsSubExpression)
// Extracted from sample_tb.sv (Test Case 220)

`include "uvm_mock.svh"

// --- TEST CASE 220: SV-COR-020 (ProceduralAssignmentAsSubExpression) ---
class EmbeddedAssignBench;
    int a, b, c;
    function void test_expr();
        // VIOLATION SV-COR-020: Assignment expression used as sub-expression!
        if ((a = b) > 0) begin
            c = (a = 1) + 2;
        end
    endfunction : test_expr
endclass : EmbeddedAssignBench
