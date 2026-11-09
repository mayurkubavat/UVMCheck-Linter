// Compliant Clean Test Case: SVA-VAC-001 (SvaVacuousAntecedent)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_svavacuousantecedent extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass

// `a && !b` is an ordinary antecedent; only a term and its own negation
// (`req && !req`) can never hold.
module sva_ordinary_antecedents(input logic clk, input logic valid, ready, req, ack);
    property p_stable_until_ready;
        @(posedge clk) (valid && !ready) |=> valid;
    endproperty
    assert property (p_stable_until_ready);
    assert property (@(posedge clk) (req && ~ack) |-> ##[1:4] ack);
endmodule
