// Compliant Clean Test Case: SV-INT-001 (BlockingAssignmentToInterface)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_blockingassignmenttointerface extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass

// A continuous assignment to an interface signal is not a procedural drive:
// there is no race to avoid, and 'assign x <= y' would not compile.
interface blk_if;
    logic r;
endinterface

module blk_top;
    blk_if vif();
    logic rst_n;
    assign vif.r = ~rst_n;
endmodule
