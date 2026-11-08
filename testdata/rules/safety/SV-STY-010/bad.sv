// Test Case: SV-STY-010 (ExplicitLiteralWidth)
// Extracted from sample_tb.sv (Test Case 45)

`include "uvm_mock.svh"

// --- TEST CASE 45: SV-STY-010 (Unsized Numeric Literals) ---
module unsized_literal_tester;
    logic [7:0] data_byte;
    logic [15:0] word_data;

    initial begin
        // VIOLATION SV-STY-010: Unsized hex literal 'hFF missing explicit width!
        data_byte = 'hFF;
        // VIOLATION SV-STY-010: Unsized binary literal 'b1010 missing explicit width!
        data_byte = 'b1010;
        // Compliant: sized literal
        word_data = 16'hDEAD;
    end
endmodule
