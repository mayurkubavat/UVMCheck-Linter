// Test Case: SV-STY-012 (SpaceKeywordParen)
// Extracted from sample_tb.sv (Test Case 47)

`include "uvm_mock.svh"

// --- TEST CASE 47: SV-STY-012 (Missing Space Between Control Keyword and Parenthesis) ---
module keyword_spacing_tester;
    logic ready, valid;

    initial begin
        // VIOLATION SV-STY-012: No space between 'if' and '('!
        if(ready && valid) begin
            $display("Ready and valid asserted");
        end

        // VIOLATION SV-STY-012: No space between 'for' and '('!
        for(int k = 0; k < 2; k++) begin
            $display("Iteration: %0d", k);
        end
    end
endmodule
