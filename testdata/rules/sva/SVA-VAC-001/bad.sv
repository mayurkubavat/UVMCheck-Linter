// Test Case: SVA-VAC-001 (SvaVacuousAntecedent)
// Extracted from sample_tb.sv (Test Case 74)

`include "uvm_mock.svh"

// --- TEST CASE 74: SVA-VAC-001 (Vacuous Implication with Constant-False Antecedent) ---
module sva_vacuous_tester(input logic clk, input logic ack);
    // VIOLATION SVA-VAC-001: Constant-false antecedent (0 |-> ...) causes vacuous 100% pass!
    property p_vacuous_pass;
        @(posedge clk) 0 |-> ack;
    endproperty
    assert property (p_vacuous_pass);
endmodule

module sva_contradiction_tester(input logic clk, input logic req, input logic ack);
    // VIOLATION SVA-VAC-001: a term and its negation, never true.
    assert property (@(posedge clk) (req && !(req)) |-> ack);
endmodule
