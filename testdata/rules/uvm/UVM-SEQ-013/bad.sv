// Test for UVM-SEQ-013: Driver and sequencer parameterized with mismatched sequence item types
`include "uvm_mock.svh"

class pkt_ahb extends uvm_sequence_item;
    `uvm_object_utils(pkt_ahb)
    function new(string name = ""); super.new(name); endfunction
endclass

class pkt_axi extends uvm_sequence_item;
    `uvm_object_utils(pkt_axi)
    function new(string name = ""); super.new(name); endfunction
endclass

class my_driver extends uvm_driver#(pkt_ahb);
    uvm_seq_item_pull_port#(pkt_ahb) seq_item_port;
    function new(string name, uvm_component parent);
        super.new(name, parent);
        seq_item_port = new("seq_item_port", this);
    endfunction
endclass

class my_sequencer extends uvm_sequencer#(pkt_axi);
    uvm_seq_item_pull_export#(pkt_axi) seq_item_export;
    function new(string name, uvm_component parent);
        super.new(name, parent);
        seq_item_export = new("seq_item_export", this);
    endfunction
endclass

class my_agent extends uvm_component;
    `uvm_component_utils(my_agent)
    my_driver driver;
    my_sequencer sequencer;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    virtual function void connect_phase(uvm_phase phase);
        super.connect_phase(phase);
        // VIOLATION: driver expects pkt_ahb, but sequencer provides pkt_axi
        driver.seq_item_port.connect(sequencer.seq_item_export);
    endfunction
endclass
