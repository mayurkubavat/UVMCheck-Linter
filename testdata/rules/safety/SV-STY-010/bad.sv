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

// VIOLATION SV-STY-010: the default 'hF is written code, though every instance
// overrides it (reported once).
module literal_width_leaf #(parameter logic [7:0] MASK = 'hF) (output logic [7:0] y);
    assign y = MASK;
endmodule

module literal_width_top;
    logic [7:0] a, b;
    literal_width_leaf #(.MASK(8'h01)) u_a (.y(a));
    literal_width_leaf #(.MASK(8'h02)) u_b (.y(b));
endmodule
