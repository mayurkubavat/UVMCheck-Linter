// Violation Test Case: SV-CMP-001 (CompilerError) and SV-CMP-002 (CompilerWarning)
`include "uvm_mock.svh"

`define CMP_DEPTH 4
`define CMP_DEPTH 8                   // warning: macro redefined

class cmp_bad_item extends uvm_object;
    `uvm_object_utils(cmp_bad_item)

    localparam bit [3:0] SEED = 4'hA;
    int count;

    function new(string name = "cmp_bad_item");
        super.new(name);
    endfunction : new

    function int bump();
        count = undeclared_total + count;   // error: undeclared identifier
        return count;
    endfunction : bump

    function bit pick();
        return SEED[7];                     // error: constant index out of range
    endfunction : pick
endclass : cmp_bad_item
