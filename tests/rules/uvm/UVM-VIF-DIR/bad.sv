// Test for UVM-VIF-DIR: Passive monitor procedurally driving virtual interface signal
`include "uvm_mock.svh"

class my_passive_monitor extends uvm_monitor;
    `uvm_component_utils(my_passive_monitor)
    virtual bus_if vif;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    virtual task run_phase(uvm_phase phase);
        // VIOLATION: Monitor should not drive interface signals!
        vif.ack = 1'b1;
    endtask
endclass
