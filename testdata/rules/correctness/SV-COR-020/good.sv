// Compliant Clean Test Case: SV-COR-020 (ProceduralAssignmentAsSubExpression)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_proceduralassignmentassubexpression extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass

// Continuous assignments are standalone assignments, one or several per
// statement.
module cor020_assign(input logic a, output logic b, c, d);
    assign b = a;
    assign c = a, d = ~a;
endmodule : cor020_assign
