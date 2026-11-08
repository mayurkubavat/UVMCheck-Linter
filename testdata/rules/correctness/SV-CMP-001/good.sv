// Compliant Test Case: SV-CMP-001 (CompilerError) and SV-CMP-002 (CompilerWarning)
`include "uvm_mock.svh"

`define CMP_DEPTH 4

class cmp_good_item extends uvm_object;
    `uvm_object_utils(cmp_good_item)

    localparam bit [3:0] SEED = 4'hA;
    int count;

    function new(string name = "cmp_good_item");
        super.new(name);
    endfunction : new

    function int bump();
        count = SEED + count;
        return count;
    endfunction : bump

    function bit pick();
        return SEED[3];
    endfunction : pick
endclass : cmp_good_item
