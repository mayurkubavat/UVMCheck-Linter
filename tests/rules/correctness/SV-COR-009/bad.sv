// Test Case: SV-COR-009 (CasexProhibited)
// Extracted from sample_tb.sv (Test Case 181)

`include "uvm_mock.svh"

// --- TEST CASE 181: SV-COR-009 (CasexProhibited) ---
module casex_violation_mod;
    logic [3:0] sel;
    logic [1:0] out;
    always_comb begin
        // VIOLATION SV-COR-009: casex treats unknown 'x' as wildcards, masking simulation bugs!
        casex (sel)
            4'b1xx0: out = 2'b00;
            4'b01xx: out = 2'b01;
            default: out = 2'b10;
        endcase
    end
endmodule
