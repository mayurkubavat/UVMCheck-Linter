// Compliant Test Case: SV-DLY-004 (ZeroTimeForeverLoop)
`include "uvm_mock.svh"

class waiting_monitor extends uvm_component;
    `uvm_component_utils(waiting_monitor)
    int count;
    event sample_ev;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction : new

    task collect();
        #10;
    endtask : collect

    task run_phase(uvm_phase phase);
        fork
            forever begin
                @(sample_ev);          // waits on an event
                count++;
            end
            forever begin
                collect();             // a task call may block
            end
            forever begin
                count++;
                if (count > 100) break; // has a way out
            end
        join_none
    endtask : run_phase
endclass : waiting_monitor
