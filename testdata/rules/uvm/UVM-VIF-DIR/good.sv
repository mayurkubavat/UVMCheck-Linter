// Test for UVM-VIF-DIR: Passive monitor only sampling virtual interface signals
`include "uvm_mock.svh"

class my_passive_monitor extends uvm_monitor;
    `uvm_component_utils(my_passive_monitor)
    virtual bus_if vif;
    bit sample_val;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    virtual task run_phase(uvm_phase phase);
        // Clean: only reads/samples vif signals into local state
        sample_val = vif.ack;
    endtask
endclass
