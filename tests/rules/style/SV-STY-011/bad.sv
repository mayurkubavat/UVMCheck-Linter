// Test Case: SV-STY-011 (OneStatementPerLine)
// Extracted from sample_tb.sv (Test Case 46)

`include "uvm_mock.svh"

// --- TEST CASE 46: SV-STY-011 (Multiple Procedural Statements on Same Line) ---
module single_stmt_per_line_tester;
    logic a, b, c;

    initial begin
        // VIOLATION SV-STY-011: Multiple statements on the same line separated by semicolon!
        a = 1; b = 0; c = 1;
    end
endmodule
