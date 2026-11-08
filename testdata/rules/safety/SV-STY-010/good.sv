// Compliant Clean Test Case: SV-STY-010 (ExplicitLiteralWidth)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_explicitliteralwidth extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass

// Compliant: a number from `__LINE__ is not a literal anyone wrote.
`define LINE_OF_CALL `__LINE__
class line_user;
    function int mark();
        return `LINE_OF_CALL;
    endfunction : mark
endclass : line_user
