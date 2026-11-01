// Violation Test Case: SV-DLY-004 (ZeroTimeForeverLoop)
`include "uvm_mock.svh"

class spin_monitor extends uvm_component;
    `uvm_component_utils(spin_monitor)
    int count;
    bit done;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction : new

    task run_phase(uvm_phase phase);
        forever begin                  // VIOLATION: nothing in the body lets time advance
            count++;
            if (count > 10) done = 1;
        end
    endtask : run_phase
endclass : spin_monitor
