// UVM-OBJ-005 (ObjectionWithoutDescription): objections that do not say what they wait for
`include "uvm_mock.svh"

class obj005_test extends uvm_test;
    `uvm_component_utils(obj005_test)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    task run_phase(uvm_phase phase);
        phase.raise_objection(this);
        #100;
        phase.drop_objection(this, "");
    endtask
endclass
