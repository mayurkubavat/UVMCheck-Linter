// Test for UVM-OBJ-004: Orphan objection in fork..join_none background process
`include "uvm_mock.svh"

class my_sequence extends uvm_sequence;
    `uvm_object_utils(my_sequence)

    uvm_phase starting_phase;

    function new(string name = "my_sequence");
        super.new(name);
    endfunction

    virtual task body();
        fork
            // VIOLATION: Background worker raises objection but never drops it!
            begin
                starting_phase.raise_objection(this);
                #100;
            end
        join_none
    endtask
endclass
