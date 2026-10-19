// Test for RAL-WIRE-001: Register predictor map and adapter fully bound in connect_phase
`include "uvm_mock.svh"

class my_env extends uvm_env;
    `uvm_component_utils(my_env)

    uvm_reg_block reg_model;
    uvm_reg_predictor#(int) predictor;
    uvm_reg_adapter adapter;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    virtual function void connect_phase(uvm_phase phase);
        super.connect_phase(phase);
        reg_model.default_map.set_sequencer(null, adapter);
        predictor.map = reg_model.default_map; // Clean: map explicitly bound
    endfunction
endclass
