// Test for UVM-OBJ-004: Clean background process with matching drop_objection
`include "uvm_mock.svh"

class my_sequence extends uvm_sequence;
    `uvm_object_utils(my_sequence)

    uvm_phase starting_phase;

    function new(string name = "my_sequence");
        super.new(name);
    endfunction

    virtual task body();
        fork
            // Clean: Background worker pairs raise with drop
            begin
                starting_phase.raise_objection(this);
                #100;
                starting_phase.drop_objection(this);
            end
        join_none
    endtask
endclass
