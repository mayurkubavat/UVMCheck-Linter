// SV-RDB-010 (UndocumentedTestbenchClass): components and sequences without a description
`include "uvm_mock.svh"

// Copyright 2026 Example Corp. All rights reserved.
class rdb010_scb extends uvm_scoreboard;
    `uvm_component_utils(rdb010_scb)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
endclass

class rdb010_seq extends uvm_sequence #(uvm_sequence_item);
    function new(string name = "rdb010_seq");
        super.new(name);
    endfunction
endclass
