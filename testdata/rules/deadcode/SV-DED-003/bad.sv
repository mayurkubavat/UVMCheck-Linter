// Test Case: SV-DED-003 (AccidentalEmptyConditional)
// Extracted from sample_tb.sv (Test Case 34)

`include "uvm_mock.svh"

// --- TEST CASE 34: SV-DED-003 (Accidental Empty Conditional Semicolon) ---
module accidental_empty_cond;
    logic checksum_valid;

    initial begin
        // VIOLATION SV-DED-003: Semicolon after if() terminates condition; begin..end block executes unconditionally!
        if (checksum_valid == 1'b0);
        begin
            $display("Checksum check failed!");
        end
    end
endmodule
