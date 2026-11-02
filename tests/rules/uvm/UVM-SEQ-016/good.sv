// UVM-SEQ-016 (SequencerFromConfigDb): other config_db reads in a sequence are fine
`include "uvm_mock.svh"

class seq016_good_cfg extends uvm_object;
    int unsigned n_items = 4;
    function new(string name = "seq016_good_cfg");
        super.new(name);
    endfunction
endclass

class seq016_good_seq extends uvm_sequence #(uvm_sequence_item);
    seq016_good_cfg cfg;
    function new(string name = "seq016_good_seq");
        super.new(name);
    endfunction
    task body();
        if (!uvm_config_db#(seq016_good_cfg)::get(null, get_full_name(), "cfg", cfg))
            $fatal(1, "no config");
    endtask
endclass
