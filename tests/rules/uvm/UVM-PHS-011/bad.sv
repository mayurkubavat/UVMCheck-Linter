// UVM-PHS-011 (RunPhaseWithRuntimePhases): run_phase alongside runtime sub-phases
`include "uvm_mock.svh"

class phs011_driver extends uvm_driver #(uvm_sequence_item);
    `uvm_component_utils(phs011_driver)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    task reset_phase(uvm_phase phase);
        #10;
    endtask
    task run_phase(uvm_phase phase);
        #10;
    endtask
    task main_phase(uvm_phase phase);
        #10;
    endtask
endclass
