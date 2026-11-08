// UVM-PHS-011 (RunPhaseWithRuntimePhases): one schedule per component
`include "uvm_mock.svh"

class phs011_good_driver extends uvm_driver #(uvm_sequence_item);
    `uvm_component_utils(phs011_good_driver)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    task run_phase(uvm_phase phase);
        #10;
    endtask
endclass

class phs011_good_test extends uvm_test;
    `uvm_component_utils(phs011_good_test)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    task reset_phase(uvm_phase phase);
        #10;
    endtask
    task main_phase(uvm_phase phase);
        #10;
    endtask
endclass

// Not a component: method names alone do not matter.
class phs011_helper;
    task run_phase(int n); endtask
    task main_phase(int n); endtask
endclass
