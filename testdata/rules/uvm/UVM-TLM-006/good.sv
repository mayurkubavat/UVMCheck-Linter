// Test for UVM-TLM-006: Analysis FIFO properly drained in run_phase
`include "uvm_mock.svh"

class my_item extends uvm_sequence_item;
    `uvm_object_utils(my_item)
    function new(string name = ""); super.new(name); endfunction
endclass

class my_monitor extends uvm_component;
    `uvm_component_utils(my_monitor)
    uvm_analysis_port#(my_item) ap;
    function new(string name, uvm_component parent);
        super.new(name, parent);
        ap = new("ap", this);
    endfunction
endclass

class my_scoreboard extends uvm_component;
    `uvm_component_utils(my_scoreboard)
    uvm_tlm_analysis_fifo#(my_item) fifo;

    function new(string name, uvm_component parent);
        super.new(name, parent);
        fifo = new("fifo", this);
    endfunction

    virtual task run_phase(uvm_phase phase);
        my_item item;
        // Clean: FIFO is drained in run_phase
        void'(fifo.try_get(item));
    endtask
endclass

class my_env extends uvm_env;
    `uvm_component_utils(my_env)
    my_monitor mon;
    my_scoreboard scb;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    virtual function void connect_phase(uvm_phase phase);
        super.connect_phase(phase);
        mon.ap.connect(scb.fifo.analysis_export);
    endfunction
endclass
