// Compliant Test Case: UVM-REP-009 (InfoAtNoneVerbosity)
`include "uvm_mock.svh"

class chatty_driver extends uvm_component;
    `uvm_component_utils(chatty_driver)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction : new
    task run_phase(uvm_phase phase);
        `uvm_info("DRV/BEAT", "driving beat", UVM_HIGH)
    endtask : run_phase
endclass : chatty_driver
