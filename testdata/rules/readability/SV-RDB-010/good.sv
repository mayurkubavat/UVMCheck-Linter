// SV-RDB-010 (UndocumentedTestbenchClass): documented classes; plain objects need nothing
`include "uvm_mock.svh"

// Reorders out-of-order completions by tag and compares each payload against the
// expected DMA read, reporting any completion that never arrives.
class rdb010_good_scb extends uvm_scoreboard;
    `uvm_component_utils(rdb010_good_scb)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
endclass

/* Writes a burst to every mapped region, then reads it back to prove the
   address decode covers the whole map without holes or aliases. */
class rdb010_good_seq extends uvm_sequence #(uvm_sequence_item);
    function new(string name = "rdb010_good_seq");
        super.new(name);
    endfunction
endclass

class rdb010_helper;
endclass
