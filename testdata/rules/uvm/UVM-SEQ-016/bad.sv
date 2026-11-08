// UVM-SEQ-016 (SequencerFromConfigDb): sequences fetching sequencer handles
`include "uvm_mock.svh"

class seq016_sqr extends uvm_sequencer #(uvm_sequence_item);
    `uvm_component_utils(seq016_sqr)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
endclass

class seq016_vseq extends uvm_sequence #(uvm_sequence_item);
    seq016_sqr axi_sqr;
    function new(string name = "seq016_vseq");
        super.new(name);
    endfunction
    task body();
        if (!uvm_config_db#(seq016_sqr)::get(null, "*.env.axi_agt", "sqr", axi_sqr))
            $fatal(1, "no sequencer");
    endtask
endclass
